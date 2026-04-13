//! I/O-free coroutine to copy a Maildir message.

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
pub enum MaildirMessageCopyError {
    #[error("Invalid Maildir message copy arg {0:?} for state {1:?}")]
    Invalid(Option<MaildirMessageCopyArg>, State),

    /// The source message could not be located.
    #[error(transparent)]
    Locate(#[from] MaildirMessageLocateError),
}

/// Result returned by [`MaildirMessageCopy::resume`].
#[derive(Clone, Debug)]
pub enum MaildirMessageCopyResult {
    /// The coroutine has successfully terminated its progression.
    Ok,

    /// The coroutine wants the caller to read the entries inside the
    /// given directories and feed back [`MaildirMessageCopyArg::DirRead`].
    WantsDirRead(BTreeSet<String>),

    /// The coroutine wants the caller to copy each `(source, target)`
    /// pair and feed back [`MaildirMessageCopyArg::Copy`].
    WantsCopy(Vec<(String, String)>),

    /// The coroutine encountered an error.
    Err(MaildirMessageCopyError),
}

/// Internal progression state of [`MaildirMessageCopy`].
#[derive(Clone, Debug, Default)]
pub enum State {
    Locate(MaildirMessageLocate),
    Copied,
    #[default]
    Invalid,
}

/// Argument fed back to [`MaildirMessageCopy::resume`] after the
/// caller performed the requested filesystem operation.
#[derive(Clone, Debug)]
pub enum MaildirMessageCopyArg {
    /// Response to [`MaildirMessageCopyResult::WantsDirRead`].
    DirRead(BTreeMap<String, BTreeSet<String>>),

    /// Response to [`MaildirMessageCopyResult::WantsCopy`].
    Copy,
}

/// I/O-free coroutine to copy a Maildir message to another Maildir.
#[derive(Debug)]
pub struct MaildirMessageCopy {
    id: String,
    target: Maildir,
    target_subdir: Option<MaildirSubdir>,
    state: State,
}

impl MaildirMessageCopy {
    /// Creates a new coroutine that will copy message `id` from
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

    /// Makes the message copy progress.
    pub fn resume(
        &mut self,
        arg: Option<impl Into<MaildirMessageCopyArg>>,
    ) -> MaildirMessageCopyResult {
        match (mem::take(&mut self.state), arg.map(Into::into)) {
            (State::Locate(mut c), arg) => {
                let locate_arg = match arg {
                    None => None,
                    Some(MaildirMessageCopyArg::DirRead(entries)) => {
                        Some(MaildirMessageLocateArg::DirRead(entries))
                    }
                    Some(other) => {
                        let state = State::Locate(c);
                        let err = MaildirMessageCopyError::Invalid(Some(other), state);
                        return MaildirMessageCopyResult::Err(err);
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
                        self.state = State::Copied;
                        MaildirMessageCopyResult::WantsCopy(pairs)
                    }
                    MaildirMessageLocateResult::WantsDirRead(paths) => {
                        self.state = State::Locate(c);
                        MaildirMessageCopyResult::WantsDirRead(paths)
                    }
                    MaildirMessageLocateResult::Err(err) => {
                        MaildirMessageCopyResult::Err(err.into())
                    }
                }
            }
            (State::Copied, Some(MaildirMessageCopyArg::Copy)) => {
                trace!("copied source to target");
                MaildirMessageCopyResult::Ok
            }
            (state, arg) => {
                let err = MaildirMessageCopyError::Invalid(arg, state);
                MaildirMessageCopyResult::Err(err)
            }
        }
    }
}
