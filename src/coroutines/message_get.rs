//! I/O-free coroutine to get a Maildir message by its ID.

use std::{
    collections::{BTreeMap, BTreeSet},
    mem,
    path::PathBuf,
};

use log::trace;
use thiserror::Error;

use crate::{coroutines::message_locate::*, maildir::Maildir, message::Message};

/// Errors that can occur during the coroutine progression.
#[derive(Clone, Debug, Error)]
pub enum MaildirMessageGetError {
    #[error("Invalid Maildir message get arg {0:?} for state {1:?}")]
    Invalid(Option<MaildirMessageGetArg>, State),

    /// The message could not be located in the Maildir.
    #[error(transparent)]
    Locate(#[from] MaildirMessageLocateError),
}

/// Result returned by [`MaildirMessageGet::resume`].
#[derive(Clone, Debug)]
pub enum MaildirMessageGetResult {
    /// The coroutine has successfully terminated its progression.
    Ok(Message),

    /// The coroutine wants the caller to read the entries inside the
    /// given directories and feed back [`MaildirMessageGetArg::DirRead`].
    WantsDirRead(BTreeSet<String>),

    /// The coroutine wants the caller to read the contents of the
    /// given files and feed back [`MaildirMessageGetArg::FileRead`].
    WantsFileRead(BTreeSet<String>),

    /// The coroutine encountered an error.
    Err(MaildirMessageGetError),
}

/// Internal progression state of [`MaildirMessageGet`].
#[derive(Clone, Debug, Default)]
pub enum State {
    Locate(MaildirMessageLocate),
    Read(PathBuf),
    #[default]
    Invalid,
}

/// Argument fed back to [`MaildirMessageGet::resume`] after the
/// caller performed the requested filesystem operation.
///
/// Each variant matches one of the `Wants*` results emitted by this
/// coroutine.
#[derive(Clone, Debug)]
pub enum MaildirMessageGetArg {
    /// Response to [`MaildirMessageGetResult::WantsDirRead`].
    ///
    /// Maps each requested directory path to the set of entry paths
    /// found inside it.
    DirRead(BTreeMap<String, BTreeSet<String>>),

    /// Response to [`MaildirMessageGetResult::WantsFileRead`].
    ///
    /// Maps each requested file path to its raw contents.
    FileRead(BTreeMap<String, Vec<u8>>),
}

/// I/O-free coroutine to get a single Maildir message by its ID.
#[derive(Debug)]
pub struct MaildirMessageGet {
    state: State,
}

impl MaildirMessageGet {
    /// Creates a new coroutine that will retrieve message `id` from
    /// `maildir`.
    pub fn new(maildir: Maildir, id: impl ToString) -> Self {
        Self {
            state: State::Locate(MaildirMessageLocate::new(maildir, id)),
        }
    }

    /// Makes the message get progress.
    pub fn resume(
        &mut self,
        arg: Option<impl Into<MaildirMessageGetArg>>,
    ) -> MaildirMessageGetResult {
        match (mem::take(&mut self.state), arg.map(Into::into)) {
            (State::Locate(mut c), arg) => {
                let locate_arg = match arg {
                    None => None,
                    Some(MaildirMessageGetArg::DirRead(entries)) => {
                        Some(MaildirMessageLocateArg::DirRead(entries))
                    }
                    Some(other) => {
                        let state = State::Locate(c);
                        let err = MaildirMessageGetError::Invalid(Some(other), state);
                        return MaildirMessageGetResult::Err(err);
                    }
                };

                match c.resume(locate_arg) {
                    MaildirMessageLocateResult::Ok { path, .. } => {
                        trace!("located message at {}", path.display());

                        let paths = BTreeSet::from_iter([path.to_string_lossy().into_owned()]);
                        self.state = State::Read(path);
                        MaildirMessageGetResult::WantsFileRead(paths)
                    }
                    MaildirMessageLocateResult::WantsDirRead(paths) => {
                        self.state = State::Locate(c);
                        MaildirMessageGetResult::WantsDirRead(paths)
                    }
                    MaildirMessageLocateResult::Err(err) => {
                        MaildirMessageGetResult::Err(err.into())
                    }
                }
            }
            (State::Read(path), Some(MaildirMessageGetArg::FileRead(map))) => {
                trace!("read message contents at {}", path.display());

                let contents = map.into_values().next().unwrap_or_default();
                MaildirMessageGetResult::Ok(Message::from((path, contents)))
            }
            (state, arg) => {
                let err = MaildirMessageGetError::Invalid(arg, state);
                MaildirMessageGetResult::Err(err)
            }
        }
    }
}
