#![cfg(feature = "client")]

use std::{fs, path::Path};

use io_maildir::{
    client::MaildirClient,
    flag::{MaildirFlag, MaildirFlags},
    maildir::{Maildir, MaildirSubdir},
};
use tempfile::tempdir;

/// A root Maildir whose `dovecot-keywords` file holds `table`, and a
/// client resolving keywords through it.
fn store_with(root: &Path, table: &[u8]) -> (MaildirClient, Maildir) {
    for subdir in ["cur", "new", "tmp"] {
        fs::create_dir_all(root.join(subdir)).unwrap();
    }
    fs::write(root.join("dovecot-keywords"), table).unwrap();

    let mut client = MaildirClient::new(root.to_string_lossy().into_owned());
    client.dovecot_keywords = true;
    let maildir = client.load_maildir("").unwrap();

    (client, maildir)
}

fn keyword(name: &str) -> MaildirFlags {
    MaildirFlags::from_iter([MaildirFlag::keyword(name)])
}

fn names(root: &Path, subdir: &str) -> Vec<String> {
    let entries = fs::read_dir(root.join(subdir)).unwrap();
    entries
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect()
}

#[test]
fn a_new_keyword_keeps_the_lines_beyond_the_letters() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let (client, maildir) = store_with(root, b"0 $Junk\n26 Beyond\n30 Far\n");

    client
        .store(
            maildir,
            MaildirSubdir::Cur,
            keyword("Work"),
            b"\r\n".to_vec(),
        )
        .unwrap();

    let table = fs::read_to_string(root.join("dovecot-keywords")).unwrap();
    assert_eq!(table, "0 $Junk\n26 Beyond\n30 Far\n1 Work\n");
    assert!(names(root, "tmp").is_empty());
}

#[test]
fn a_file_that_does_not_parse_is_not_rewritten() {
    let files: [&[u8]; 2] = [b"0 \xff\xfe\n", b"0 $Junk\nnot a slot\n"];

    for file in files {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let (client, maildir) = store_with(root, file);
        fs::write(root.join("cur/1.host:2,a"), b"\r\n").unwrap();

        let stored = client.store(
            maildir.clone(),
            MaildirSubdir::Cur,
            keyword("Work"),
            b"\r\n".to_vec(),
        );
        assert!(stored.is_err());
        let added = client.add_flags(maildir, "1.host", keyword("Work"));
        assert!(added.is_err());

        assert_eq!(fs::read(root.join("dovecot-keywords")).unwrap(), file);
        assert_eq!(names(root, "cur"), ["1.host:2,a"]);
        assert!(names(root, "tmp").is_empty());
    }
}

#[test]
fn a_file_holding_a_stray_line_still_names_its_keywords() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let (client, maildir) = store_with(root, b"0 $Junk\nnot a slot\n");
    fs::write(root.join("cur/1.host:2,a"), b"\r\n").unwrap();

    let entry = client.get(maildir, "1.host").unwrap();
    let flags: Vec<&MaildirFlag> = entry.flags().iter().collect();
    assert_eq!(flags, [&MaildirFlag::keyword("$Junk")]);
}

#[test]
fn a_copy_into_a_file_that_does_not_parse_delivers_nothing() {
    let dir = tempdir().unwrap();
    let (client, source) = store_with(&dir.path().join("A"), b"0 $Important\n");
    let (_, target) = store_with(&dir.path().join("B"), b"0 $Junk\nnot a slot\n");
    fs::write(dir.path().join("A/cur/1.host:2,Sa"), b"\r\n").unwrap();

    let copied = client.copy("1.host", source.clone(), target.clone(), None);
    assert!(copied.is_err());
    let moved = client.r#move("1.host", source, target, None);
    assert!(moved.is_err());

    assert_eq!(names(&dir.path().join("A"), "cur"), ["1.host:2,Sa"]);
    assert!(names(&dir.path().join("B"), "cur").is_empty());
    assert!(names(&dir.path().join("B"), "tmp").is_empty());
}

#[test]
fn a_keyword_differing_by_case_takes_the_slot_the_file_names() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let (client, maildir) = store_with(root, b"0 $Forwarded\n");
    fs::write(root.join("cur/1.host:2,"), b"\r\n").unwrap();

    client
        .add_flags(maildir.clone(), "1.host", keyword("$forwarded"))
        .unwrap();
    assert_eq!(names(root, "cur"), ["1.host:2,a"]);

    client
        .remove_flags(maildir, "1.host", keyword("$FORWARDED"))
        .unwrap();
    assert_eq!(names(root, "cur"), ["1.host:2,"]);

    let table = fs::read_to_string(root.join("dovecot-keywords")).unwrap();
    assert_eq!(table, "0 $Forwarded\n");
}
