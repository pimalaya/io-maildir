//! I/O-free coroutine to list Maildirs inside a root directory.

use std::{collections::HashSet, path::PathBuf};

use io_fs::{
    coroutines::dir_read::{FsDirRead, FsDirReadError, FsDirReadResult},
    io::{FsInput, FsOutput},
};
use log::debug;
use thiserror::Error;

use crate::maildir::Maildir;

/// Errors that can occur during the coroutine progression.
#[derive(Clone, Debug, Error)]
pub enum MaildirListError {
    /// An error occurred while reading the root directory.
    #[error("List Maildirs error")]
    DirRead(#[source] FsDirReadError),
}

/// Output emitted after the coroutine finishes its progression.
#[derive(Clone, Debug)]
pub enum MaildirListResult {
    /// The coroutine has successfully terminated its progression.
    Ok(HashSet<Maildir>),

    /// A filesystem I/O needs to be performed to make the coroutine
    /// progress.
    Io(FsInput),

    /// An error occurred during the coroutine progression.
    Err(MaildirListError),
}

/// I/O-free coroutine to list all valid Maildirs inside a root
/// directory.
///
/// Entries starting with `.` and entries that are not valid Maildirs
/// are silently ignored.
#[derive(Debug)]
pub struct MaildirList(FsDirRead);

impl MaildirList {
    /// Creates a new coroutine that will list Maildirs inside `root`.
    pub fn new(root: impl AsRef<std::path::Path>) -> Self {
        Self(FsDirRead::new([root.as_ref().to_string_lossy()]))
    }

    /// Makes the listing progress.
    pub fn resume(&mut self, arg: Option<FsOutput>) -> MaildirListResult {
        let entries = match self.0.resume(arg) {
            FsDirReadResult::Ok(entries) => entries,
            FsDirReadResult::Io(input) => return MaildirListResult::Io(input),
            FsDirReadResult::Err(err) => {
                return MaildirListResult::Err(MaildirListError::DirRead(err));
            }
        };

        let paths = entries.into_values().next().unwrap_or_default();
        let mut maildirs = HashSet::new();

        for path in paths {
            let path_buf = PathBuf::from(&path);

            let Some(name) = path_buf.file_name().and_then(|n| n.to_str()) else {
                continue;
            };

            if name.starts_with('.') {
                continue;
            }

            match Maildir::try_from(path_buf) {
                Ok(maildir) => {
                    maildirs.insert(maildir);
                }
                Err(err) => {
                    debug!("ignoring invalid maildir at {path}: {err}");
                }
            }
        }

        MaildirListResult::Ok(maildirs)
    }
}
