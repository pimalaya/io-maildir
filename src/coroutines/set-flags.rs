//! I/O-free coroutine to set flags in a Vdir collection.

use io_fs::{
    coroutines::rename::*,
    error::{FsError, FsResult},
    io::FsIo,
};
use thiserror::Error;

use crate::{
    coroutines::locate_message_by_id::*,
    flag::Flags,
    maildir::{Maildir, MaildirSubdir},
    message::INFORMATIONAL_SUFFIX_SEPARATOR,
};

/// Errors that can occur during the coroutine progression.
#[derive(Clone, Debug, Error)]
pub enum SetMaildirFlagsError {
    #[error(transparent)]
    Locate(#[from] LocateMaildirMessageByIdError),
    #[error("Set flags to message file name error")]
    Rename(#[from] FsError),
}

/// Output emitted when the coroutine terminates its progression.
#[derive(Clone, Debug)]
pub enum SetMaildirFlagsResult {
    /// The coroutine successfully terminated its progression.
    Ok,

    /// The coroutine encountered an error.
    Err(SetMaildirFlagsError),

    /// An I/O needs to be processed in order to make the coroutine
    /// progress further.
    Io(FsIo),
}

#[derive(Debug)]
enum State {
    Locate(LocateMaildirMessageById),
    Rename(Rename),
}

/// I/O-free coroutine to set flags in a Vdir collection.
#[derive(Debug)]
pub struct SetMaildirFlags {
    state: State,
    id: String,
    flags: Flags,
}

impl SetMaildirFlags {
    /// Creates a new coroutine from the given setressbook path.
    pub fn new(maildir: Maildir, id: impl ToString, flags: Flags) -> Self {
        let coroutine = LocateMaildirMessageById::new(maildir.clone(), id.to_string());
        let state = State::Locate(coroutine);

        Self {
            state,
            id: id.to_string(),
            flags,
        }
    }

    /// Makes the coroutine progress.
    pub fn resume(&mut self, mut arg: Option<FsIo>) -> SetMaildirFlagsResult {
        loop {
            match &mut self.state {
                State::Locate(coroutine) => {
                    let (path, subdir) = match coroutine.resume(arg.take()) {
                        LocateMaildirMessageByIdResult::Ok { path, subdir, .. } => (path, subdir),
                        LocateMaildirMessageByIdResult::Err { err } => {
                            break SetMaildirFlagsResult::Err(err.into())
                        }
                        LocateMaildirMessageByIdResult::Io { io } => {
                            break SetMaildirFlagsResult::Io(io)
                        }
                    };

                    match subdir {
                        MaildirSubdir::New | MaildirSubdir::Tmp => {
                            return SetMaildirFlagsResult::Ok;
                        }
                        MaildirSubdir::Cur => {
                            let mut file_name = self.id.clone();
                            file_name.push(INFORMATIONAL_SUFFIX_SEPARATOR);
                            file_name.push_str("2,");
                            file_name.push_str(&self.flags.to_string());

                            let new_path = path.with_file_name(file_name);
                            self.state = State::Rename(Rename::new(Some((path, new_path))));
                        }
                    }
                }
                State::Rename(coroutine) => {
                    return match coroutine.resume(arg.take()) {
                        FsResult::Ok(()) => SetMaildirFlagsResult::Ok,
                        FsResult::Err(err) => SetMaildirFlagsResult::Err(err.into()),
                        FsResult::Io(io) => SetMaildirFlagsResult::Io(io),
                    }
                }
            }
        }
    }
}
