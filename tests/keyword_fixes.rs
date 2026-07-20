//! Keyword regression tests.
//!
//! Covers the dovecot slot letters that flag operations used to drop,
//! and the header round-trip of a keyword containing the active
//! separator.

use std::{collections::BTreeMap, path::Path};

use io_maildir::{
    client::MaildirClient,
    entry::{MaildirEntry, headers::extract_keywords_header},
    flag::{KeywordHeader, MaildirFlag, MaildirFlags},
    maildir::{Maildir, MaildirSubdir},
    path::MaildirFsPath,
};
use tempfile::tempdir;

fn eml(tag: &str) -> Vec<u8> {
    [
        "From: io-maildir test <test@example.org>",
        "To: io-maildir test <test@example.org>",
        &format!("Subject: keyword fix test {tag}"),
        "Date: Thu, 01 Jan 2026 00:00:00 +0000",
        "MIME-Version: 1.0",
        "Content-Type: text/plain; charset=utf-8",
        "",
        &format!("body {tag}"),
    ]
    .join("\r\n")
    .into_bytes()
}

/// Reads back the keywords of a message, resolving its filename slot
/// letters through the per-folder dovecot table.
fn dovecot_keywords(client: &MaildirClient, maildir: &Maildir, id: &str) -> Vec<String> {
    let (path, _, _) = client.locate(maildir.clone(), id).expect("locate message");
    let table = client
        .load_dovecot_keywords(maildir)
        .expect("load dovecot table");
    MaildirFlags::with_dovecot(&path, &table)
        .iter()
        .filter_map(|f| f.as_keyword().map(str::to_string))
        .collect()
}

fn setup(client: &MaildirClient) -> Maildir {
    client.create_maildir("inbox").expect("create inbox");
    client.load_maildir("inbox").expect("load inbox")
}

#[test]
fn add_flag_preserves_dovecot_keyword() {
    let _ = env_logger::try_init();
    let dir = tempdir().unwrap();
    let root = MaildirFsPath::new(dir.path().to_string_lossy().into_owned());

    let mut client = MaildirClient::new(root);
    client.dovecot_keywords = true;
    let inbox = setup(&client);

    let (id, _) = client
        .store(
            inbox.clone(),
            MaildirSubdir::Cur,
            MaildirFlags::from_iter([MaildirFlag::keyword("NonJunk")]),
            eml("critical"),
        )
        .expect("store keyworded message");

    assert_eq!(
        dovecot_keywords(&client, &inbox, &id),
        vec!["NonJunk".to_string()],
        "keyword should be encoded via a dovecot slot after store",
    );

    client
        .add_flags(
            inbox.clone(),
            &id,
            MaildirFlags::from_iter([MaildirFlag::Seen]),
        )
        .expect("add Seen");

    let (path, _, _) = client.locate(inbox.clone(), &id).expect("locate after add");
    let named = MaildirFlags::from(&path);
    assert!(
        named.contains(&MaildirFlag::Seen),
        "Seen must be present after add_flags",
    );
    assert_eq!(
        dovecot_keywords(&client, &inbox, &id),
        vec!["NonJunk".to_string()],
        "NonJunk must survive add_flags(\\Seen)",
    );
}

#[test]
fn remove_one_keyword_leaves_the_other() {
    let _ = env_logger::try_init();
    let dir = tempdir().unwrap();
    let root = MaildirFsPath::new(dir.path().to_string_lossy().into_owned());

    let mut client = MaildirClient::new(root);
    client.dovecot_keywords = true;
    let inbox = setup(&client);

    let (id, _) = client
        .store(
            inbox.clone(),
            MaildirSubdir::Cur,
            MaildirFlags::from_iter([
                MaildirFlag::keyword("NonJunk"),
                MaildirFlag::keyword("Work"),
            ]),
            eml("multi"),
        )
        .expect("store two-keyword message");

    let mut before = dovecot_keywords(&client, &inbox, &id);
    before.sort();
    assert_eq!(before, vec!["NonJunk".to_string(), "Work".to_string()]);

    client
        .remove_flags(
            inbox.clone(),
            &id,
            MaildirFlags::from_iter([MaildirFlag::keyword("Work")]),
        )
        .expect("remove Work keyword");

    assert_eq!(
        dovecot_keywords(&client, &inbox, &id),
        vec!["NonJunk".to_string()],
        "removing Work must leave NonJunk's slot letter intact",
    );
}

