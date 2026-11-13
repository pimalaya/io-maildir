#![cfg(feature = "client")]

use std::{fs, path::Path};

use io_maildir::{
    client::MaildirClient,
    flag::{MaildirFlag, MaildirFlags},
    maildir::MaildirSubdir,
};
use tempfile::tempdir;

fn maildir_at(path: &Path) {
    for subdir in ["cur", "new", "tmp"] {
        fs::create_dir_all(path.join(subdir)).unwrap();
    }
}

fn client(root: &Path) -> MaildirClient {
    MaildirClient::new(root.to_string_lossy().into_owned())
}

fn names(client: &MaildirClient) -> Vec<String> {
    client
        .list_maildirs()
        .unwrap()
        .iter()
        .map(|maildir| {
            let path = maildir.path().as_str();
            let root = client.store.root.as_str();
            path.strip_prefix(root)
                .unwrap()
                .trim_start_matches('/')
                .to_owned()
        })
        .collect()
}

#[test]
fn deleting_or_renaming_the_root_leaves_the_store_whole() {
    for maildirpp in [false, true] {
        let dir = tempdir().unwrap();
        let root = dir.path().join("store");
        maildir_at(&root);
        maildir_at(&root.join("Work"));
        maildir_at(&root.join(".Sent"));
        fs::write(root.join("cur/1.host:2,S"), b"Subject: a\r\n\r\n").unwrap();

        let mut client = client(&root);
        client.store.maildirpp = maildirpp;

        for name in ["", ".", "./", "Work/.."] {
            assert!(client.delete_maildir(name).is_err(), "{name}");
            assert!(client.rename_maildir(name, "Moved").is_err(), "{name}");
        }

        assert!(root.join("cur/1.host:2,S").is_file());
        assert!(root.join("Work/cur").is_dir());
        assert!(root.join(".Sent/cur").is_dir());
        assert!(dir.path().join("store").is_dir());
    }
}

#[test]
fn a_copy_names_its_keywords_through_the_target_table() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    maildir_at(&root.join("A"));
    maildir_at(&root.join("B"));
    fs::write(
        root.join("A/dovecot-keywords"),
        "0 $Important\n1 $Junk\n2 Project X\n",
    )
    .unwrap();
    fs::write(root.join("B/dovecot-keywords"), "0 $Junk\n").unwrap();
    fs::write(root.join("A/cur/1.host:2,FSac"), b"Subject: a\r\n\r\n").unwrap();

    let mut client = client(root);
    client.dovecot_keywords = true;
    let a = client.load_maildir("A").unwrap();
    let b = client.load_maildir("B").unwrap();

    let id = client.copy("1.host", a.clone(), b.clone(), None).unwrap();
    let copy = client.get(b.clone(), &id).unwrap();
    let mut flags: Vec<MaildirFlag> = copy.flags().iter().cloned().collect();
    flags.sort();
    let expected = [
        MaildirFlag::Flagged,
        MaildirFlag::Seen,
        MaildirFlag::keyword("$Important"),
        MaildirFlag::keyword("Project X"),
    ];
    assert_eq!(flags, expected);

    let id = client.r#move("1.host", a, b.clone(), None).unwrap();
    let moved = client.get(b, &id).unwrap();
    assert_eq!(moved.flags(), copy.flags());

    let table = fs::read_to_string(root.join("B/dovecot-keywords")).unwrap();
    assert!(table.contains("0 $Junk"), "{table}");
    assert!(
        table.contains("$Important") && table.contains("Project X"),
        "{table}"
    );
}

#[test]
fn a_copy_without_dovecot_keywords_keeps_the_letters() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    maildir_at(&root.join("A"));
    maildir_at(&root.join("B"));
    fs::write(root.join("A/cur/1.host:2,Sa"), b"Subject: a\r\n\r\n").unwrap();

    let client = client(root);
    let a = client.load_maildir("A").unwrap();
    let b = client.load_maildir("B").unwrap();

    let id = client
        .copy("1.host", a, b, Some(MaildirSubdir::Cur))
        .unwrap();
    assert!(root.join(format!("B/cur/{id}:2,Sa")).is_file());
}

