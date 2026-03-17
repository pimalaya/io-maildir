//! I/O-free coroutine to get a Vdir message.

use std::path::PathBuf;

use io_fs::{
    coroutines::read_file::*,
    error::{FsError, FsResult},
    io::FsIo,
};
use thiserror::Error;

use crate::{coroutines::locate_message_by_id::*, maildir::Maildir, message::Message};

/// Errors that can occur during the coroutine progression.
#[derive(Clone, Debug, Error)]
pub enum GetMaildirMessageError {
    #[error(transparent)]
    Locate(#[from] LocateMaildirMessageByIdError),
    #[error("Get Maildir message error")]
    Get(#[from] FsError),
}

/// Output emitted when the coroutine terminates its progression.
#[derive(Clone, Debug)]
pub enum GetMaildirMessageResult {
    /// The coroutine successfully terminated its progression.
    Ok(Message),

    /// The coroutine encountered an error.
    Err(GetMaildirMessageError),

    /// An I/O needs to be processed in order to make the coroutine
    /// progress further.
    Io(FsIo),
}

#[derive(Debug)]
enum State {
    Locate(LocateMaildirMessageById),
    Get { path: PathBuf, coroutine: ReadFile },
}

/// I/O-free coroutine to get a Vdir message.
#[derive(Debug)]
pub struct GetMaildirMessage {
    state: State,
}

impl GetMaildirMessage {
    /// Creates a new coroutine from the given collection's path.
    pub fn new(maildir: Maildir, id: impl ToString) -> Self {
        let coroutine = LocateMaildirMessageById::new(maildir, id.to_string());
        let state = State::Locate(coroutine);
        Self { state }
    }

    /// Makes the coroutine progress.
    pub fn resume(&mut self, mut arg: Option<FsIo>) -> GetMaildirMessageResult {
        loop {
            match &mut self.state {
                State::Locate(coroutine) => {
                    let path = match coroutine.resume(arg.take()) {
                        LocateMaildirMessageByIdResult::Ok { path, .. } => path,
                        LocateMaildirMessageByIdResult::Err { err } => {
                            break GetMaildirMessageResult::Err(err.into())
                        }
                        LocateMaildirMessageByIdResult::Io { io } => {
                            break GetMaildirMessageResult::Io(io)
                        }
                    };

                    self.state = State::Get {
                        path: path.clone(),
                        coroutine: ReadFile::new(path),
                    };
                }
                State::Get { path, coroutine } => {
                    let contents = match coroutine.resume(arg.take()) {
                        FsResult::Ok(contents) => contents,
                        FsResult::Err(err) => break GetMaildirMessageResult::Err(err.into()),
                        FsResult::Io(io) => break GetMaildirMessageResult::Io(io),
                    };

                    let message = Message {
                        path: path.clone(),
                        contents,
                    };

                    break GetMaildirMessageResult::Ok(message);
                }
            }
        }
    }
}
