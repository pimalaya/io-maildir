//! I/O-free coroutine to create a Maildir.

use std::{collections::BTreeSet, path::Path};

use log::trace;
use thiserror::Error;

/// Errors that can occur during the coroutine progression.
#[derive(Clone, Debug, Error)]
pub enum MaildirCreateError {
    #[error("Invalid Maildir create arg: {0:?}")]
    Invalid(Option<MaildirCreateArg>),
}

/// Result returned by [`MaildirCreate::resume`].
#[derive(Clone, Debug)]
pub enum MaildirCreateResult {
    /// The coroutine has successfully terminated its progression.
    Ok,

    /// The coroutine wants the caller to create the given directories
    /// and feed back [`MaildirCreateArg::DirCreate`].
    WantsDirCreate(BTreeSet<String>),

    /// The coroutine encountered an error.
    Err(MaildirCreateError),
}

/// Argument fed back to [`MaildirCreate::resume`] after the
/// caller performed the requested filesystem operation.
#[derive(Clone, Debug)]
pub enum MaildirCreateArg {
    /// Response to [`MaildirCreateResult::WantsDirCreate`].
    DirCreate,
}

/// I/O-free coroutine to create a Maildir with its `cur`, `new` and
/// `tmp` subdirectories.
///
/// All four directories are created in a single I/O request. The
/// [`BTreeSet`] guarantees lexicographic order, so `root` is always
/// created before its subdirectories.
#[derive(Debug)]
pub struct MaildirCreate {
    wants_dir_create: Option<BTreeSet<String>>,
}

impl MaildirCreate {
    /// Creates a new coroutine that will initialise a Maildir rooted
    /// at `root`.
    pub fn new(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref();
        let cur = root.join("cur");
        let new = root.join("new");
        let tmp = root.join("tmp");

        let paths = BTreeSet::from_iter([
            root.to_string_lossy().into_owned(),
            cur.to_string_lossy().into_owned(),
            new.to_string_lossy().into_owned(),
            tmp.to_string_lossy().into_owned(),
        ]);

        Self {
            wants_dir_create: Some(paths),
        }
    }

    /// Makes the Maildir creation progress.
    pub fn resume(&mut self, arg: Option<impl Into<MaildirCreateArg>>) -> MaildirCreateResult {
        match (self.wants_dir_create.take(), arg.map(Into::into)) {
            (Some(paths), None) => {
                trace!("wants filesystem I/O to create {} directories", paths.len());
                MaildirCreateResult::WantsDirCreate(paths)
            }
            (None, Some(MaildirCreateArg::DirCreate)) => {
                trace!("resume after creating Maildir directories");
                MaildirCreateResult::Ok
            }
            (_, arg) => {
                let err = MaildirCreateError::Invalid(arg);
                MaildirCreateResult::Err(err)
            }
        }
    }
}
