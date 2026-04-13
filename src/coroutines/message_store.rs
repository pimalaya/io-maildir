//! I/O-free coroutine to store a message in a Maildir.

use std::{
    path::PathBuf,
    process,
    sync::atomic::{AtomicUsize, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use gethostname::gethostname;
use io_fs::{
    coroutines::{
        file_create::{FsFileCreate, FsFileCreateError, FsFileCreateResult},
        rename::{FsRename, FsRenameError, FsRenameResult},
    },
    io::{FsInput, FsOutput},
};
use thiserror::Error;

use crate::{
    flag::Flags,
    maildir::{Maildir, MaildirSubdir},
    message::INFORMATIONAL_SUFFIX_SEPARATOR,
};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Errors that can occur during the coroutine progression.
#[derive(Clone, Debug, Error)]
pub enum MaildirMessageStoreError {
    /// An error occurred while creating the temporary file.
    #[error("Store Maildir message: write tmp file error")]
    CreateFile(#[source] FsFileCreateError),

    /// An error occurred while moving the temporary file to its final
    /// location.
    #[error("Store Maildir message: rename tmp to final error")]
    Rename(#[source] FsRenameError),
}

/// Output emitted after the coroutine finishes its progression.
#[derive(Clone, Debug)]
pub enum MaildirMessageStoreResult {
    /// The coroutine has successfully terminated its progression.
    Ok { id: String, path: PathBuf },

    /// A filesystem I/O needs to be performed to make the coroutine
    /// progress.
    Io(FsInput),

    /// An error occurred during the coroutine progression.
    Err(MaildirMessageStoreError),
}

#[derive(Debug)]
enum State {
    Create(FsFileCreate),
    Rename(FsRename),
}

/// I/O-free coroutine to store a message in a Maildir.
///
/// Follows the Maildir delivery protocol: write to `/tmp` first, then
/// atomically rename to the target subdir (`/new` or `/cur`).
#[derive(Debug)]
pub struct MaildirMessageStore {
    id: String,
    tmp_path: String,
    final_path: String,
    state: State,
}

impl MaildirMessageStore {
    /// Creates a new coroutine that will store `contents` as a new
    /// message in `maildir` under `subdir` with the given `flags`.
    pub fn new(maildir: Maildir, subdir: MaildirSubdir, flags: Flags, contents: Vec<u8>) -> Self {
        let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        let secs = ts.as_secs();
        let nanos = ts.subsec_nanos();
        let counter = COUNTER.fetch_add(1, Ordering::SeqCst);
        let pid = process::id();
        let hostname = gethostname().into_string().unwrap_or_default();
        let id = format!("{secs}.#{counter:x}M{nanos}P{pid}.{hostname}");

        let mut id_with_info = id.clone();
        if let MaildirSubdir::Cur = subdir {
            id_with_info.push(INFORMATIONAL_SUFFIX_SEPARATOR);
            id_with_info.push_str("2,");
            id_with_info.push_str(&flags.to_string());
        }

        let tmp_path = format!("{}/{id}", maildir.tmp().to_string_lossy());
        let final_path = format!(
            "{}/{id_with_info}",
            maildir.subdir(&subdir).to_string_lossy()
        );

        let state = State::Create(FsFileCreate::new([(tmp_path.clone(), contents)]));

        Self {
            id,
            tmp_path,
            final_path,
            state,
        }
    }

    /// Makes the message store progress.
    pub fn resume(&mut self, mut arg: Option<FsOutput>) -> MaildirMessageStoreResult {
        loop {
            match &mut self.state {
                State::Create(coroutine) => {
                    match coroutine.resume(arg.take()) {
                        FsFileCreateResult::Ok => {}
                        FsFileCreateResult::Io(input) => {
                            return MaildirMessageStoreResult::Io(input);
                        }
                        FsFileCreateResult::Err(err) => {
                            return MaildirMessageStoreResult::Err(
                                MaildirMessageStoreError::CreateFile(err),
                            );
                        }
                    }

                    self.state = State::Rename(FsRename::new([(
                        self.tmp_path.clone(),
                        self.final_path.clone(),
                    )]));
                }
                State::Rename(coroutine) => {
                    return match coroutine.resume(arg.take()) {
                        FsRenameResult::Ok => MaildirMessageStoreResult::Ok {
                            id: self.id.clone(),
                            path: PathBuf::from(&self.final_path),
                        },
                        FsRenameResult::Io(input) => MaildirMessageStoreResult::Io(input),
                        FsRenameResult::Err(err) => {
                            MaildirMessageStoreResult::Err(MaildirMessageStoreError::Rename(err))
                        }
                    };
                }
            }
        }
    }
}
