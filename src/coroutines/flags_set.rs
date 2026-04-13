//! I/O-free coroutine to set (replace) flags on a Maildir message.

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
pub enum MaildirFlagsSetError {
    /// The message could not be located.
    #[error(transparent)]
    Locate(#[from] MaildirMessageLocateError),

    /// An error occurred while renaming the message file.
    #[error("Set Maildir flags: rename message file error")]
    Rename(#[from] FsRenameError),
}

/// Output emitted after the coroutine finishes its progression.
#[derive(Clone, Debug)]
pub enum MaildirFlagsSetResult {
    /// The coroutine has successfully terminated its progression.
    Ok,

    /// A filesystem I/O needs to be performed to make the coroutine
    /// progress.
    Io(FsInput),

    /// An error occurred during the coroutine progression.
    Err(MaildirFlagsSetError),
}

#[derive(Debug)]
enum State {
    Locate(MaildirMessageLocate),
    Rename(FsRename),
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
    pub fn resume(&mut self, mut arg: Option<FsOutput>) -> MaildirFlagsSetResult {
        loop {
            match &mut self.state {
                State::Locate(coroutine) => {
                    let (path, subdir) = match coroutine.resume(arg.take()) {
                        MaildirMessageLocateResult::Ok { path, subdir, .. } => (path, subdir),
                        MaildirMessageLocateResult::Io(input) => {
                            return MaildirFlagsSetResult::Io(input);
                        }
                        MaildirMessageLocateResult::Err(err) => {
                            return MaildirFlagsSetResult::Err(err.into());
                        }
                    };

                    match subdir {
                        MaildirSubdir::New | MaildirSubdir::Tmp => {
                            return MaildirFlagsSetResult::Ok;
                        }
                        MaildirSubdir::Cur => {
                            let mut file_name = self.id.clone();
                            file_name.push(INFORMATIONAL_SUFFIX_SEPARATOR);
                            file_name.push_str("2,");
                            file_name.push_str(&self.flags.to_string());

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
                        FsRenameResult::Ok => MaildirFlagsSetResult::Ok,
                        FsRenameResult::Io(input) => MaildirFlagsSetResult::Io(input),
                        FsRenameResult::Err(err) => MaildirFlagsSetResult::Err(err.into()),
                    };
                }
            }
        }
    }
}
