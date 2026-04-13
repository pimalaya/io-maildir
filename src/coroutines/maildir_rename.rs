//! I/O-free coroutine to rename a Maildir.

use std::path::Path;

use io_fs::{
    coroutines::rename::{FsRename, FsRenameError, FsRenameResult},
    io::{FsInput, FsOutput},
};
use thiserror::Error;

/// Errors that can occur during the coroutine progression.
#[derive(Clone, Debug, Error)]
pub enum MaildirRenameError {
    /// An error occurred while renaming the Maildir directory.
    #[error("Rename Maildir error")]
    Rename(#[source] FsRenameError),
}

/// Output emitted after the coroutine finishes its progression.
#[derive(Clone, Debug)]
pub enum MaildirRenameResult {
    /// The coroutine has successfully terminated its progression.
    Ok,

    /// A filesystem I/O needs to be performed to make the coroutine
    /// progress.
    Io(FsInput),

    /// An error occurred during the coroutine progression.
    Err(MaildirRenameError),
}

/// I/O-free coroutine to rename a Maildir directory.
#[derive(Debug)]
pub struct MaildirRename(FsRename);

impl MaildirRename {
    /// Creates a new coroutine that will rename the Maildir at `path`
    /// to `name` (keeping the same parent directory).
    pub fn new(path: impl AsRef<Path>, name: impl ToString) -> Self {
        let path = path.as_ref();
        let from = path.to_string_lossy().into_owned();
        let to = path
            .with_file_name(name.to_string())
            .to_string_lossy()
            .into_owned();
        Self(FsRename::new([(from, to)]))
    }

    /// Makes the Maildir rename progress.
    pub fn resume(&mut self, arg: Option<FsOutput>) -> MaildirRenameResult {
        match self.0.resume(arg) {
            FsRenameResult::Ok => MaildirRenameResult::Ok,
            FsRenameResult::Io(input) => MaildirRenameResult::Io(input),
            FsRenameResult::Err(err) => MaildirRenameResult::Err(MaildirRenameError::Rename(err)),
        }
    }
}
