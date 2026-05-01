//! I/O-free coroutine to remove flags from a Maildir message.

use std::{
    collections::{BTreeMap, BTreeSet},
    mem,
};

use log::trace;
use thiserror::Error;

use crate::{
    coroutines::message_locate::*,
    flag::Flags,
    maildir::{Maildir, MaildirSubdir},
    message::INFORMATIONAL_SUFFIX_SEPARATOR,
};

/// Errors that can occur during the coroutine progression.
#[derive(Clone, Debug, Error)]
pub enum MaildirFlagsRemoveError {
    #[error("Invalid Maildir flags remove arg {0:?} for state {1:?}")]
    Invalid(Option<MaildirFlagsRemoveArg>, State),

    /// The message could not be located.
    #[error(transparent)]
    Locate(#[from] MaildirMessageLocateError),
}

/// Result returned by [`MaildirFlagsRemove::resume`].
#[derive(Clone, Debug)]
pub enum MaildirFlagsRemoveResult {
    /// The coroutine has successfully terminated its progression.
    Ok,

    /// The coroutine wants the caller to read the entries inside the
    /// given directories and feed back [`MaildirFlagsRemoveArg::DirRead`].
    WantsDirRead(BTreeSet<String>),

    /// The coroutine wants the caller to rename each `(from, to)`
    /// pair and feed back [`MaildirFlagsRemoveArg::Rename`].
    WantsRename(Vec<(String, String)>),

    /// The coroutine encountered an error.
    Err(MaildirFlagsRemoveError),
}

/// Internal progression state of [`MaildirFlagsRemove`].
#[derive(Clone, Debug, Default)]
pub enum State {
    Locate(MaildirMessageLocate),
    Renamed,
    #[default]
    Invalid,
}

/// Argument fed back to [`MaildirFlagsRemove::resume`] after the
/// caller performed the requested filesystem operation.
#[derive(Clone, Debug)]
pub enum MaildirFlagsRemoveArg {
    /// Response to [`MaildirFlagsRemoveResult::WantsDirRead`].
    DirRead(BTreeMap<String, BTreeSet<String>>),

    /// Response to [`MaildirFlagsRemoveResult::WantsRename`].
    Rename,
}

/// I/O-free coroutine to remove flags from a Maildir message.
///
/// Only messages in `/cur` carry flags; messages in `/new` or `/tmp`
/// are left unchanged.
#[derive(Debug)]
pub struct MaildirFlagsRemove {
    state: State,
    id: String,
    flags: Flags,
}

impl MaildirFlagsRemove {
    /// Creates a new coroutine that will remove `flags` from message
    /// `id` in `maildir`.
    pub fn new(maildir: Maildir, id: impl ToString, flags: Flags) -> Self {
        let id = id.to_string();
        Self {
            state: State::Locate(MaildirMessageLocate::new(maildir, &id)),
            id,
            flags,
        }
    }

    /// Makes the flags remove progress.
    pub fn resume(
        &mut self,
        arg: Option<impl Into<MaildirFlagsRemoveArg>>,
    ) -> MaildirFlagsRemoveResult {
        match (mem::take(&mut self.state), arg.map(Into::into)) {
            (State::Locate(mut c), arg) => {
                let locate_arg = match arg {
                    None => None,
                    Some(MaildirFlagsRemoveArg::DirRead(entries)) => {
                        Some(MaildirMessageLocateArg::DirRead(entries))
                    }
                    Some(other) => {
                        let state = State::Locate(c);
                        let err = MaildirFlagsRemoveError::Invalid(Some(other), state);
                        return MaildirFlagsRemoveResult::Err(err);
                    }
                };

                match c.resume(locate_arg) {
                    MaildirMessageLocateResult::Ok {
                        path,
                        subdir,
                        flags: mut existing,
                    } => match subdir {
                        MaildirSubdir::New | MaildirSubdir::Tmp => {
                            trace!("message is in /new or /tmp, flags are a no-op");
                            MaildirFlagsRemoveResult::Ok
                        }
                        MaildirSubdir::Cur => {
                            existing.difference(&self.flags);

                            let mut file_name = self.id.clone();
                            file_name.push(INFORMATIONAL_SUFFIX_SEPARATOR);
                            file_name.push_str("2,");
                            file_name.push_str(&existing.to_string());

                            let new_path = path.with_file_name(file_name);
                            trace!("rename {} -> {}", path.display(), new_path.display());

                            let pairs = vec![(
                                path.to_string_lossy().into_owned(),
                                new_path.to_string_lossy().into_owned(),
                            )];
                            self.state = State::Renamed;
                            MaildirFlagsRemoveResult::WantsRename(pairs)
                        }
                    },
                    MaildirMessageLocateResult::WantsDirRead(paths) => {
                        self.state = State::Locate(c);
                        MaildirFlagsRemoveResult::WantsDirRead(paths)
                    }
                    MaildirMessageLocateResult::Err(err) => {
                        MaildirFlagsRemoveResult::Err(err.into())
                    }
                }
            }
            (State::Renamed, Some(MaildirFlagsRemoveArg::Rename)) => MaildirFlagsRemoveResult::Ok,
            (state, arg) => {
                let err = MaildirFlagsRemoveError::Invalid(arg, state);
                MaildirFlagsRemoveResult::Err(err)
            }
        }
    }
}
