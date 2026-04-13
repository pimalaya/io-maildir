//! Example: store a message in a Maildir synchronously.
//!
//! Run with:
//!
//! ```sh
//! cargo run --example std_store_message
//! ```

use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::Write,
    path::PathBuf,
};

use io_maildir::{
    coroutines::{
        maildir_create::{MaildirCreate, MaildirCreateArg, MaildirCreateResult},
        message_store::{MaildirMessageStore, MaildirMessageStoreArg, MaildirMessageStoreResult},
    },
    flag::Flags,
    maildir::{Maildir, MaildirSubdir},
};
use tempfile::tempdir;

fn dir_create<I: IntoIterator<Item = String>>(paths: I) {
    for path in paths {
        fs::create_dir(&path).unwrap();
    }
}

fn file_create(files: BTreeMap<String, Vec<u8>>) {
    for (path, contents) in files {
        let mut f = File::create(&path).unwrap();
        f.write_all(&contents).unwrap();
    }
}

fn rename(pairs: Vec<(String, String)>) {
    for (from, to) in pairs {
        fs::rename(&from, &to).unwrap();
    }
}

fn main() {
    let _ = env_logger::try_init();

    let tmp = tempdir().unwrap();
    let root: PathBuf = tmp.path().join("inbox");

    // create a new Maildir

    let mut arg: Option<MaildirCreateArg> = None;
    let mut create = MaildirCreate::new(root.clone());

    loop {
        match create.resume(arg.take()) {
            MaildirCreateResult::Ok => break,
            MaildirCreateResult::WantsDirCreate(paths) => {
                dir_create(paths);
                arg = Some(MaildirCreateArg::DirCreate);
            }
            MaildirCreateResult::Err(err) => panic!("{err}"),
        }
    }

    let maildir = Maildir::try_from(root).unwrap();

    // store a message in /new

    let contents = b"From: alice@example.com\r\nTo: bob@example.com\r\nSubject: Hello\r\n\r\nHello, world!\r\n".to_vec();

    let mut arg: Option<MaildirMessageStoreArg> = None;
    let mut store = MaildirMessageStore::new(
        maildir.clone(),
        MaildirSubdir::New,
        Flags::default(),
        contents,
    );

    let (id, path) = loop {
        match store.resume(arg.take()) {
            MaildirMessageStoreResult::Ok { id, path } => break (id, path),
            MaildirMessageStoreResult::WantsFileCreate(files) => {
                file_create(files);
                arg = Some(MaildirMessageStoreArg::FileCreate);
            }
            MaildirMessageStoreResult::WantsRename(pairs) => {
                rename(pairs);
                arg = Some(MaildirMessageStoreArg::Rename);
            }
            MaildirMessageStoreResult::Err(err) => panic!("{err}"),
        }
    };

    println!("Stored message:");
    println!("  ID:   {id}");
    println!("  Path: {}", path.display());
}
