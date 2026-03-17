//! I/O-free coroutine to delete a Maildir.

use std::path::PathBuf;

use io_fs::{
    coroutines::remove_dir::*,
    error::{FsError, FsResult},
    io::FsIo,
};
use thiserror::Error;

/// Errors that can occur during the coroutine progression.
#[derive(Clone, Debug, Error)]
pub enum DeleteMaildirError {
    #[error("Delete Maildir structure error")]
    DeleteDirs(#[source] FsError),
}

/// Output emitted when the coroutine terminates its progression.
#[derive(Clone, Debug)]
pub enum DeleteMaildirResult {
    /// An I/O needs to be processed in order to make the coroutine
    /// progress further.
    Io(FsIo),

    /// The coroutine successfully terminated its progression.
    Ok,

    /// The coroutine encountered an error.
    Err(DeleteMaildirError),
}

/// I/O-free coroutine to delete a Vdir maildir.
#[derive(Debug)]
pub struct DeleteMaildir(RemoveDir);

impl DeleteMaildir {
    /// Deletes a new coroutine from the given maildir.
    pub fn new(path: PathBuf) -> Self {
        let coroutine = RemoveDir::new(path);
        Self(coroutine)
    }

    /// Makes the coroutine progress.
    pub fn resume(&mut self, mut arg: Option<FsIo>) -> DeleteMaildirResult {
        match self.0.resume(arg.take()) {
            FsResult::Io(io) => DeleteMaildirResult::Io(io),
            FsResult::Ok(()) => DeleteMaildirResult::Ok,
            FsResult::Err(err) => {
                let err = DeleteMaildirError::DeleteDirs(err);
                DeleteMaildirResult::Err(err)
            }
        }
    }
}
