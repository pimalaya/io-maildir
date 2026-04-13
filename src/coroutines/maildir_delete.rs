//! I/O-free coroutine to delete a Maildir.

use std::{collections::BTreeSet, path::Path};

use log::trace;
use thiserror::Error;

/// Errors that can occur during the coroutine progression.
#[derive(Clone, Debug, Error)]
pub enum MaildirDeleteError {
    #[error("Invalid Maildir delete arg: {0:?}")]
    Invalid(Option<MaildirDeleteArg>),
}

/// Result returned by [`MaildirDelete::resume`].
#[derive(Clone, Debug)]
pub enum MaildirDeleteResult {
    /// The coroutine has successfully terminated its progression.
    Ok,

    /// The coroutine wants the caller to recursively remove the given
    /// directories and feed back [`MaildirDeleteArg::DirRemove`].
    WantsDirRemove(BTreeSet<String>),

    /// The coroutine encountered an error.
    Err(MaildirDeleteError),
}

/// Argument fed back to [`MaildirDelete::resume`] after the
/// caller performed the requested filesystem operation.
#[derive(Clone, Debug)]
pub enum MaildirDeleteArg {
    /// Response to [`MaildirDeleteResult::WantsDirRemove`].
    DirRemove,
}

/// I/O-free coroutine to delete a Maildir and all its contents.
#[derive(Debug)]
pub struct MaildirDelete {
    wants_dir_remove: Option<BTreeSet<String>>,
}

impl MaildirDelete {
    /// Creates a new coroutine that will recursively remove the
    /// Maildir at `path`.
    pub fn new(path: impl AsRef<Path>) -> Self {
        let paths = BTreeSet::from_iter([path.as_ref().to_string_lossy().into_owned()]);
        Self {
            wants_dir_remove: Some(paths),
        }
    }

    /// Makes the Maildir deletion progress.
    pub fn resume(&mut self, arg: Option<impl Into<MaildirDeleteArg>>) -> MaildirDeleteResult {
        match (self.wants_dir_remove.take(), arg.map(Into::into)) {
            (Some(paths), None) => {
                trace!("wants filesystem I/O to remove {} directories", paths.len());
                MaildirDeleteResult::WantsDirRemove(paths)
            }
            (None, Some(MaildirDeleteArg::DirRemove)) => {
                trace!("resume after removing Maildir");
                MaildirDeleteResult::Ok
            }
            (_, arg) => {
                let err = MaildirDeleteError::Invalid(arg);
                MaildirDeleteResult::Err(err)
            }
        }
    }
}
