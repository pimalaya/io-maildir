#![cfg(feature = "client")]

use std::{fs, path::Path};

use io_maildir::{client::MaildirClient, maildir::Maildir};
use tempfile::tempdir;

fn store_at(root: &Path) -> (MaildirClient, Maildir) {
    for subdir in ["cur", "new", "tmp"] {
        fs::create_dir_all(root.join(subdir)).unwrap();
    }

    let client = MaildirClient::new(root.to_string_lossy().into_owned());
    let maildir = client.load_maildir("").unwrap();

    (client, maildir)
}

fn listed(client: &MaildirClient, maildir: &Maildir) -> Vec<String> {
    let entries = client.list_entries(maildir.clone()).unwrap();
    let names = entries.iter().map(|entry| entry.path().file_name());
    names.map(|name| name.unwrap().to_owned()).collect()
}

#[test]
fn a_listing_keeps_the_regular_files_alone() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let (client, maildir) = store_at(root);
    fs::write(root.join("new/1.host"), b"\r\n").unwrap();
    fs::write(root.join("cur/2.host:2,S"), b"\r\n").unwrap();
    fs::write(root.join("cur/.hidden"), b"\r\n").unwrap();
    fs::create_dir(root.join("cur/3.host:2,S")).unwrap();

    assert_eq!(listed(&client, &maildir), ["2.host:2,S", "1.host"]);
}

#[cfg(unix)]
#[test]
fn a_link_counts_as_what_it_points_to() {
    use std::os::unix::fs::symlink;

    let dir = tempdir().unwrap();
    let root = dir.path();
    let (client, maildir) = store_at(root);
    fs::write(root.join("target"), b"\r\n").unwrap();
    symlink(root.join("target"), root.join("cur/1.host:2,S")).unwrap();
    symlink(root.join("tmp"), root.join("cur/2.host:2,S")).unwrap();
    symlink(root.join("nowhere"), root.join("cur/3.host:2,S")).unwrap();

    assert_eq!(listed(&client, &maildir), ["1.host:2,S"]);
    assert!(client.locate(maildir.clone(), "1.host").is_ok());
    assert!(client.locate(maildir, "3.host").is_err());
}

#[test]
fn many_ids_are_located_at_once() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let (client, maildir) = store_at(root);
    fs::write(root.join("new/1.host"), b"\r\n").unwrap();
    fs::write(root.join("cur/2.host:2,FS"), b"\r\n").unwrap();
    fs::write(root.join("tmp/3.host"), b"\r\n").unwrap();

    let ids = ["1.host", "2.host", "2.host:2,FS", "3.host", "4.host"];
    let found = client.locate_many(maildir, ids).unwrap();

    let ids: Vec<&str> = found.keys().map(String::as_str).collect();
    assert_eq!(ids, ["1.host", "2.host", "2.host:2,FS", "3.host"]);
    assert_eq!(found["2.host"].flags.to_string(), "FS");
    assert_eq!(found["2.host"].path, found["2.host:2,FS"].path);
}
