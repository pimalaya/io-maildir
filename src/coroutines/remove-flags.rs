//! I/O-free coroutine to remove flags in a Vdir collection.

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
pub enum RemoveMaildirFlagsError {
    #[error(transparent)]
    Locate(#[from] LocateMaildirMessageByIdError),
    #[error("Remove flags to message file name error")]
    Rename(#[from] FsError),
}

/// Output emitted when the coroutine terminates its progression.
#[derive(Clone, Debug)]
pub enum RemoveMaildirFlagsResult {
    /// The coroutine successfully terminated its progression.
    Ok,

    /// The coroutine encountered an error.
    Err(RemoveMaildirFlagsError),

    /// An I/O needs to be processed in order to make the coroutine
    /// progress further.
    Io(FsIo),
}

#[derive(Debug)]
enum State {
    Locate(LocateMaildirMessageById),
    Rename(Rename),
}

/// I/O-free coroutine to remove flags in a Vdir collection.
#[derive(Debug)]
pub struct RemoveMaildirFlags {
    state: State,
    id: String,
    flags: Flags,
}

impl RemoveMaildirFlags {
    /// Creates a new coroutine from the given removeressbook path.
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
    pub fn resume(&mut self, mut arg: Option<FsIo>) -> RemoveMaildirFlagsResult {
        loop {
            match &mut self.state {
                State::Locate(coroutine) => {
                    let (path, subdir, mut flags) = match coroutine.resume(arg.take()) {
                        LocateMaildirMessageByIdResult::Ok {
                            path,
                            subdir,
                            flags,
                        } => (path, subdir, flags),
                        LocateMaildirMessageByIdResult::Err { err } => {
                            break RemoveMaildirFlagsResult::Err(err.into())
                        }
                        LocateMaildirMessageByIdResult::Io { io } => {
                            break RemoveMaildirFlagsResult::Io(io)
                        }
                    };

                    match subdir {
                        MaildirSubdir::New | MaildirSubdir::Tmp => {
                            return RemoveMaildirFlagsResult::Ok;
                        }
                        MaildirSubdir::Cur => {
                            flags.difference(&self.flags);

                            let mut file_name = self.id.clone();
                            file_name.push(INFORMATIONAL_SUFFIX_SEPARATOR);
                            file_name.push_str("2,");
                            file_name.push_str(&flags.to_string());

                            let new_path = path.with_file_name(file_name);
                            self.state = State::Rename(Rename::new(Some((path, new_path))));
                        }
                    }
                }
                State::Rename(coroutine) => {
                    return match coroutine.resume(arg.take()) {
                        FsResult::Ok(()) => RemoveMaildirFlagsResult::Ok,
                        FsResult::Err(err) => RemoveMaildirFlagsResult::Err(err.into()),
                        FsResult::Io(io) => RemoveMaildirFlagsResult::Io(io),
                    }
                }
            }
        }
    }
}
