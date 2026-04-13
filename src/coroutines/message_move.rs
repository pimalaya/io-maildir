//! I/O-free coroutine to move a Maildir message.

use std::{
    collections::{BTreeMap, BTreeSet},
    mem,
};

use log::trace;
use thiserror::Error;

use crate::{
    coroutines::message_locate::*,
    maildir::{Maildir, MaildirSubdir},
    message::INFORMATIONAL_SUFFIX_SEPARATOR,
};

/// Errors that can occur during the coroutine progression.
#[derive(Clone, Debug, Error)]
pub enum MaildirMessageMoveError {
    #[error("Invalid Maildir message move arg {0:?} for state {1:?}")]
    Invalid(Option<MaildirMessageMoveArg>, State),

    /// The source message could not be located.
    #[error(transparent)]
    Locate(#[from] MaildirMessageLocateError),
}

/// Result returned by [`MaildirMessageMove::resume`].
#[derive(Clone, Debug)]
pub enum MaildirMessageMoveResult {
    /// The coroutine has successfully terminated its progression.
    Ok,

    /// The coroutine wants the caller to read the entries inside the
    /// given directories and feed back [`MaildirMessageMoveArg::DirRead`].
    WantsDirRead(BTreeSet<String>),

    /// The coroutine wants the caller to rename each `(from, to)`
    /// pair and feed back [`MaildirMessageMoveArg::Rename`].
    WantsRename(Vec<(String, String)>),

    /// The coroutine encountered an error.
    Err(MaildirMessageMoveError),
}

/// Internal progression state of [`MaildirMessageMove`].
#[derive(Clone, Debug, Default)]
pub enum State {
    Locate(MaildirMessageLocate),
    Renamed,
    #[default]
    Invalid,
}

/// Argument fed back to [`MaildirMessageMove::resume`] after the
/// caller performed the requested filesystem operation.
#[derive(Clone, Debug)]
pub enum MaildirMessageMoveArg {
    /// Response to [`MaildirMessageMoveResult::WantsDirRead`].
    DirRead(BTreeMap<String, BTreeSet<String>>),

    /// Response to [`MaildirMessageMoveResult::WantsRename`].
    Rename,
}

/// I/O-free coroutine to move a Maildir message to another Maildir.
#[derive(Debug)]
pub struct MaildirMessageMove {
    id: String,
    target: Maildir,
    target_subdir: Option<MaildirSubdir>,
    state: State,
}

impl MaildirMessageMove {
    /// Creates a new coroutine that will move message `id` from
    /// `source` into `target`.
    ///
    /// If `target_subdir` is `None`, the message is placed into the
    /// same subdir as in the source Maildir.
    pub fn new(
        id: impl ToString,
        source: Maildir,
        target: Maildir,
        target_subdir: Option<MaildirSubdir>,
    ) -> Self {
        let id = id.to_string();
        Self {
            state: State::Locate(MaildirMessageLocate::new(source, &id)),
            id,
            target,
            target_subdir,
        }
    }

    /// Makes the message move progress.
    pub fn resume(
        &mut self,
        arg: Option<impl Into<MaildirMessageMoveArg>>,
    ) -> MaildirMessageMoveResult {
        match (mem::take(&mut self.state), arg.map(Into::into)) {
            (State::Locate(mut c), arg) => {
                let locate_arg = match arg {
                    None => None,
                    Some(MaildirMessageMoveArg::DirRead(entries)) => {
                        Some(MaildirMessageLocateArg::DirRead(entries))
                    }
                    Some(other) => {
                        let state = State::Locate(c);
                        let err = MaildirMessageMoveError::Invalid(Some(other), state);
                        return MaildirMessageMoveResult::Err(err);
                    }
                };

                match c.resume(locate_arg) {
                    MaildirMessageLocateResult::Ok { path, subdir, .. } => {
                        trace!("located source at {}", path.display());

                        let target = match self.target_subdir {
                            Some(MaildirSubdir::Cur) => {
                                let name =
                                    format!("{}{}2,", self.id, INFORMATIONAL_SUFFIX_SEPARATOR);
                                self.target.cur().join(&self.id).with_file_name(name)
                            }
                            Some(MaildirSubdir::New) => self.target.new().join(&self.id),
                            Some(MaildirSubdir::Tmp) => self.target.tmp().join(&self.id),
                            None => self.target.subdir(&subdir).join(&self.id),
                        };

                        let pairs = vec![(
                            path.to_string_lossy().into_owned(),
                            target.to_string_lossy().into_owned(),
                        )];
                        self.state = State::Renamed;
                        MaildirMessageMoveResult::WantsRename(pairs)
                    }
                    MaildirMessageLocateResult::WantsDirRead(paths) => {
                        self.state = State::Locate(c);
                        MaildirMessageMoveResult::WantsDirRead(paths)
                    }
                    MaildirMessageLocateResult::Err(err) => {
                        MaildirMessageMoveResult::Err(err.into())
                    }
                }
            }
            (State::Renamed, Some(MaildirMessageMoveArg::Rename)) => {
                trace!("renamed source to target");
                MaildirMessageMoveResult::Ok
            }
            (state, arg) => {
                let err = MaildirMessageMoveError::Invalid(arg, state);
                MaildirMessageMoveResult::Err(err)
            }
        }
    }
}
