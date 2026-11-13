#![cfg(feature = "client")]

use std::fs;

use io_maildir::client::MaildirClient;
use tempfile::tempdir;

#[test]
fn a_store_holding_plain_files_beside_its_folders_lists() {
    let dir = tempdir().expect("temporary store");
    let client = MaildirClient::new(dir.path().to_string_lossy().into_owned());

    client.create_maildir("Archive").expect("create Archive");
    client
        .create_maildir("Archive/2026")
        .expect("create nested");

    for file in [
        "subscriptions",
        "Archive/dovecot-uidlist",
        "Archive/dovecot-keywords",
    ] {
        fs::write(dir.path().join(file), b"").expect("write sidecar");
    }

    let maildirs = client.list_maildirs().expect("list maildirs");
    let names: Vec<_> = maildirs
        .iter()
        .filter_map(|maildir| client.store.relative(maildir.path()))
        .map(|name| name.as_str().to_owned())
        .collect();

    assert_eq!(names, ["Archive", "Archive/2026"]);
}
