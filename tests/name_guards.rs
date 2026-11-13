#![cfg(feature = "client")]

use std::{fs, path::Path};

use io_maildir::{
    client::MaildirClient,
    flag::{KeywordHeader, MaildirFlag, MaildirFlags},
    maildir::MaildirSubdir,
};
use tempfile::tempdir;

const RESERVED: [&str; 5] = ["cur", "new", "tmp", "Work/cur", "new/Kid"];

fn maildir_at(path: &Path) {
    for subdir in ["cur", "new", "tmp"] {
        fs::create_dir_all(path.join(subdir)).unwrap();
    }
}

fn client(root: &Path) -> MaildirClient {
    MaildirClient::new(root.to_string_lossy().into_owned())
}

fn keywords(names: &[&str]) -> MaildirFlags {
    names.iter().copied().map(MaildirFlag::keyword).collect()
}

#[test]
fn creating_a_reserved_name_leaves_the_parent_maildir_alone() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("store");
    maildir_at(&root);
    maildir_at(&root.join("Work"));

    let client = client(&root);

    for name in RESERVED {
        assert!(client.create_maildir(name).is_err(), "{name}");
    }
    for name in [".", "..", "../Out", "Work/.."] {
        assert!(client.create_maildir(name).is_err(), "{name}");
    }

    assert_eq!(fs::read_dir(root.join("new")).unwrap().count(), 0);
    assert_eq!(fs::read_dir(root.join("cur")).unwrap().count(), 0);
    assert_eq!(fs::read_dir(root.join("Work/cur")).unwrap().count(), 0);
    assert!(!dir.path().join("Out").exists());
    assert!(!dir.path().join("cur").exists());
}

#[test]
fn deleting_or_renaming_a_reserved_name_keeps_the_messages() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("store");
    maildir_at(&root);
    maildir_at(&root.join("Work"));
    fs::write(root.join("new/1.host"), b"Subject: a\r\n\r\n").unwrap();
    fs::write(root.join("Work/cur/2.host:2,S"), b"Subject: b\r\n\r\n").unwrap();
    fs::create_dir_all(root.join("new/Kid")).unwrap();

    let client = client(&root);

    for name in RESERVED {
        assert!(client.delete_maildir(name).is_err(), "{name}");
        assert!(client.rename_maildir(name, "Moved").is_err(), "{name}");
        assert!(client.rename_maildir("Work", name).is_err(), "{name}");
    }

    assert!(root.join("new/1.host").is_file());
    assert!(root.join("Work/cur/2.host:2,S").is_file());
    assert!(root.join("new/Kid").is_dir());
    assert!(!root.join("Moved").exists());
}

#[test]
fn loading_a_reserved_name_or_a_path_leaving_the_root_is_refused() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("store");
    maildir_at(dir.path());
    maildir_at(&root);
    maildir_at(&root.join("new"));
    maildir_at(&root.join("Work"));

    let client = client(&root);

    for name in ["new", "..", "Work/..", "."] {
        assert!(client.load_maildir(name).is_err(), "{name}");
    }

    client.load_maildir("").unwrap();
    client.load_maildir("Work").unwrap();
}

#[test]
fn maildirpp_takes_the_subdirectory_names_as_folder_names() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    maildir_at(root);

    let mut client = client(root);
    client.store.maildirpp = true;

    client.create_maildir("new").unwrap();
    assert!(root.join(".new/cur").is_dir());
    client.load_maildir("new").unwrap();
    client.rename_maildir("new", "tmp").unwrap();
    assert!(root.join(".tmp/cur").is_dir());
    client.delete_maildir("tmp").unwrap();
    assert!(!root.join(".tmp").exists());
    assert_eq!(fs::read_dir(root.join("new")).unwrap().count(), 0);
}

#[test]
fn a_keyword_holding_a_line_break_adds_no_slot_to_the_keyword_file() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    maildir_at(root);
    fs::write(root.join("dovecot-keywords"), "0 $Junk\n").unwrap();

    let mut client = client(root);
    client.dovecot_keywords = true;
    let maildir = client.load_maildir("").unwrap();

    let flags = keywords(&["Work", "x\n0 $NotJunk"]);
    let (id, _) = client
        .store(maildir.clone(), MaildirSubdir::Cur, flags, b"\r\n".to_vec())
        .unwrap();
    let flags = keywords(&["y\r\n0 $NotJunk", " padded", ""]);
    client.add_flags(maildir.clone(), &id, flags).unwrap();
    let flags = keywords(&["Work", "z\n0 $NotJunk"]);
    client.set_flags(maildir.clone(), &id, flags).unwrap();

    let table = fs::read_to_string(root.join("dovecot-keywords")).unwrap();
    assert_eq!(table, "0 $Junk\n1 Work\n");

    let entry = client.get(maildir, &id).unwrap();
    let flags: Vec<&MaildirFlag> = entry.flags().iter().collect();
    assert_eq!(flags, [&MaildirFlag::keyword("Work")]);
}

#[test]
fn a_keyword_holding_a_line_break_adds_no_header_to_the_message() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    maildir_at(root);

    let mut client = client(root);
    client.keywords_header = Some(KeywordHeader::XKeywords);
    let maildir = client.load_maildir("").unwrap();

    let flags = keywords(&["Work", "x\r\nBcc: eve@example.org", "y\r\n\r\nbody"]);
    let contents = b"Subject: a\r\n\r\nhello".to_vec();
    let (_, path) = client
        .store(maildir, MaildirSubdir::Cur, flags, contents)
        .unwrap();

    let stored = fs::read_to_string(path.as_str()).unwrap();
    assert_eq!(stored, "X-Keywords: Work\r\nSubject: a\r\n\r\nhello");
}
