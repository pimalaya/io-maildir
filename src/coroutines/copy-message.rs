//! I/O-free coroutine to copy a Vdir message.

use io_fs::{
    coroutines::copy::*,
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
pub enum CopyMaildirMessageError {
    #[error(transparent)]
    Locate(#[from] LocateMaildirMessageByIdError),
    #[error("Copy Maildir message error")]
    Copy(#[from] FsError),
}

/// Output emitted when the coroutine terminates its progression.
#[derive(Clone, Debug)]
pub enum CopyMaildirMessageResult {
    /// The coroutine successfully terminated its progression.
    Ok,

    /// The coroutine encountered an error.
    Err(CopyMaildirMessageError),

    /// An I/O needs to be processed in order to make the coroutine
    /// progress further.
    Io(FsIo),
}

#[derive(Debug)]
enum State {
    Locate(LocateMaildirMessageById),
    Copy(Copy),
}

/// I/O-free coroutine to copy a Vdir message.
#[derive(Debug)]
pub struct CopyMaildirMessage {
    id: String,
    target: Maildir,
    target_subdir: Option<MaildirSubdir>,
    state: State,
}

impl CopyMaildirMessage {
    /// Creates a new coroutine from the given collection's path.
    pub fn new(
        id: impl ToString,
        source: Maildir,
        target: Maildir,
        target_subdir: Option<MaildirSubdir>,
    ) -> Self {
        let coroutine = LocateMaildirMessageById::new(source.clone(), id.to_string());
        let state = State::Locate(coroutine);

        Self {
            id: id.to_string(),
            target,
            target_subdir,
            state,
        }
    }

    /// Makes the coroutine progress.
    pub fn resume(&mut self, mut arg: Option<FsIo>) -> CopyMaildirMessageResult {
        loop {
            match &mut self.state {
                State::Locate(coroutine) => {
                    let (source, subdir) = match coroutine.resume(arg.take()) {
                        LocateMaildirMessageByIdResult::Ok { path, subdir, .. } => (path, subdir),
                        LocateMaildirMessageByIdResult::Err { err } => {
                            return CopyMaildirMessageResult::Err(err.into())
                        }
                        LocateMaildirMessageByIdResult::Io { io } => {
                            return CopyMaildirMessageResult::Io(io)
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

                    self.state = State::Copy(Copy::new(source, target))
                }
                State::Copy(coroutine) => {
                    return match coroutine.resume(arg.take()) {
                        FsResult::Ok(()) => CopyMaildirMessageResult::Ok,
                        FsResult::Err(err) => CopyMaildirMessageResult::Err(err.into()),
                        FsResult::Io(io) => CopyMaildirMessageResult::Io(io),
                    }
                }
            }
        }
    }
}
