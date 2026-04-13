//! I/O-free coroutine to create a Maildir.

use std::path::Path;

use io_fs::{
    coroutines::dir_create::{FsDirCreate, FsDirCreateError, FsDirCreateResult},
    io::{FsInput, FsOutput},
};
use thiserror::Error;

/// Errors that can occur during the coroutine progression.
#[derive(Clone, Debug, Error)]
pub enum MaildirCreateError {
    /// An error occurred while creating the Maildir directory
    /// structure.
    #[error("Create Maildir structure error")]
    DirCreate(#[source] FsDirCreateError),
}

/// Output emitted after the coroutine finishes its progression.
#[derive(Clone, Debug)]
pub enum MaildirCreateResult {
    /// The coroutine has successfully terminated its progression.
    Ok,

    /// A filesystem I/O needs to be performed to make the coroutine
    /// progress.
    Io(FsInput),

    /// An error occurred during the coroutine progression.
    Err(MaildirCreateError),
}

/// I/O-free coroutine to create a Maildir with its `cur`, `new` and
/// `tmp` subdirectories.
///
/// All four directories are created in a single I/O request. The
/// [`BTreeSet`] inside [`FsDirCreate`] guarantees lexicographic order,
/// so `root` is always created before its subdirectories.
///
/// [`BTreeSet`]: alloc::collections::BTreeSet
#[derive(Debug)]
pub struct MaildirCreate(FsDirCreate);

impl MaildirCreate {
    /// Creates a new coroutine that will initialise a Maildir rooted
    /// at `root`.
    pub fn new(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref();
        let cur = root.join("cur");
        let new = root.join("new");
        let tmp = root.join("tmp");
        Self(FsDirCreate::new([
            root.to_string_lossy(),
            cur.to_string_lossy(),
            new.to_string_lossy(),
            tmp.to_string_lossy(),
        ]))
    }

    /// Makes the Maildir creation progress.
    pub fn resume(&mut self, arg: Option<FsOutput>) -> MaildirCreateResult {
        match self.0.resume(arg) {
            FsDirCreateResult::Ok => MaildirCreateResult::Ok,
            FsDirCreateResult::Io(input) => MaildirCreateResult::Io(input),
            FsDirCreateResult::Err(err) => {
                MaildirCreateResult::Err(MaildirCreateError::DirCreate(err))
            }
        }
    }
}
