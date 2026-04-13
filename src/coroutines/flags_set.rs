//! I/O-free coroutine to set (replace) flags on a Maildir message.

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
pub enum MaildirFlagsSetError {
    #[error("Invalid Maildir flags set arg {0:?} for state {1:?}")]
    Invalid(Option<MaildirFlagsSetArg>, State),

    /// The message could not be located.
    #[error(transparent)]
    Locate(#[from] MaildirMessageLocateError),
}

/// Result returned by [`MaildirFlagsSet::resume`].
#[derive(Clone, Debug)]
pub enum MaildirFlagsSetResult {
    /// The coroutine has successfully terminated its progression.
    Ok,

    /// The coroutine wants the caller to read the entries inside the
    /// given directories and feed back [`MaildirFlagsSetArg::DirRead`].
    WantsDirRead(BTreeSet<String>),

    /// The coroutine wants the caller to rename each `(from, to)`
    /// pair and feed back [`MaildirFlagsSetArg::Rename`].
    WantsRename(Vec<(String, String)>),

    /// The coroutine encountered an error.
    Err(MaildirFlagsSetError),
}

/// Internal progression state of [`MaildirFlagsSet`].
#[derive(Clone, Debug, Default)]
pub enum State {
    Locate(MaildirMessageLocate),
    Renamed,
    #[default]
    Invalid,
}

/// Argument fed back to [`MaildirFlagsSet::resume`] after the
/// caller performed the requested filesystem operation.
#[derive(Clone, Debug)]
pub enum MaildirFlagsSetArg {
    /// Response to [`MaildirFlagsSetResult::WantsDirRead`].
    DirRead(BTreeMap<String, BTreeSet<String>>),

    /// Response to [`MaildirFlagsSetResult::WantsRename`].
    Rename,
}

/// I/O-free coroutine to set (replace) the flags of a Maildir
/// message.
///
/// Only messages in `/cur` carry flags; messages in `/new` or `/tmp`
/// are left unchanged.
#[derive(Debug)]
pub struct MaildirFlagsSet {
    state: State,
    id: String,
    flags: Flags,
}

impl MaildirFlagsSet {
    /// Creates a new coroutine that will replace the flags of message
    /// `id` in `maildir` with `flags`.
    pub fn new(maildir: Maildir, id: impl ToString, flags: Flags) -> Self {
        let id = id.to_string();
        Self {
            state: State::Locate(MaildirMessageLocate::new(maildir, &id)),
            id,
            flags,
        }
    }

    /// Makes the flags set progress.
    pub fn resume(&mut self, arg: Option<impl Into<MaildirFlagsSetArg>>) -> MaildirFlagsSetResult {
        match (mem::take(&mut self.state), arg.map(Into::into)) {
            (State::Locate(mut c), arg) => {
                let locate_arg = match arg {
                    None => None,
                    Some(MaildirFlagsSetArg::DirRead(entries)) => {
                        Some(MaildirMessageLocateArg::DirRead(entries))
                    }
                    Some(other) => {
                        let state = State::Locate(c);
                        let err = MaildirFlagsSetError::Invalid(Some(other), state);
                        return MaildirFlagsSetResult::Err(err);
                    }
                };

                match c.resume(locate_arg) {
                    MaildirMessageLocateResult::Ok { path, subdir, .. } => match subdir {
                        MaildirSubdir::New | MaildirSubdir::Tmp => {
                            trace!("message is in /new or /tmp, flags are a no-op");
                            MaildirFlagsSetResult::Ok
                        }
                        MaildirSubdir::Cur => {
                            let mut file_name = self.id.clone();
                            file_name.push(INFORMATIONAL_SUFFIX_SEPARATOR);
                            file_name.push_str("2,");
                            file_name.push_str(&self.flags.to_string());

                            let new_path = path.with_file_name(file_name);
                            trace!("rename {} -> {}", path.display(), new_path.display());

                            let pairs = vec![(
                                path.to_string_lossy().into_owned(),
                                new_path.to_string_lossy().into_owned(),
                            )];
                            self.state = State::Renamed;
                            MaildirFlagsSetResult::WantsRename(pairs)
                        }
                    },
                    MaildirMessageLocateResult::WantsDirRead(paths) => {
                        self.state = State::Locate(c);
                        MaildirFlagsSetResult::WantsDirRead(paths)
                    }
                    MaildirMessageLocateResult::Err(err) => MaildirFlagsSetResult::Err(err.into()),
                }
            }
            (State::Renamed, Some(MaildirFlagsSetArg::Rename)) => MaildirFlagsSetResult::Ok,
            (state, arg) => {
                let err = MaildirFlagsSetError::Invalid(arg, state);
                MaildirFlagsSetResult::Err(err)
            }
        }
    }
}
