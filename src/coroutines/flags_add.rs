//! I/O-free coroutine to add flags to a Maildir message.

use io_fs::{
    coroutines::rename::{FsRename, FsRenameError, FsRenameResult},
    io::{FsInput, FsOutput},
};
use thiserror::Error;

use crate::{
    coroutines::message_locate::{
        MaildirMessageLocate, MaildirMessageLocateError, MaildirMessageLocateResult,
    },
    flag::Flags,
    maildir::{Maildir, MaildirSubdir},
    message::INFORMATIONAL_SUFFIX_SEPARATOR,
};

/// Errors that can occur during the coroutine progression.
#[derive(Clone, Debug, Error)]
pub enum MaildirFlagsAddError {
    /// The message could not be located.
    #[error(transparent)]
    Locate(#[from] MaildirMessageLocateError),

    /// An error occurred while renaming the message file.
    #[error("Add Maildir flags: rename message file error")]
    Rename(#[from] FsRenameError),
}

/// Output emitted after the coroutine finishes its progression.
#[derive(Clone, Debug)]
pub enum MaildirFlagsAddResult {
    /// The coroutine has successfully terminated its progression.
    Ok,

    /// A filesystem I/O needs to be performed to make the coroutine
    /// progress.
    Io(FsInput),

    /// An error occurred during the coroutine progression.
    Err(MaildirFlagsAddError),
}

#[derive(Debug)]
enum State {
    Locate(MaildirMessageLocate),
    Rename(FsRename),
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
    pub fn resume(&mut self, mut arg: Option<FsOutput>) -> MaildirFlagsAddResult {
        loop {
            match &mut self.state {
                State::Locate(coroutine) => {
                    let (path, subdir, mut existing) = match coroutine.resume(arg.take()) {
                        MaildirMessageLocateResult::Ok {
                            path,
                            subdir,
                            flags,
                        } => (path, subdir, flags),
                        MaildirMessageLocateResult::Io(input) => {
                            return MaildirFlagsAddResult::Io(input);
                        }
                        MaildirMessageLocateResult::Err(err) => {
                            return MaildirFlagsAddResult::Err(err.into());
                        }
                    };

                    match subdir {
                        MaildirSubdir::New | MaildirSubdir::Tmp => {
                            return MaildirFlagsAddResult::Ok;
                        }
                        MaildirSubdir::Cur => {
                            existing.extend(self.flags.clone());

                            let mut file_name = self.id.clone();
                            file_name.push(INFORMATIONAL_SUFFIX_SEPARATOR);
                            file_name.push_str("2,");
                            file_name.push_str(&existing.to_string());

                            let new_path = path.with_file_name(file_name);
                            self.state = State::Rename(FsRename::new([(
                                path.to_string_lossy(),
                                new_path.to_string_lossy(),
                            )]));
                        }
                    }
                }
                State::Rename(coroutine) => {
                    return match coroutine.resume(arg.take()) {
                        FsRenameResult::Ok => MaildirFlagsAddResult::Ok,
                        FsRenameResult::Io(input) => MaildirFlagsAddResult::Io(input),
                        FsRenameResult::Err(err) => MaildirFlagsAddResult::Err(err.into()),
                    };
                }
            }
        }
    }
}
