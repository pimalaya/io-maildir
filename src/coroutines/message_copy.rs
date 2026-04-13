//! I/O-free coroutine to copy a Maildir message.

use io_fs::{
    coroutines::copy::{FsCopy, FsCopyError, FsCopyResult},
    io::{FsInput, FsOutput},
};
use thiserror::Error;

use crate::{
    coroutines::message_locate::{
        MaildirMessageLocate, MaildirMessageLocateError, MaildirMessageLocateResult,
    },
    maildir::{Maildir, MaildirSubdir},
    message::INFORMATIONAL_SUFFIX_SEPARATOR,
};

/// Errors that can occur during the coroutine progression.
#[derive(Clone, Debug, Error)]
pub enum MaildirMessageCopyError {
    /// The source message could not be located.
    #[error(transparent)]
    Locate(#[from] MaildirMessageLocateError),

    /// An error occurred while copying the message file.
    #[error("Copy Maildir message file error")]
    Copy(#[from] FsCopyError),
}

/// Output emitted after the coroutine finishes its progression.
#[derive(Clone, Debug)]
pub enum MaildirMessageCopyResult {
    /// The coroutine has successfully terminated its progression.
    Ok,

    /// A filesystem I/O needs to be performed to make the coroutine
    /// progress.
    Io(FsInput),

    /// An error occurred during the coroutine progression.
    Err(MaildirMessageCopyError),
}

#[derive(Debug)]
enum State {
    Locate(MaildirMessageLocate),
    Copy(FsCopy),
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
        Self {
            id: id.to_string(),
            target,
            target_subdir,
            state: State::Locate(MaildirMessageLocate::new(source, id.to_string())),
        }
    }

    /// Makes the message copy progress.
    pub fn resume(&mut self, mut arg: Option<FsOutput>) -> MaildirMessageCopyResult {
        loop {
            match &mut self.state {
                State::Locate(coroutine) => {
                    let (source, subdir) = match coroutine.resume(arg.take()) {
                        MaildirMessageLocateResult::Ok { path, subdir, .. } => (path, subdir),
                        MaildirMessageLocateResult::Io(input) => {
                            return MaildirMessageCopyResult::Io(input);
                        }
                        MaildirMessageLocateResult::Err(err) => {
                            return MaildirMessageCopyResult::Err(err.into());
                        }
                    };

                    let target = match self.target_subdir {
                        Some(MaildirSubdir::Cur) => {
                            let name = format!("{}{}2,", self.id, INFORMATIONAL_SUFFIX_SEPARATOR);
                            self.target.cur().join(&self.id).with_file_name(name)
                        }
                        Some(MaildirSubdir::New) => self.target.new().join(&self.id),
                        Some(MaildirSubdir::Tmp) => self.target.tmp().join(&self.id),
                        None => self.target.subdir(&subdir).join(&self.id),
                    };

                    self.state = State::Copy(FsCopy::new([(
                        source.to_string_lossy(),
                        target.to_string_lossy(),
                    )]));
                }
                State::Copy(coroutine) => {
                    return match coroutine.resume(arg.take()) {
                        FsCopyResult::Ok => MaildirMessageCopyResult::Ok,
                        FsCopyResult::Io(input) => MaildirMessageCopyResult::Io(input),
                        FsCopyResult::Err(err) => MaildirMessageCopyResult::Err(err.into()),
                    };
                }
            }
        }
    }
}
