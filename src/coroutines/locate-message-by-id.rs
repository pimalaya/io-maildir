//! I/O-free coroutine to create a Vdir item.

use std::path::PathBuf;

use io_fs::{
    coroutines::read_dir::ReadDir,
    error::{FsError, FsResult},
    io::FsIo,
};
use thiserror::Error;

use crate::maildir::Maildir;

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
    /// The coroutine successfully terminated its progression.
    Ok(PathBuf),

    /// The coroutine encountered an error.
    Err(LocateMaildirMessageByIdError),

    /// An I/O needs to be processed in order to make the coroutine
    /// progress further.
    Io(FsIo),
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
                    let new = self.maildir.new().join(&self.id);
                    if new.is_file() {
                        return LocateMaildirMessageByIdResult::Ok(new);
                    }

                    let tmp = self.maildir.tmp().join(&self.id);
                    if tmp.is_file() {
                        return LocateMaildirMessageByIdResult::Ok(tmp);
                    }

                    self.state = State::InspectCur(ReadDir::new(self.maildir.cur()));
                }
                State::InspectCur(coroutine) => {
                    let paths = match coroutine.resume(arg.take()) {
                        FsResult::Ok(paths) => paths,
                        FsResult::Io(io) => return LocateMaildirMessageByIdResult::Io(io),
                        FsResult::Err(err) => {
                            let err = LocateMaildirMessageByIdError::Inspect(err);
                            return LocateMaildirMessageByIdResult::Err(err);
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
                            return LocateMaildirMessageByIdResult::Ok(path);
                        }
                    }

                    let err = LocateMaildirMessageByIdError::NotFound(self.id.clone());
                    return LocateMaildirMessageByIdResult::Err(err);
                }
            }
        }
    }
}
