//! I/O-free coroutine to create a Maildir.

use std::path::PathBuf;

use io_fs::{
    coroutines::create_dirs::CreateDirs,
    error::{FsError, FsResult},
    io::FsIo,
};
use thiserror::Error;

/// Errors that can occur during the coroutine progression.
#[derive(Clone, Debug, Error)]
pub enum CreateMaildirError {
    #[error("Create Maildir structure error")]
    CreateDirs(#[source] FsError),
}

/// Output emitted when the coroutine terminates its progression.
#[derive(Clone, Debug)]
pub enum CreateMaildirResult {
    /// An I/O needs to be processed in order to make the coroutine
    /// progress further.
    Io(FsIo),

    /// The coroutine successfully terminated its progression.
    Ok,

    /// The coroutine encountered an error.
    Err(CreateMaildirError),
}

/// I/O-free coroutine to create a Vdir maildir.
#[derive(Debug)]
pub struct CreateMaildir(CreateDirs);

impl CreateMaildir {
    /// Creates a new coroutine from the given maildir.
    pub fn new(root: PathBuf) -> Self {
        let tmp = root.join("tmp");
        let new = root.join("new");
        let cur = root.join("cur");
        let coroutine = CreateDirs::new([root, tmp, new, cur]);
        Self(coroutine)
    }

    /// Makes the coroutine progress.
    pub fn resume(&mut self, mut arg: Option<FsIo>) -> CreateMaildirResult {
        match self.0.resume(arg.take()) {
            FsResult::Io(io) => CreateMaildirResult::Io(io),
            FsResult::Ok(()) => CreateMaildirResult::Ok,
            FsResult::Err(err) => {
                let err = CreateMaildirError::CreateDirs(err);
                CreateMaildirResult::Err(err)
            }
        }
    }
}