#[cfg(unix)]
#[test]
fn a_symlink_back_to_an_ancestor_lists_nothing_twice() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("store");
    maildir_at(&root.join("Work"));
    std::os::unix::fs::symlink(&root, root.join("Loop")).unwrap();
    std::os::unix::fs::symlink(root.join("Work"), root.join("Work/Again")).unwrap();

    assert_eq!(names(&client(&root)), ["Work"]);
}

#[cfg(unix)]
#[test]
fn an_unreadable_folder_is_skipped_and_an_unreadable_entry_named() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().unwrap();
    let root = dir.path();
    maildir_at(&root.join("A"));
    maildir_at(&root.join("Locked/Inner"));
    fs::write(root.join("A/cur/1.host:2,S"), b"Subject: a\r\n\r\n").unwrap();
    fs::write(root.join("A/cur/2.host:2,S"), b"Subject: b\r\n\r\n").unwrap();
    let locked = root.join("Locked");
    let entry = root.join("A/cur/2.host:2,S");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    fs::set_permissions(&entry, fs::Permissions::from_mode(0o000)).unwrap();

    // NOTE: root reads through any mode, so the checks only hold for
    // an ordinary user.
    if fs::read(&entry).is_ok() {
        return;
    }

    let client = client(root);
    let listed = names(&client);
    let a = client.load_maildir("A").unwrap();
    let entries: Vec<_> = client
        .list_entries(a.clone())
        .unwrap()
        .into_iter()
        .collect();
    let readable = client.read_readable_entries(&a, &entries);
    let err = client.read_entries(&a, &entries).unwrap_err().to_string();

    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
    fs::set_permissions(&entry, fs::Permissions::from_mode(0o644)).unwrap();

    assert_eq!(listed, ["A"]);
    assert_eq!(readable.len(), 1);
    assert!(err.contains("2.host:2,S"), "{err}");
}

#[test]
fn a_flag_write_keeps_an_experimental_info_section() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    maildir_at(root);
    let name = "1.host:1,experimental";
    fs::write(root.join("cur").join(name), b"Subject: a\r\n\r\n").unwrap();

    let client = client(root);
    let maildir = client.load_maildir("").unwrap();
    let seen = MaildirFlags::from_iter([MaildirFlag::Seen]);

    assert!(client.add_flags(maildir.clone(), "1.host", seen).is_err());
    assert!(client.get(maildir, "1.host").unwrap().flags().is_empty());
    assert!(root.join("cur").join(name).is_file());
}

#[test]
fn loading_an_unsearchable_folder_names_the_permission_error() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().unwrap();
    let root = dir.path();
    let locked = root.join("Locked");
    maildir_at(&locked);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();

    // NOTE: root searches through any mode, so the check only holds for
    // an ordinary user.
    if locked.join("cur").is_dir() {
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        return;
    }

    let err = client(root).load_maildir("Locked").unwrap_err().to_string();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();

    assert!(err.contains("Locked/cur"), "{err}");
    assert!(err.contains("ermission denied"), "{err}");
    assert!(!err.contains("missing"), "{err}");
}

#[test]
fn a_maildirpp_rename_or_delete_takes_the_children_along() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    maildir_at(root);
    maildir_at(&root.join(".Lists.rust"));
    maildir_at(&root.join(".Lists.rust.Kaffee"));
    maildir_at(&root.join(".Lists.rustacean"));
    maildir_at(&root.join(".Taken.Kaffee"));

    let mut client = client(root);
    client.store.maildirpp = true;

    assert!(client.rename_maildir("Lists/rust", "Taken").is_err());
    assert!(root.join(".Lists.rust/cur").is_dir());

    client.rename_maildir("Lists/rust", "Lists/go").unwrap();
    assert_eq!(
        names(&client),
        [
            "",
            ".Lists.go",
            ".Lists.go.Kaffee",
            ".Lists.rustacean",
            ".Taken.Kaffee"
        ]
    );

    client.delete_maildir("Lists/go").unwrap();
    assert_eq!(names(&client), ["", ".Lists.rustacean", ".Taken.Kaffee"]);
}