#[test]
fn normal_keyword_round_trips_via_dovecot() {
    let _ = env_logger::try_init();
    let dir = tempdir().unwrap();
    let root = MaildirFsPath::new(dir.path().to_string_lossy().into_owned());

    let mut client = MaildirClient::new(root);
    client.dovecot_keywords = true;
    let inbox = setup(&client);

    let (id, _) = client
        .store(
            inbox.clone(),
            MaildirSubdir::Cur,
            MaildirFlags::from_iter([MaildirFlag::keyword("Important")]),
            eml("normal"),
        )
        .expect("store");

    assert_eq!(
        dovecot_keywords(&client, &inbox, &id),
        vec!["Important".to_string()],
    );
}

#[test]
fn normal_keyword_round_trips_via_header() {
    let _ = env_logger::try_init();
    let dir = tempdir().unwrap();
    let root = MaildirFsPath::new(dir.path().to_string_lossy().into_owned());

    let mut client = MaildirClient::new(root);
    client.keywords_header = Some(KeywordHeader::XKeywords);
    let inbox = setup(&client);

    let (id, _) = client
        .store(
            inbox.clone(),
            MaildirSubdir::Cur,
            MaildirFlags::from_iter([MaildirFlag::keyword("Important")]),
            eml("hdr"),
        )
        .expect("store");

    let (path, _, _) = client.locate(inbox.clone(), &id).expect("locate");
    let entry = MaildirEntry::from_path(path);
    let msg = client.read_entry(&entry).expect("read");
    let kws = extract_keywords_header(msg.contents(), KeywordHeader::XKeywords);
    assert_eq!(kws, vec!["Important".to_string()]);
}

#[test]
fn keyword_with_separator_is_dropped_not_corrupted() {
    let _ = env_logger::try_init();
    let dir = tempdir().unwrap();
    let root = MaildirFsPath::new(dir.path().to_string_lossy().into_owned());

    let mut client = MaildirClient::new(root);
    client.keywords_header = Some(KeywordHeader::XKeywords);
    let inbox = setup(&client);

    let (id, _) = client
        .store(
            inbox.clone(),
            MaildirSubdir::Cur,
            MaildirFlags::from_iter([MaildirFlag::keyword("Foo,Bar")]),
            eml("sep"),
        )
        .expect("store");

    let (path, _, _) = client.locate(inbox.clone(), &id).expect("locate");
    let entry = MaildirEntry::from_path(path);
    let msg = client.read_entry(&entry).expect("read");
    let kws = extract_keywords_header(msg.contents(), KeywordHeader::XKeywords);

    assert!(
        !kws.iter().any(|k| k == "Foo"),
        "no corrupted `Foo` fragment should appear",
    );
    assert!(
        !kws.iter().any(|k| k == "Bar"),
        "no corrupted `Bar` fragment should appear",
    );
    assert!(
        kws.is_empty(),
        "the corruptible keyword should be dropped entirely, got {kws:?}",
    );

    let (final_path, _, _) = client.locate(inbox.clone(), &id).expect("still located");
    assert!(Path::new(final_path.as_str()).is_file());
}

/// A dovecot `,S=<size>,W=<vsize>` extension lives in the unique part,
/// before the info section, so it is never a flag.
#[test]
fn size_extensions_are_not_flags() {
    let no_info = MaildirFsPath::new("/x/new/1614632942.M123P456.host,S=1234,W=1256");
    let flags = MaildirFlags::from(&no_info);
    assert!(
        flags.is_empty(),
        "an entry with no info section has no flags, got {:?}",
        flags.to_string(),
    );

    let no_comma = MaildirFsPath::new("/x/new/1614632942.M123P456.host");
    assert!(MaildirFlags::from(&no_comma).is_empty());

    let with_info = MaildirFsPath::new("/x/cur/1614632942.M123P456.host,S=1234,W=1256:2,Sa");
    let flags = MaildirFlags::from(&with_info);
    assert!(flags.contains(&MaildirFlag::Seen), "Seen must parse");
    assert_eq!(
        flags.to_string(),
        "Sa",
        "the dovecot slot letter must survive beside the named flag",
    );
}

/// Same rule for the dovecot-resolving reader, or a lowercase letter of
/// the unique part could resolve into a keyword the message never had.
#[test]
fn size_extensions_do_not_resolve_as_dovecot_keywords() {
    let table = BTreeMap::from([('a', "NonJunk".to_string()), ('w', "Work".to_string())]);

    let no_info = MaildirFsPath::new("/x/new/1614632942.M123P456.host,a");
    let resolved = MaildirFlags::with_dovecot(&no_info, &table);
    assert!(
        resolved.is_empty(),
        "no info section means no keywords, got {resolved:?}",
    );

    let with_info = MaildirFsPath::new("/x/cur/1614632942.M123P456.host:2,Sa");
    let resolved: Vec<String> = MaildirFlags::with_dovecot(&with_info, &table)
        .iter()
        .filter_map(|f| f.as_keyword().map(str::to_string))
        .collect();
    assert_eq!(resolved, vec!["NonJunk".to_string()]);
}
