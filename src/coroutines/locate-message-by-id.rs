//! I/O-free coroutine to create a Vdir item.

use std::path::PathBuf;

use io_fs::{
    coroutines::read_dir::ReadDir,
    error::{FsError, FsResult},
    io::FsIo,
};
use thiserror::Error;

use crate::{
    flag::{Flag, Flags},
    maildir::{Maildir, MaildirSubdir},
};

/// Errors that can occur during the coroutine progression.
#[derive(Clone, Debug, Error)]
pub enum LocateMaildirMessageByIdError {
    #[error("Read messages in Maildir /cur subdir error")]
    Inspect(#[source] FsError),
    #[error("Message {0} not found in Maildir /cur subdir")]
    NotFound(String),
}

/// Output emitted when the coroutine terminates its progression.
#[derive(Clone, Debug)]
pub enum LocateMaildirMessageByIdResult {
    /// An I/O needs to be processed in order to make the coroutine
    /// progress further.
    Io { io: FsIo },

    /// The coroutine successfully terminated its progression.
    Ok {
        path: PathBuf,
        subdir: MaildirSubdir,
        flags: Flags,
    },

    /// The coroutine encountered an error.
    Err { err: LocateMaildirMessageByIdError },
}

#[derive(Debug)]
enum State {
    InspectNewAndTmp,
    InspectCur(ReadDir),
}

/// I/O-free coroutine to create a Vdir item.
#[derive(Debug)]
pub struct LocateMaildirMessageById {
    maildir: Maildir,
    id: String,
    state: State,
}

impl LocateMaildirMessageById {
    /// Creates a new coroutine from the given item.
    pub fn new(maildir: Maildir, id: impl ToString) -> Self {
        Self {
            maildir,
            id: id.to_string(),
            state: State::InspectNewAndTmp,
        }
    }

    /// Makes the coroutine progress.
    pub fn resume(&mut self, mut arg: Option<FsIo>) -> LocateMaildirMessageByIdResult {
        loop {
            match &mut self.state {
                State::InspectNewAndTmp => {
                    let path = self.maildir.new().join(&self.id);
                    if path.is_file() {
                        return LocateMaildirMessageByIdResult::Ok {
                            path,
                            subdir: MaildirSubdir::New,
                            flags: Flags::default(),
                        };
                    }

                    let path = self.maildir.tmp().join(&self.id);
                    if path.is_file() {
                        return LocateMaildirMessageByIdResult::Ok {
                            path,
                            subdir: MaildirSubdir::New,
                            flags: Flags::default(),
                        };
                    }

                    self.state = State::InspectCur(ReadDir::new(self.maildir.cur()));
                }
                State::InspectCur(coroutine) => {
                    let paths = match coroutine.resume(arg.take()) {
                        FsResult::Ok(paths) => paths,
                        FsResult::Io(io) => return LocateMaildirMessageByIdResult::Io { io },
                        FsResult::Err(err) => {
                            let err = LocateMaildirMessageByIdError::Inspect(err);
                            return LocateMaildirMessageByIdResult::Err { err };
                        }
                    };

                    for path in paths {
                        if !path.is_file() {
                            continue;
                        }

                        let Some(name) = path.file_name() else {
                            continue;
                        };

                        let Some(name) = name.to_str() else {
                            continue;
                        };

                        if name.starts_with(&self.id) {
                            let flags = match name.rsplit_once(',') {
                                None => Flags::default(),
                                Some((_, flags)) => {
                                    let flags = flags.chars().filter_map(Flag::from_char);
                                    Flags::from_iter(flags)
                                }
                            };

                            return LocateMaildirMessageByIdResult::Ok {
                                path,
                                subdir: MaildirSubdir::Cur,
                                flags,
                            };
                        }
                    }

                    let err = LocateMaildirMessageByIdError::NotFound(self.id.clone());
                    return LocateMaildirMessageByIdResult::Err { err };
                }
            }
        }
    }
}
