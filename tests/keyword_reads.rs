//! Keyword resolution on read.
//!
//! Covers the client read paths (`read_entry`, `read_entries`,
//! `read_entries_par`, `get`) handing back an entry whose flags carry
//! the custom keywords its mailbox spells, through the dovecot sidecar
//! or through a header.

use std::{collections::BTreeMap, fs};

use io_maildir::{
    client::MaildirClient,
    entry::MaildirFullEntry,
    flag::{KeywordHeader, MaildirFlag, MaildirFlags},
    maildir::{Maildir, MaildirSubdir},
    path::MaildirFsPath,
};
use tempfile::{TempDir, tempdir};

fn eml(keywords_header: Option<&str>) -> Vec<u8> {
    let mut lines = vec![
        "From: io-maildir test <test@example.org>".to_string(),
        "Subject: keyword read test".to_string(),
        "Date: Thu, 01 Jan 2026 00:00:00 +0000".to_string(),
    ];

    if let Some(header) = keywords_header {
        lines.push(header.to_string());
    }

    lines.push(String::new());
    lines.push("body".to_string());
    lines.join("\r\n").into_bytes()
}

fn client(dir: &TempDir) -> MaildirClient {
    let root = MaildirFsPath::new(dir.path().to_string_lossy().into_owned());
    MaildirClient::new(root)
}

fn maildir(client: &MaildirClient, name: &str) -> Maildir {
    client.create_maildir(name).expect("create maildir");
    client.load_maildir(name).expect("load maildir")
}

/// Writes a `dovecot-keywords` sidecar mapping each slot to its name,
/// in the order given.
fn sidecar(client: &MaildirClient, maildir: &Maildir, names: &[&str]) {
    let table: BTreeMap<char, String> = names
        .iter()
        .enumerate()
        .map(|(slot, name)| {
            let letter = char::from(b'a' + slot as u8);
            (letter, name.to_string())
        })
        .collect();

    client
        .store_dovecot_keywords(maildir, &table)
        .expect("store dovecot keywords");
}

/// Stores `contents` under `cur/` with `flags`, returning its id.
fn store(
    client: &MaildirClient,
    maildir: &Maildir,
    flags: MaildirFlags,
    contents: Vec<u8>,
) -> String {
    let (id, _path) = client
        .store(maildir.clone(), MaildirSubdir::Cur, flags, contents)
        .expect("store entry");
    id
}

fn keywords(entry: &MaildirFullEntry) -> Vec<&str> {
    entry
        .flags()
        .iter()
        .filter_map(MaildirFlag::as_keyword)
        .collect()
}

fn read_only_entry(client: &MaildirClient, maildir: &Maildir) -> MaildirFullEntry {
    let entries: Vec<_> = client
        .list_entries(maildir.clone())
        .expect("list entries")
        .into_iter()
        .collect();

    client
        .read_entries(maildir, &entries)
        .expect("read entries")
        .into_iter()
        .next()
        .expect("one entry")
}

#[test]
fn keywords_stay_unread_while_both_options_are_off() {
    let dir = tempdir().unwrap();
    let client = client(&dir);
    let inbox = maildir(&client, "inbox");
    sidecar(&client, &inbox, &["NonJunk"]);

    let flags = MaildirFlags::from_iter([MaildirFlag::Seen]);
    let path = client
        .store(
            inbox.clone(),
            MaildirSubdir::Cur,
            flags,
            eml(Some("X-Keywords: Work")),
        )
        .expect("store entry")
        .1;
    // NOTE: the slot letter is appended by hand, the store path writing
    // one only when it resolves keywords itself.
    fs::rename(path.as_str(), format!("{path}a")).expect("append the slot letter");

    let entry = read_only_entry(&client, &inbox);

    assert!(keywords(&entry).is_empty());
    assert!(entry.flags().contains(&MaildirFlag::Seen));
}

#[test]
fn dovecot_slot_letters_resolve_on_read() {
    let dir = tempdir().unwrap();
    let mut client = client(&dir);
    client.dovecot_keywords = true;
    let inbox = maildir(&client, "inbox");

    let flags = MaildirFlags::from_iter([MaildirFlag::Seen, MaildirFlag::keyword("NonJunk")]);
    store(&client, &inbox, flags, eml(None));

    let entry = read_only_entry(&client, &inbox);

    assert_eq!(keywords(&entry), ["NonJunk"]);
    assert!(entry.flags().contains(&MaildirFlag::Seen));
}

