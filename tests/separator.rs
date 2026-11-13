#![cfg(feature = "client")]

use std::{fs, path::Path};

use io_maildir::{
    client::MaildirClient,
    flag::{MaildirFlag, MaildirFlags},
    maildir::{Maildir, MaildirSubdir},
};
use tempfile::tempdir;

/// A store holding the Maildirs `inbox` and `archive`, read with the
/// info separator `separator`.
fn store_at(root: &Path, separator: char) -> (MaildirClient, Maildir, Maildir) {
    for subdir in ["cur", "new", "tmp"] {
        fs::create_dir_all(root.join("inbox").join(subdir)).unwrap();
        fs::create_dir_all(root.join("archive").join(subdir)).unwrap();
    }

    let mut client = MaildirClient::new(root.to_string_lossy().into_owned());
    client.store.separator = separator;
    let inbox = client.load_maildir("inbox").unwrap();
    let archive = client.load_maildir("archive").unwrap();

    (client, inbox, archive)
}

fn names(root: &Path, dir: &str) -> Vec<String> {
    let entries = fs::read_dir(root.join(dir)).unwrap();
    entries
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect()
}

#[test]
fn a_store_writes_and_reads_its_own_separator() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let (client, inbox, archive) = store_at(root, ';');

    let seen = MaildirFlags::from_iter([MaildirFlag::Seen]);
    let (id, _) = client
        .store(inbox.clone(), MaildirSubdir::Cur, seen, b"\r\n".to_vec())
        .unwrap();
    assert_eq!(names(root, "inbox/cur"), [format!("{id};2,S")]);

    let entries = client.list_entries(inbox.clone()).unwrap();
    let entry = entries.first().unwrap();
    assert_eq!(entry.id(), Some(id.as_str()));
    assert_eq!(entry.flags().to_string(), "S");

    let flagged = MaildirFlags::from_iter([MaildirFlag::Flagged]);
    client.add_flags(inbox.clone(), &id, flagged).unwrap();
    assert_eq!(names(root, "inbox/cur"), [format!("{id};2,FS")]);

    let copy = client.copy(&id, inbox, archive.clone(), None).unwrap();
    assert_eq!(names(root, "archive/cur"), [format!("{copy};2,FS")]);

    let read = client.get(archive, &copy).unwrap();
    assert_eq!(read.id(), Some(copy.as_str()));
    assert_eq!(read.flags().to_string(), "FS");
}

#[test]
fn a_name_is_split_at_the_separator_of_its_store_alone() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let (semicolon, inbox, _) = store_at(root, ';');
    fs::write(root.join("inbox/cur/1.host;2,S"), b"\r\n").unwrap();

    let (path, _, flags) = semicolon.locate(inbox, "1.host").unwrap();
    assert_eq!(path.file_name(), Some("1.host;2,S"));
    assert_eq!(flags.to_string(), "S");

    let (colon, inbox, _) = store_at(root, ':');
    assert!(colon.locate(inbox.clone(), "1.host").is_err());
    let entries = colon.list_entries(inbox).unwrap();
    assert_eq!(entries.first().unwrap().id(), Some("1.host;2,S"));
}

#[cfg(unix)]
#[test]
fn a_store_off_the_colon_takes_no_backslash_nor_colon_in_a_name() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let (client, inbox, _) = store_at(root, ';');
    fs::write(root.join("inbox/cur/a\\b;2,S"), b"\r\n").unwrap();

    assert!(client.create_maildir("inbox\\cur").is_err());
    assert!(client.load_maildir("..\\inbox").is_err());
    assert!(client.locate(inbox, "a\\b").is_err());
    assert_eq!(names(root, "").len(), 2);

    let (client, inbox, _) = store_at(root, ':');
    assert!(client.locate(inbox, "a\\b;2,S").is_ok());
}
