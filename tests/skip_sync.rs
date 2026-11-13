#![cfg(feature = "client")]

use std::{collections::BTreeSet, fs, path::Path};

use io_maildir::{
    client::MaildirClient,
    coroutine::{MaildirCoroutine, MaildirCoroutineState, MaildirReply, MaildirYield},
    entry::store::MaildirEntryStoreError,
    flag::MaildirFlags,
    maildir::MaildirSubdir,
    path::MaildirFsPath,
};
use tempfile::tempdir;

/// Asks for a file sync then a directory sync of `path`.
struct Syncs {
    path: MaildirFsPath,
    step: u8,
}

impl MaildirCoroutine for Syncs {
    type Yield = MaildirYield;
    type Return = Result<(), MaildirEntryStoreError>;

    fn resume(
        &mut self,
        arg: Option<MaildirReply>,
    ) -> MaildirCoroutineState<Self::Yield, Self::Return> {
        let paths = BTreeSet::from([self.path.clone()]);
        self.step += 1;

        match (self.step, arg) {
            (1, None) => MaildirCoroutineState::Yielded(MaildirYield::WantsFileSync(paths)),
            (2, Some(MaildirReply::FileSync)) => {
                MaildirCoroutineState::Yielded(MaildirYield::WantsDirSync(paths))
            }
            (3, Some(MaildirReply::DirSync)) => MaildirCoroutineState::Complete(Ok(())),
            (_, arg) => {
                let err = MaildirEntryStoreError::UnexpectedArg(arg);
                MaildirCoroutineState::Complete(Err(err))
            }
        }
    }
}

fn syncs(root: &Path) -> Syncs {
    let path = root.join("missing").to_string_lossy().into_owned();
    let path = MaildirFsPath::from(path);
    Syncs { path, step: 0 }
}

#[test]
fn skip_sync_acknowledges_the_requests_without_touching_the_path() {
    let dir = tempdir().unwrap();
    let mut client = MaildirClient::new(dir.path().to_string_lossy().into_owned());

    let err = client.run(syncs(dir.path())).unwrap_err().to_string();
    assert!(err.contains("missing"), "{err}");

    client.skip_sync = true;
    client.run(syncs(dir.path())).unwrap();
}

#[test]
fn a_delivery_completes_with_and_without_the_flushes() {
    for skip_sync in [false, true] {
        let dir = tempdir().unwrap();
        for subdir in ["cur", "new", "tmp"] {
            fs::create_dir_all(dir.path().join(subdir)).unwrap();
        }

        let mut client = MaildirClient::new(dir.path().to_string_lossy().into_owned());
        client.skip_sync = skip_sync;
        let maildir = client.load_maildir("").unwrap();

        let (_, path) = client
            .store(
                maildir,
                MaildirSubdir::New,
                MaildirFlags::default(),
                b"hello".to_vec(),
            )
            .unwrap();
        assert_eq!(fs::read(path.as_str()).unwrap(), b"hello");
    }
}
