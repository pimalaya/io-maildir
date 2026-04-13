//! I/O-free coroutine to rename a Maildir.

use std::path::Path;

use log::trace;
use thiserror::Error;

/// Errors that can occur during the coroutine progression.
#[derive(Clone, Debug, Error)]
pub enum MaildirRenameError {
    #[error("Invalid Maildir rename arg: {0:?}")]
    Invalid(Option<MaildirRenameArg>),
}

/// Result returned by [`MaildirRename::resume`].
#[derive(Clone, Debug)]
pub enum MaildirRenameResult {
    /// The coroutine has successfully terminated its progression.
    Ok,

    /// The coroutine wants the caller to rename each `(from, to)`
    /// pair and feed back [`MaildirRenameArg::Rename`].
    WantsRename(Vec<(String, String)>),

    /// The coroutine encountered an error.
    Err(MaildirRenameError),
}

/// Argument fed back to [`MaildirRename::resume`] after the
/// caller performed the requested filesystem operation.
#[derive(Clone, Debug)]
pub enum MaildirRenameArg {
    /// Response to [`MaildirRenameResult::WantsRename`].
    Rename,
}

/// I/O-free coroutine to rename a Maildir directory.
#[derive(Debug)]
pub struct MaildirRename {
    wants_rename: Option<Vec<(String, String)>>,
}

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

        Self {
            wants_rename: Some(vec![(from, to)]),
        }
    }

    /// Makes the Maildir rename progress.
    pub fn resume(&mut self, arg: Option<impl Into<MaildirRenameArg>>) -> MaildirRenameResult {
        match (self.wants_rename.take(), arg.map(Into::into)) {
            (Some(pairs), None) => {
                trace!("wants filesystem I/O to rename {} path(s)", pairs.len());
                MaildirRenameResult::WantsRename(pairs)
            }
            (None, Some(MaildirRenameArg::Rename)) => {
                trace!("resume after renaming Maildir");
                MaildirRenameResult::Ok
            }
            (_, arg) => {
                let err = MaildirRenameError::Invalid(arg);
                MaildirRenameResult::Err(err)
            }
        }
    }
}
