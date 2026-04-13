//! I/O-free coroutine to delete a Maildir.

use std::path::Path;

use io_fs::{
    coroutines::dir_remove::{FsDirRemove, FsDirRemoveError, FsDirRemoveResult},
    io::{FsInput, FsOutput},
};
use thiserror::Error;

/// Errors that can occur during the coroutine progression.
#[derive(Clone, Debug, Error)]
pub enum MaildirDeleteError {
    /// An error occurred while removing the Maildir directory tree.
    #[error("Delete Maildir error")]
    DirRemove(#[source] FsDirRemoveError),
}

/// Output emitted after the coroutine finishes its progression.
#[derive(Clone, Debug)]
pub enum MaildirDeleteResult {
    /// The coroutine has successfully terminated its progression.
    Ok,

    /// A filesystem I/O needs to be performed to make the coroutine
    /// progress.
    Io(FsInput),

    /// An error occurred during the coroutine progression.
    Err(MaildirDeleteError),
}

/// I/O-free coroutine to delete a Maildir and all its contents.
#[derive(Debug)]
pub struct MaildirDelete(FsDirRemove);

impl MaildirDelete {
    /// Creates a new coroutine that will recursively remove the
    /// Maildir at `path`.
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self(FsDirRemove::new([path.as_ref().to_string_lossy()]))
    }

    /// Makes the Maildir deletion progress.
    pub fn resume(&mut self, arg: Option<FsOutput>) -> MaildirDeleteResult {
        match self.0.resume(arg) {
            FsDirRemoveResult::Ok => MaildirDeleteResult::Ok,
            FsDirRemoveResult::Io(input) => MaildirDeleteResult::Io(input),
            FsDirRemoveResult::Err(err) => {
                MaildirDeleteResult::Err(MaildirDeleteError::DirRemove(err))
            }
        }
    }
}
