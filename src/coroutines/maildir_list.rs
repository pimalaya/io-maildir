//! I/O-free coroutine to list Maildirs inside a root directory.

use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    path::{Path, PathBuf},
};

use log::trace;
use thiserror::Error;

use crate::maildir::Maildir;

/// Errors that can occur during the coroutine progression.
#[derive(Clone, Debug, Error)]
pub enum MaildirListError {
    #[error("Invalid Maildir list arg: {0:?}")]
    Invalid(Option<MaildirListArg>),
}

/// Result returned by [`MaildirList::resume`].
#[derive(Clone, Debug)]
pub enum MaildirListResult {
    /// The coroutine has successfully terminated its progression.
    Ok(HashSet<Maildir>),

    /// The coroutine wants the caller to read the entries inside the
    /// given directories and feed back [`MaildirListArg::DirRead`].
    WantsDirRead(BTreeSet<String>),

    /// The coroutine encountered an error.
    Err(MaildirListError),
}

/// Argument fed back to [`MaildirList::resume`] after the
/// caller performed the requested filesystem operation.
#[derive(Clone, Debug)]
pub enum MaildirListArg {
    /// Response to [`MaildirListResult::WantsDirRead`].
    ///
    /// Maps each requested directory path to the set of entry paths
    /// found inside it.
    DirRead(BTreeMap<String, BTreeSet<String>>),
}

/// I/O-free coroutine to list all valid Maildirs inside a root
/// directory.
///
/// Entries starting with `.` and entries that are not valid Maildirs
/// are silently ignored.
#[derive(Debug)]
pub struct MaildirList {
    wants_dir_read: Option<BTreeSet<String>>,
}

impl MaildirList {
    /// Creates a new coroutine that will list Maildirs inside `root`.
    pub fn new(root: impl AsRef<Path>) -> Self {
        let paths = BTreeSet::from_iter([root.as_ref().to_string_lossy().into_owned()]);
        Self {
            wants_dir_read: Some(paths),
        }
    }

    /// Makes the listing progress.
    pub fn resume(&mut self, arg: Option<impl Into<MaildirListArg>>) -> MaildirListResult {
        match (self.wants_dir_read.take(), arg.map(Into::into)) {
            (Some(paths), None) => {
                trace!("wants filesystem I/O to read {} directories", paths.len());
                MaildirListResult::WantsDirRead(paths)
            }
            (None, Some(MaildirListArg::DirRead(entries))) => {
                trace!("resume after listing Maildirs");

                let entries = entries.into_values().next().unwrap_or_default();
                let mut maildirs = HashSet::new();

                for path in entries {
                    let path = PathBuf::from(&path);

                    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
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
                            trace!("ignoring invalid Maildir at {}: {err}", path.display());
                        }
                    }
                }

                MaildirListResult::Ok(maildirs)
            }
            (_, arg) => {
                let err = MaildirListError::Invalid(arg);
                MaildirListResult::Err(err)
            }
        }
    }
}
