//! I/O-free coroutine to move a Vdir message.

use io_fs::{
    coroutines::rename::*,
    error::{FsError, FsResult},
    io::FsIo,
};
use thiserror::Error;

use crate::{
    coroutines::locate_message_by_id::*,
    maildir::{Maildir, MaildirSubdir},
    message::INFORMATIONAL_SUFFIX_SEPARATOR,
};

/// Errors that can occur during the coroutine progression.
#[derive(Clone, Debug, Error)]
pub enum MoveMaildirMessageError {
    #[error(transparent)]
    Locate(#[from] LocateMaildirMessageByIdError),
    #[error("Move Maildir message error")]
    Move(#[from] FsError),
}

/// Output emitted when the coroutine terminates its progression.
#[derive(Clone, Debug)]
pub enum MoveMaildirMessageResult {
    /// The coroutine successfully terminated its progression.
    Ok,

    /// The coroutine encountered an error.
    Err(MoveMaildirMessageError),

    /// An I/O needs to be processed in order to make the coroutine
    /// progress further.
    Io(FsIo),
}

#[derive(Debug)]
enum State {
    Locate(LocateMaildirMessageById),
    Move(Rename),
}

/// I/O-free coroutine to move a Vdir message.
#[derive(Debug)]
pub struct MoveMaildirMessage {
    target: Maildir,
    target_subdir: MaildirSubdir,
    id: String,
    state: State,
}

impl MoveMaildirMessage {
    /// Creates a new coroutine from the given collection's path.
    pub fn new(
        source: Maildir,
        target: Maildir,
        target_subdir: MaildirSubdir,
        id: impl ToString,
    ) -> Self {
        let coroutine = LocateMaildirMessageById::new(source.clone(), id.to_string());
        let state = State::Locate(coroutine);

        Self {
            target,
            target_subdir,
            id: id.to_string(),
            state,
        }
    }

    /// Makes the coroutine progress.
    pub fn resume(&mut self, mut arg: Option<FsIo>) -> MoveMaildirMessageResult {
        loop {
            match &mut self.state {
                State::Locate(coroutine) => {
                    let source = match coroutine.resume(arg.take()) {
                        LocateMaildirMessageByIdResult::Ok(path) => path,
                        LocateMaildirMessageByIdResult::Err(err) => {
                            return MoveMaildirMessageResult::Err(err.into())
                        }
                        LocateMaildirMessageByIdResult::Io(io) => {
                            return MoveMaildirMessageResult::Io(io)
                        }
                    };

                    let target = match self.target_subdir {
                        MaildirSubdir::Cur => self.target.cur().join(&self.id).with_file_name(
                            format!("{}{}2,", self.id, INFORMATIONAL_SUFFIX_SEPARATOR),
                        ),
                        MaildirSubdir::New => self.target.new().join(&self.id),
                        MaildirSubdir::Tmp => self.target.tmp().join(&self.id),
                    };

                    self.state = State::Move(Rename::new(Some((source, target))))
                }
                State::Move(coroutine) => {
                    return match coroutine.resume(arg.take()) {
                        FsResult::Ok(()) => MoveMaildirMessageResult::Ok,
                        FsResult::Err(err) => MoveMaildirMessageResult::Err(err.into()),
                        FsResult::Io(io) => MoveMaildirMessageResult::Io(io),
                    }
                }
            }
        }
    }
}
