#![cfg(feature = "client")]

use std::{fs, path::Path};

use io_maildir::{
    client::MaildirClient,
    entry::{copy::MaildirEntryCopy, r#move::MaildirEntryMove},
    flag::{MaildirFlag, MaildirFlags},
    maildir::{Maildir, MaildirSubdir},
    path::MaildirFsPath,
};
use tempfile::tempdir;

fn fixture() -> (tempfile::TempDir, MaildirClient, Maildir) {
    let dir = tempdir().expect("temporary maildir");
    for subdir in ["new", "cur", "tmp"] {
        fs::create_dir(dir.path().join(subdir)).expect("create subdirectory");
    }
    let root = MaildirFsPath::new(dir.path().to_string_lossy().into_owned());
    let maildir = Maildir::from_path(root.clone());
    (dir, MaildirClient::new(root), maildir)
}

fn put(maildir: &Maildir, subdir: &str, name: &str, bytes: &[u8]) {
    fs::write(
        maildir.path().as_str().to_owned() + "/" + subdir + "/" + name,
        bytes,
    )
    .expect("write fixture entry");
}

fn snapshot(maildir: &Maildir) -> Vec<(String, Vec<u8>)> {
    let mut files = Vec::new();
    for subdir in ["new", "cur", "tmp"] {
        for entry in fs::read_dir(maildir.path().as_str().to_owned() + "/" + subdir).unwrap() {
            let entry = entry.unwrap();
            if entry.path().is_file() {
                files.push((
                    entry.path().to_string_lossy().into_owned(),
                    fs::read(entry.path()).unwrap(),
                ));
            }
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files
}

#[test]
fn listed_plain_and_suffixed_entries_round_trip_without_mutation() {
    let (_dir, client, maildir) = fixture();
    let new_plain = b"Subject: plain\r\n\r\nnew plain";
    let new_suffixed = b"Subject: suffixed\r\n\r\nnew suffixed";
    let cur_plain = b"Subject: current\r\n\r\ncur plain";
    let cur_suffixed = b"Subject: flagged\r\n\r\ncur flagged";
    let sep = maildir.separator;
    put(&maildir, "new", "plain", new_plain);
    put(&maildir, "new", &format!("abc,U=5{sep}2,"), new_suffixed);
    put(&maildir, "cur", "cur-id", cur_plain);
    put(&maildir, "cur", &format!("cur-flag{sep}2,F"), cur_suffixed);
    let before = snapshot(&maildir);

    for (id, body, subdir, flagged) in [
        ("plain", new_plain.as_slice(), MaildirSubdir::New, false),
        (
            "abc,U=5",
            new_suffixed.as_slice(),
            MaildirSubdir::New,
            false,
        ),
        ("cur-id", cur_plain.as_slice(), MaildirSubdir::Cur, false),
        (
            "cur-flag",
            cur_suffixed.as_slice(),
            MaildirSubdir::Cur,
            true,
        ),
    ] {
        let listed = client.list_entries(maildir.clone()).unwrap();
        let entry = listed.iter().find(|entry| entry.id() == Some(id)).unwrap();
        let got = client.get(maildir.clone(), id).unwrap();
        assert_eq!(got.contents(), body);
        assert_eq!(got.path(), entry.path());
        let (path, actual_subdir, flags) = client.locate(maildir.clone(), id).unwrap();
        assert_eq!(actual_subdir, subdir);
        assert_eq!(path, *entry.path());
        assert_eq!(flags.contains(&MaildirFlag::Flagged), flagged);
    }
    assert_eq!(
        snapshot(&maildir),
        before,
        "lookup and read must not mutate files"
    );
}

#[test]
fn overlapping_and_prefix_ids_resolve_only_exact_entries() {
    let (_dir, client, maildir) = fixture();
    put(&maildir, "cur", "abc:2,S", b"short");
    put(&maildir, "cur", "abc123:2,F", b"long");
    put(&maildir, "cur", "12345:2,S", b"numeric");
    assert_eq!(
        client.get(maildir.clone(), "abc").unwrap().contents(),
        b"short"
    );
    assert_eq!(
        client.get(maildir.clone(), "abc123").unwrap().contents(),
        b"long"
    );
    assert!(client.get(maildir.clone(), "missing").is_err());
    assert!(client.get(maildir, "1").is_err());
}

#[test]
fn delete_exact_id_leaves_prefix_neighbor_untouched() {
    let (_dir, client, maildir) = fixture();
    put(&maildir, "cur", "abc:2,S", b"requested");
    put(&maildir, "cur", "abc123:2,F", b"neighbor");
    client.delete_entry(maildir.clone(), "abc").unwrap();
    assert!(!Path::new(&(maildir.path().as_str().to_owned() + "/cur/abc:2,S")).exists());
    assert_eq!(
        fs::read(maildir.path().as_str().to_owned() + "/cur/abc123:2,F").unwrap(),
        b"neighbor"
    );
}

#[test]
fn matching_directory_is_not_a_message_file() {
    let (_dir, client, maildir) = fixture();
    fs::create_dir(maildir.path().as_str().to_owned() + "/cur/folder").unwrap();
    assert!(client.get(maildir, "folder").is_err());
}

#[test]
fn full_basename_reads_and_flagged_new_entries_keep_their_identity() {
    let (_dir, client, maildir) = fixture();
    let sep = maildir.separator;
    put(
        &maildir,
        "cur",
        &format!("full-id{sep}2,F"),
        b"full basename",
    );
    put(&maildir, "new", &format!("new-id{sep}2,F"), b"flagged new");

    let full = format!("full-id{sep}2,F");
    let got = client.get(maildir.clone(), &full).unwrap();
    assert_eq!(got.contents(), b"full basename");
    let (path, subdir, flags) = client.locate(maildir.clone(), &full).unwrap();
    assert_eq!(path.file_name(), Some(full.as_str()));
    assert_eq!(subdir, MaildirSubdir::Cur);
    assert!(flags.contains(&MaildirFlag::Flagged));
    client
        .add_flags(
            maildir.clone(),
            &full,
            MaildirFlags::from_iter([MaildirFlag::Seen]),
        )
        .unwrap();
    let (updated, _, updated_flags) = client.locate(maildir.clone(), "full-id").unwrap();
    assert!(updated_flags.contains(&MaildirFlag::Seen));
    assert!(updated_flags.contains(&MaildirFlag::Flagged));
    assert_eq!(
        updated.file_name(),
        Some(format!("full-id{sep}2,FS").as_str())
    );

    let listed = client.list_entries(maildir.clone()).unwrap();
    let listed_new = listed
        .iter()
        .find(|entry| entry.id() == Some("new-id"))
        .unwrap();
    assert!(listed_new.flags().contains(&MaildirFlag::Flagged));
    let got_new = client.get(maildir.clone(), "new-id").unwrap();
    assert_eq!(got_new.contents(), b"flagged new");
    assert!(
        client
            .locate(maildir, "new-id")
            .unwrap()
            .2
            .contains(&MaildirFlag::Flagged)
    );
}

#[test]
fn traversal_input_cannot_read_a_sibling_fixture() {
    let dir = tempdir().unwrap();
    let root_path = dir.path().join("maildir");
    for subdir in ["new", "cur", "tmp", "sibling/cur"] {
        fs::create_dir_all(root_path.join(subdir)).unwrap();
    }
    fs::write(root_path.join("sibling/cur/secret:2,S"), b"outside fixture").unwrap();
    let root = MaildirFsPath::new(root_path.to_string_lossy().into_owned());
    let maildir = Maildir::from_path(root.clone());
    let client = MaildirClient::new(root);
    assert!(client.get(maildir, "../sibling/cur/secret:2,S").is_err());
}

#[test]
fn prefix_failures_do_not_mutate_any_message_or_destination() {
    let (_dir, client, source) = fixture();
    let target_root = source.path().as_str().to_owned() + "/target";
    for subdir in ["new", "cur", "tmp"] {
        fs::create_dir_all(format!("{target_root}/{subdir}")).unwrap();
    }
    let target = Maildir::from_path(MaildirFsPath::new(target_root));
    put(&source, "cur", "abc123:2,F", b"the only message");
    let source_before = snapshot(&source);
    let target_before = snapshot(&target);
    let flags = MaildirFlags::from_iter([MaildirFlag::Seen]);

    let errors = [
        format!("{:?}", client.get(source.clone(), "abc").unwrap_err()),
        format!(
            "{:?}",
            client.delete_entry(source.clone(), "abc").unwrap_err()
        ),
        format!(
            "{:?}",
            client
                .run(MaildirEntryMove::new(
                    "abc",
                    source.clone(),
                    target.clone(),
                    None
                ))
                .unwrap_err()
        ),
        format!(
            "{:?}",
            client
                .run(MaildirEntryCopy::new(
                    "abc",
                    source.clone(),
                    target.clone(),
                    None
                ))
                .unwrap_err()
        ),
        format!(
            "{:?}",
            client
                .add_flags(source.clone(), "abc", flags.clone())
                .unwrap_err()
        ),
        format!(
            "{:?}",
            client
                .remove_flags(source.clone(), "abc", flags.clone())
                .unwrap_err()
        ),
        format!(
            "{:?}",
            client.set_flags(source.clone(), "abc", flags).unwrap_err()
        ),
    ];
    for error in errors {
        assert!(
            error.contains("NotFound"),
            "expected locator NotFound, got {error}"
        );
    }
    assert_eq!(snapshot(&source), source_before);
    assert_eq!(snapshot(&target), target_before);
}

#[test]
fn exact_copy_and_move_find_flagged_new_entries() {
    let (_dir, client, source) = fixture();
    let target_root = source.path().as_str().to_owned() + "/target";
    for subdir in ["new", "cur", "tmp"] {
        fs::create_dir_all(format!("{target_root}/{subdir}")).unwrap();
    }
    let target = Maildir::from_path(MaildirFsPath::new(target_root));
    let sep = source.separator;
    let name = format!("chosen{sep}2,F");
    put(&source, "new", &name, b"selected source");

    client
        .run(MaildirEntryCopy::new(
            &name,
            source.clone(),
            target.clone(),
            Some(MaildirSubdir::New),
        ))
        .unwrap();
    let copied = client.list_entries(target.clone()).unwrap();
    assert_eq!(copied.len(), 1);
    assert_eq!(
        fs::read(copied.iter().next().unwrap().path().as_str()).unwrap(),
        b"selected source"
    );
    assert!(copied.iter().next().unwrap().flags().is_empty());
    assert!(Path::new(&(source.path().as_str().to_owned() + "/new/" + &name)).exists());

    client
        .run(MaildirEntryMove::new(
            "chosen",
            source.clone(),
            target.clone(),
            None,
        ))
        .unwrap();
    let moved = client.list_entries(target.clone()).unwrap();
    assert_eq!(moved.len(), 2);
    assert!(moved.iter().all(|entry| entry.flags().is_empty()));
    assert!(!Path::new(&(source.path().as_str().to_owned() + "/new/" + &name)).exists());

    let explicit_copy = format!("explicit-copy{sep}2,F");
    put(&source, "new", &explicit_copy, b"explicit copy");
    client
        .run(MaildirEntryCopy::new(
            "explicit-copy",
            source.clone(),
            target.clone(),
            Some(MaildirSubdir::Cur),
        ))
        .unwrap();

    let explicit_move = format!("explicit-move{sep}2,F");
    put(&source, "new", &explicit_move, b"explicit move");
    client
        .run(MaildirEntryMove::new(
            &explicit_move,
            source.clone(),
            target.clone(),
            Some(MaildirSubdir::Cur),
        ))
        .unwrap();
    let final_entries = client.list_entries(target.clone()).unwrap();
    assert_eq!(final_entries.len(), 4);
    let flagged = final_entries
        .iter()
        .filter(|entry| entry.flags().contains(&MaildirFlag::Flagged));
    assert!(
        flagged
            .clone()
            .all(|entry| entry.path().as_str().contains("/cur/"))
    );
    assert_eq!(flagged.count(), 2);
    assert!(Path::new(&(source.path().as_str().to_owned() + "/new/" + &explicit_copy)).exists());
    assert!(!Path::new(&(source.path().as_str().to_owned() + "/new/" + &explicit_move)).exists());
}
