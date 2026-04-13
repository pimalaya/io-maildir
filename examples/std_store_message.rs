//! Example: store a message in a Maildir synchronously.
//!
//! Run with:
//!
//! ```sh
//! cargo run --example std_store_message
//! ```

use std::path::PathBuf;

use io_fs::runtimes::std::handle;
use io_maildir::{
    coroutines::{
        maildir_create::{MaildirCreate, MaildirCreateResult},
        message_store::{MaildirMessageStore, MaildirMessageStoreResult},
    },
    flag::Flags,
    maildir::{Maildir, MaildirSubdir},
};
use tempfile::tempdir;

fn main() {
    let _ = env_logger::try_init();

    let tmp = tempdir().unwrap();
    let root: PathBuf = tmp.path().join("inbox");

    // create a new Maildir

    let mut arg = None;
    let mut create = MaildirCreate::new(root.clone());

    loop {
        match create.resume(arg.take()) {
            MaildirCreateResult::Ok => break,
            MaildirCreateResult::Io(input) => arg = Some(handle(input).unwrap()),
            MaildirCreateResult::Err(err) => panic!("{err}"),
        }
    }

    let maildir = Maildir::try_from(root).unwrap();

    // store a message in /new

    let contents = b"From: alice@example.com\r\nTo: bob@example.com\r\nSubject: Hello\r\n\r\nHello, world!\r\n".to_vec();

    let mut arg = None;
    let mut store = MaildirMessageStore::new(
        maildir.clone(),
        MaildirSubdir::New,
        Flags::default(),
        contents,
    );

    let (id, path) = loop {
        match store.resume(arg.take()) {
            MaildirMessageStoreResult::Ok { id, path } => break (id, path),
            MaildirMessageStoreResult::Io(input) => arg = Some(handle(input).unwrap()),
            MaildirMessageStoreResult::Err(err) => panic!("{err}"),
        }
    };

    println!("Stored message:");
    println!("  ID:   {id}");
    println!("  Path: {}", path.display());
}
