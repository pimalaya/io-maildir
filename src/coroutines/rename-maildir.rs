//! I/O-free coroutine to rename a Maildir.

use std::path::PathBuf;

use io_fs::{
    coroutines::rename::Rename,
    error::{FsError, FsResult},
    io::FsIo,
};
use thiserror::Error;

/// Errors that can occur during the coroutine progression.
#[derive(Clone, Debug, Error)]
pub enum RenameMaildirError {
    #[error("Rename Maildir structure error")]
    RenameDirs(#[source] FsError),
}

/// Output emitted when the coroutine terminates its progression.
#[derive(Clone, Debug)]
pub enum RenameMaildirResult {
    /// An I/O needs to be processed in order to make the coroutine
    /// progress further.
    Io(FsIo),

    /// The coroutine successfully terminated its progression.
    Ok,

    /// The coroutine encountered an error.
    Err(RenameMaildirError),
}

/// I/O-free coroutine to rename a Vdir maildir.
#[derive(Debug)]
pub struct RenameMaildir(Rename);

impl RenameMaildir {
    /// Renames a new coroutine from the given maildir.
    pub fn new(path: PathBuf, name: impl ToString) -> Self {
        let new_path = path.with_file_name(name.to_string());
        let coroutine = Rename::new(Some((path, new_path)));
        Self(coroutine)
    }

    /// Makes the coroutine progress.
    pub fn resume(&mut self, mut arg: Option<FsIo>) -> RenameMaildirResult {
        match self.0.resume(arg.take()) {
            FsResult::Io(io) => RenameMaildirResult::Io(io),
            FsResult::Ok(()) => RenameMaildirResult::Ok,
            FsResult::Err(err) => {
                let err = RenameMaildirError::RenameDirs(err);
                RenameMaildirResult::Err(err)
            }
        }
    }
}
