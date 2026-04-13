//! I/O-free coroutine to move a Maildir message.

use io_fs::{
    coroutines::rename::{FsRename, FsRenameError, FsRenameResult},
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
pub enum MaildirMessageMoveError {
    /// The source message could not be located.
    #[error(transparent)]
    Locate(#[from] MaildirMessageLocateError),

    /// An error occurred while moving (renaming) the message file.
    #[error("Move Maildir message file error")]
    Rename(#[from] FsRenameError),
}

/// Output emitted after the coroutine finishes its progression.
#[derive(Clone, Debug)]
pub enum MaildirMessageMoveResult {
    /// The coroutine has successfully terminated its progression.
    Ok,

    /// A filesystem I/O needs to be performed to make the coroutine
    /// progress.
    Io(FsInput),

    /// An error occurred during the coroutine progression.
    Err(MaildirMessageMoveError),
}

#[derive(Debug)]
enum State {
    Locate(MaildirMessageLocate),
    Move(FsRename),
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
        Self {
            id: id.to_string(),
            target,
            target_subdir,
            state: State::Locate(MaildirMessageLocate::new(source, id.to_string())),
        }
    }

    /// Makes the message move progress.
    pub fn resume(&mut self, mut arg: Option<FsOutput>) -> MaildirMessageMoveResult {
        loop {
            match &mut self.state {
                State::Locate(coroutine) => {
                    let (source, subdir) = match coroutine.resume(arg.take()) {
                        MaildirMessageLocateResult::Ok { path, subdir, .. } => (path, subdir),
                        MaildirMessageLocateResult::Io(input) => {
                            return MaildirMessageMoveResult::Io(input);
                        }
                        MaildirMessageLocateResult::Err(err) => {
                            return MaildirMessageMoveResult::Err(err.into());
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

                    self.state = State::Move(FsRename::new([(
                        source.to_string_lossy(),
                        target.to_string_lossy(),
                    )]));
                }
                State::Move(coroutine) => {
                    return match coroutine.resume(arg.take()) {
                        FsRenameResult::Ok => MaildirMessageMoveResult::Ok,
                        FsRenameResult::Io(input) => MaildirMessageMoveResult::Io(input),
                        FsRenameResult::Err(err) => MaildirMessageMoveResult::Err(err.into()),
                    };
                }
            }
        }
    }
}
