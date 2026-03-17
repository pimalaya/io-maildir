//! I/O-free coroutine to list maildirs in a Vdir collection.

use std::{collections::HashSet, path::PathBuf};

use io_fs::{
    coroutines::read_dir::ReadDir,
    error::{FsError, FsResult},
    io::FsIo,
};
use log::debug;
use thiserror::Error;

use crate::maildir::Maildir;

/// Errors that can occur during the coroutine progression.
#[derive(Clone, Debug, Error)]
pub enum ListMaildirsError {
    /// An error occured during the directory listing.
    #[error("List Vdir maildirs error")]
    ListDirsError(#[source] FsError),

    /// An error occured during the metadata files listing.
    #[error("Read Vdir maildirs' metadata error")]
    ListFilesError(#[source] FsError),
}

/// Output emitted when the coroutine terminates its progression.
#[derive(Clone, Debug)]
pub enum ListMaildirsResult {
    /// The coroutine successfully terminated its progression.
    Ok(HashSet<Maildir>),

    /// The coroutine encountered an error.
    Err(ListMaildirsError),

    /// An I/O needs to be processed in order to make the coroutine
    /// progress further.
    Io(FsIo),
}

/// I/O-free coroutine to list maildirs in a Vdir collection.
#[derive(Debug)]
pub struct ListMaildirs(ReadDir);

impl ListMaildirs {
    /// Creates a new coroutine from the given addressbook path.
    pub fn new(root: PathBuf) -> Self {
        Self(ReadDir::new(root))
    }

    /// Makes the coroutine progress.
    pub fn resume(&mut self, arg: Option<FsIo>) -> ListMaildirsResult {
        let paths = match self.0.resume(arg) {
            FsResult::Ok(paths) => paths,
            FsResult::Io(io) => return ListMaildirsResult::Io(io),
            FsResult::Err(err) => {
                let err = ListMaildirsError::ListDirsError(err);
                return ListMaildirsResult::Err(err);
            }
        };

        let mut maildirs = HashSet::new();

        for path in paths {
            let Some(name) = path.file_name() else {
                continue;
            };

            let Some(name) = name.to_str() else {
                continue;
            };

            if name.starts_with('.') {
                continue;
            }

            match Maildir::try_from(path.clone()) {
                Ok(maildir) => {
                    maildirs.insert(maildir);
                }
                Err(err) => {
                    debug!("ignoring invalid maildir at {}: {err}", path.display());
                }
            }
        }

        ListMaildirsResult::Ok(maildirs)
    }
}
