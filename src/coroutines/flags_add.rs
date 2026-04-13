//! I/O-free coroutine to add flags to a Maildir message.

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
pub enum MaildirFlagsAddError {
    #[error("Invalid Maildir flags add arg {0:?} for state {1:?}")]
    Invalid(Option<MaildirFlagsAddArg>, State),

    /// The message could not be located.
    #[error(transparent)]
    Locate(#[from] MaildirMessageLocateError),
}

/// Result returned by [`MaildirFlagsAdd::resume`].
#[derive(Clone, Debug)]
pub enum MaildirFlagsAddResult {
    /// The coroutine has successfully terminated its progression.
    Ok,

    /// The coroutine wants the caller to read the entries inside the
    /// given directories and feed back [`MaildirFlagsAddArg::DirRead`].
    WantsDirRead(BTreeSet<String>),

    /// The coroutine wants the caller to rename each `(from, to)`
    /// pair and feed back [`MaildirFlagsAddArg::Rename`].
    WantsRename(Vec<(String, String)>),

    /// The coroutine encountered an error.
    Err(MaildirFlagsAddError),
}

/// Internal progression state of [`MaildirFlagsAdd`].
#[derive(Clone, Debug, Default)]
pub enum State {
    Locate(MaildirMessageLocate),
    Renamed,
    #[default]
    Invalid,
}

/// Argument fed back to [`MaildirFlagsAdd::resume`] after the
/// caller performed the requested filesystem operation.
#[derive(Clone, Debug)]
pub enum MaildirFlagsAddArg {
    /// Response to [`MaildirFlagsAddResult::WantsDirRead`].
    DirRead(BTreeMap<String, BTreeSet<String>>),

    /// Response to [`MaildirFlagsAddResult::WantsRename`].
    Rename,
}

/// I/O-free coroutine to add flags to a Maildir message.
///
/// Only messages in `/cur` carry flags; messages in `/new` or `/tmp`
/// are left unchanged.
#[derive(Debug)]
pub struct MaildirFlagsAdd {
    state: State,
    id: String,
    flags: Flags,
}

impl MaildirFlagsAdd {
    /// Creates a new coroutine that will add `flags` to message `id`
    /// in `maildir`.
    pub fn new(maildir: Maildir, id: impl ToString, flags: Flags) -> Self {
        let id = id.to_string();
        Self {
            state: State::Locate(MaildirMessageLocate::new(maildir, &id)),
            id,
            flags,
        }
    }

    /// Makes the flags add progress.
    pub fn resume(&mut self, arg: Option<impl Into<MaildirFlagsAddArg>>) -> MaildirFlagsAddResult {
        match (mem::take(&mut self.state), arg.map(Into::into)) {
            (State::Locate(mut c), arg) => {
                let locate_arg = match arg {
                    None => None,
                    Some(MaildirFlagsAddArg::DirRead(entries)) => {
                        Some(MaildirMessageLocateArg::DirRead(entries))
                    }
                    Some(other) => {
                        let state = State::Locate(c);
                        let err = MaildirFlagsAddError::Invalid(Some(other), state);
                        return MaildirFlagsAddResult::Err(err);
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
                            MaildirFlagsAddResult::Ok
                        }
                        MaildirSubdir::Cur => {
                            existing.extend(self.flags.clone());

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
                            MaildirFlagsAddResult::WantsRename(pairs)
                        }
                    },
                    MaildirMessageLocateResult::WantsDirRead(paths) => {
                        self.state = State::Locate(c);
                        MaildirFlagsAddResult::WantsDirRead(paths)
                    }
                    MaildirMessageLocateResult::Err(err) => MaildirFlagsAddResult::Err(err.into()),
                }
            }
            (State::Renamed, Some(MaildirFlagsAddArg::Rename)) => MaildirFlagsAddResult::Ok,
            (state, arg) => {
                let err = MaildirFlagsAddError::Invalid(arg, state);
                MaildirFlagsAddResult::Err(err)
            }
        }
    }
}
