//! I/O-free coroutine to get a Maildir message by its ID.

use std::path::PathBuf;

use io_fs::{
    coroutines::file_read::{FsFileRead, FsFileReadError, FsFileReadResult},
    io::{FsInput, FsOutput},
};
use thiserror::Error;

use crate::{
    coroutines::message_locate::{
        MaildirMessageLocate, MaildirMessageLocateError, MaildirMessageLocateResult,
    },
    maildir::Maildir,
    message::Message,
};

/// Errors that can occur during the coroutine progression.
#[derive(Clone, Debug, Error)]
pub enum MaildirMessageGetError {
    /// The message could not be located in the Maildir.
    #[error(transparent)]
    Locate(#[from] MaildirMessageLocateError),

    /// An error occurred while reading the message file.
    #[error("Read Maildir message file error")]
    Read(#[from] FsFileReadError),
}

/// Output emitted after the coroutine finishes its progression.
#[derive(Clone, Debug)]
pub enum MaildirMessageGetResult {
    /// The coroutine has successfully terminated its progression.
    Ok(Message),

    /// A filesystem I/O needs to be performed to make the coroutine
    /// progress.
    Io(FsInput),

    /// An error occurred during the coroutine progression.
    Err(MaildirMessageGetError),
}

#[derive(Debug)]
enum State {
    Locate(MaildirMessageLocate),
    Read {
        path: PathBuf,
        coroutine: FsFileRead,
    },
}

/// I/O-free coroutine to get a single Maildir message by its ID.
#[derive(Debug)]
pub struct MaildirMessageGet {
    state: State,
}

impl MaildirMessageGet {
    /// Creates a new coroutine that will retrieve message `id` from
    /// `maildir`.
    pub fn new(maildir: Maildir, id: impl ToString) -> Self {
        Self {
            state: State::Locate(MaildirMessageLocate::new(maildir, id)),
        }
    }

    /// Makes the message get progress.
    pub fn resume(&mut self, mut arg: Option<FsOutput>) -> MaildirMessageGetResult {
        loop {
            match &mut self.state {
                State::Locate(coroutine) => {
                    let path = match coroutine.resume(arg.take()) {
                        MaildirMessageLocateResult::Ok { path, .. } => path,
                        MaildirMessageLocateResult::Io(input) => {
                            return MaildirMessageGetResult::Io(input);
                        }
                        MaildirMessageLocateResult::Err(err) => {
                            return MaildirMessageGetResult::Err(err.into());
                        }
                    };

                    self.state = State::Read {
                        coroutine: FsFileRead::new([path.to_string_lossy()]),
                        path,
                    };
                }
                State::Read { path, coroutine } => {
                    let contents = match coroutine.resume(arg.take()) {
                        FsFileReadResult::Ok(map) => map.into_values().next().unwrap_or_default(),
                        FsFileReadResult::Io(input) => return MaildirMessageGetResult::Io(input),
                        FsFileReadResult::Err(err) => {
                            return MaildirMessageGetResult::Err(err.into());
                        }
                    };

                    return MaildirMessageGetResult::Ok(Message {
                        path: path.clone(),
                        contents,
                    });
                }
            }
        }
    }
}