#[test]
fn header_keywords_resolve_on_read() {
    let dir = tempdir().unwrap();
    let mut client = client(&dir);
    client.keywords_header = Some(KeywordHeader::XLabel);
    let inbox = maildir(&client, "inbox");

    let flags =
        MaildirFlags::from_iter([MaildirFlag::keyword("work"), MaildirFlag::keyword("later")]);
    store(&client, &inbox, flags, eml(None));

    let entry = read_only_entry(&client, &inbox);

    assert_eq!(keywords(&entry), ["later", "work"]);
}

#[test]
fn every_read_path_resolves_alike() {
    let dir = tempdir().unwrap();
    let mut client = client(&dir);
    client.dovecot_keywords = true;
    let inbox = maildir(&client, "inbox");

    let flags = MaildirFlags::from_iter([MaildirFlag::keyword("NonJunk")]);
    let id = store(&client, &inbox, flags, eml(None));

    let entries: Vec<_> = client
        .list_entries(inbox.clone())
        .expect("list entries")
        .into_iter()
        .collect();

    let one = client
        .read_entry(&inbox, &entries[0])
        .expect("read one entry");
    let sequential = client
        .read_entries(&inbox, &entries)
        .expect("read entries sequentially");
    let parallel = client
        .read_entries_par(&inbox, &entries)
        .expect("read entries in parallel");
    let fetched = client.get(inbox.clone(), &id).expect("get entry");

    assert_eq!(keywords(&one), ["NonJunk"]);
    assert_eq!(sequential, parallel);
    assert_eq!(keywords(sequential.iter().next().unwrap()), ["NonJunk"]);
    assert_eq!(keywords(&fetched), ["NonJunk"]);
}

#[test]
fn each_mailbox_reads_its_own_table() {
    let dir = tempdir().unwrap();
    let mut client = client(&dir);
    client.dovecot_keywords = true;
    let inbox = maildir(&client, "inbox");
    let work = maildir(&client, "work");

    // Slot `a` names a different keyword in each mailbox, which a table
    // loaded once for the whole store would get wrong for one of them.
    store(
        &client,
        &inbox,
        MaildirFlags::from_iter([MaildirFlag::keyword("RootKeyword")]),
        eml(None),
    );
    store(
        &client,
        &work,
        MaildirFlags::from_iter([MaildirFlag::keyword("WorkKeyword")]),
        eml(None),
    );

    assert_eq!(keywords(&read_only_entry(&client, &inbox)), ["RootKeyword"]);
    assert_eq!(keywords(&read_only_entry(&client, &work)), ["WorkKeyword"]);
}

#[test]
#[cfg(unix)]
fn an_unreadable_sidecar_leaves_the_mailbox_readable() {
    use std::{fs::Permissions, os::unix::fs::PermissionsExt};

    let dir = tempdir().unwrap();
    let mut client = client(&dir);
    client.dovecot_keywords = true;
    let inbox = maildir(&client, "inbox");

    let flags = MaildirFlags::from_iter([MaildirFlag::Seen, MaildirFlag::keyword("NonJunk")]);
    store(&client, &inbox, flags, eml(None));

    let sidecar = inbox.path().join("dovecot-keywords");
    fs::set_permissions(sidecar.as_str(), Permissions::from_mode(0o000))
        .expect("make the sidecar unreadable");

    // NOTE: root reads it regardless, and then there is nothing to
    // assert.
    if fs::File::open(sidecar.as_str()).is_ok() {
        return;
    }

    let entry = read_only_entry(&client, &inbox);

    assert!(keywords(&entry).is_empty());
    assert!(entry.flags().contains(&MaildirFlag::Seen));
}

#[test]
fn a_missing_sidecar_leaves_the_mailbox_readable() {
    let dir = tempdir().unwrap();
    let mut client = client(&dir);
    client.dovecot_keywords = true;
    let inbox = maildir(&client, "inbox");

    let flags = MaildirFlags::from_iter([MaildirFlag::Seen, MaildirFlag::keyword("NonJunk")]);
    store(&client, &inbox, flags, eml(None));
    fs::remove_file(inbox.path().join("dovecot-keywords").as_str()).expect("remove the sidecar");

    let entry = read_only_entry(&client, &inbox);

    assert!(keywords(&entry).is_empty());
    assert!(entry.flags().contains(&MaildirFlag::Seen));
}
