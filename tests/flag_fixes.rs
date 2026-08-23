//! Flag regression tests.
//!
//! Covers the new to cur transition a flag write owes an entry that is
//! still in the new subdirectory, where a Maildir name has no info
//! suffix to hold a flag.

use std::path::Path;

use io_maildir::{
    client::MaildirClient,
    flag::{MaildirFlag, MaildirFlags},
    maildir::{Maildir, MaildirSubdir},
    path::MaildirFsPath,
};
use tempfile::tempdir;

fn eml() -> Vec<u8> {
    [
        "From: io-maildir test <test@example.org>",
        "To: io-maildir test <test@example.org>",
        "Subject: flag fix test",
        "Date: Thu, 01 Jan 2026 00:00:00 +0000",
        "",
        "body",
    ]
    .join("\r\n")
    .into_bytes()
}

fn setup(client: &MaildirClient) -> Maildir {
    client.create_maildir("inbox").expect("create inbox");
    client.load_maildir("inbox").expect("load inbox")
}

#[test]
fn adding_a_flag_moves_the_entry_out_of_new() {
    let _ = env_logger::try_init();
    let dir = tempdir().unwrap();
    let root = MaildirFsPath::new(dir.path().to_string_lossy().into_owned());

    let client = MaildirClient::new(root);
    let inbox = setup(&client);

    let (id, path) = client
        .store(
            inbox.clone(),
            MaildirSubdir::New,
            MaildirFlags::default(),
            eml(),
        )
        .expect("store message in new");
    assert_eq!(path, inbox.new().join(&id));

    let seen = MaildirFlags::from_iter([MaildirFlag::Seen]);
    client
        .add_flags(inbox.clone(), &id, seen)
        .expect("add seen flag");

    assert!(
        !Path::new(path.as_str()).exists(),
        "the entry must leave new once it carries a flag",
    );
    let moved = inbox.cur().join(&format!("{id}:2,S"));
    assert!(
        Path::new(moved.as_str()).exists(),
        "the entry must land in cur under its flags, got no {moved}",
    );

    // The id is what a caller stored, so it has to survive the move.
    let (located, subdir, flags) = client.locate(inbox, &id).expect("locate moved entry");
    assert_eq!(located, moved);
    assert_eq!(subdir, MaildirSubdir::Cur);
    assert!(flags.contains(&MaildirFlag::Seen));
}

#[test]
fn removing_an_absent_flag_leaves_the_entry_in_new() {
    let _ = env_logger::try_init();
    let dir = tempdir().unwrap();
    let root = MaildirFsPath::new(dir.path().to_string_lossy().into_owned());

    let client = MaildirClient::new(root);
    let inbox = setup(&client);

    let (id, path) = client
        .store(
            inbox.clone(),
            MaildirSubdir::New,
            MaildirFlags::default(),
            eml(),
        )
        .expect("store message in new");

    let seen = MaildirFlags::from_iter([MaildirFlag::Seen]);
    client
        .remove_flags(inbox.clone(), &id, seen)
        .expect("remove seen flag");

    assert!(
        Path::new(path.as_str()).exists(),
        "a write that clears nothing must leave the entry unread in new",
    );
    let (_, subdir, _) = client.locate(inbox, &id).expect("locate entry");
    assert_eq!(subdir, MaildirSubdir::New);
}
