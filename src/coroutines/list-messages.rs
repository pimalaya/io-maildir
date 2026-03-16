//! I/O-free coroutine to list messages in a Vdir collection.

use std::{collections::HashSet, path::PathBuf};

use io_fs::{
    coroutines::{read_dir::ReadDir, read_files::ReadFiles},
    error::{FsError, FsResult},
    io::FsIo,
};
use thiserror::Error;

use crate::{maildir::Maildir, message::Message};

/// Errors that can occur during the coroutine progression.
#[derive(Clone, Debug, Error)]
pub enum ListMessagesError {
    /// An error occured during the directory listing.
    #[error("List Vdir messages error")]
    ListDirsError(#[source] FsError),

    /// An error occured during the metadata files listing.
    #[error("Read Vdir messages' metadata error")]
    ListFilesError(#[source] FsError),
}

/// Output emitted when the coroutine terminates its progression.
#[derive(Clone, Debug)]
pub enum ListMessagesResult {
    /// The coroutine successfully terminated its progression.
    Ok(HashSet<Message>),

    /// The coroutine encountered an error.
    Err(ListMessagesError),

    /// An I/O needs to be processed in order to make the coroutine
    /// progress further.
    Io(FsIo),
}

#[derive(Debug)]
enum State {
    ListNewMessages(ReadDir),
    ListCurMessages(HashSet<PathBuf>, ReadDir),
    ReadMessages(ReadFiles),
}

/// I/O-free coroutine to list messages in a Vdir collection.
#[derive(Debug)]
pub struct ListMessages {
    state: State,
    maildir: Maildir,
}

impl ListMessages {
    /// Creates a new coroutine from the given addressbook path.
    pub fn new(maildir: Maildir) -> Self {
        let coroutine = ReadDir::new(maildir.new());
        let state = State::ListNewMessages(coroutine);
        Self { state, maildir }
    }

    /// Makes the coroutine progress.
    pub fn resume(&mut self, mut arg: Option<FsIo>) -> ListMessagesResult {
        loop {
            match &mut self.state {
                State::ListNewMessages(coroutine) => {
                    let mut paths = match coroutine.resume(arg.take()) {
                        FsResult::Ok(paths) => paths,
                        FsResult::Io(io) => break ListMessagesResult::Io(io),
                        FsResult::Err(err) => {
                            let err = ListMessagesError::ListDirsError(err);
                            break ListMessagesResult::Err(err);
                        }
                    };

                    paths.retain(|path| {
                        if !path.is_file() {
                            return false;
                        }

                        let Some(name) = path.file_name() else {
                            return false;
                        };

                        let Some(name) = name.to_str() else {
                            return false;
                        };

                        if name.starts_with('.') {
                            return false;
                        }

                        true
                    });

                    let coroutine = ReadDir::new(self.maildir.cur());
                    self.state = State::ListCurMessages(paths, coroutine);
                }
                State::ListCurMessages(paths, coroutine) => {
                    let mut cur_paths = match coroutine.resume(arg.take()) {
                        FsResult::Ok(paths) => paths,
                        FsResult::Io(io) => break ListMessagesResult::Io(io),
                        FsResult::Err(err) => {
                            let err = ListMessagesError::ListDirsError(err);
                            break ListMessagesResult::Err(err);
                        }
                    };

                    cur_paths.retain(|path| {
                        if !path.is_file() {
                            return false;
                        }

                        let Some(name) = path.file_name() else {
                            return false;
                        };

                        let Some(name) = name.to_str() else {
                            return false;
                        };

                        if name.starts_with('.') {
                            return false;
                        }

                        true
                    });

                    paths.extend(cur_paths);

                    let coroutine = ReadFiles::new(paths.drain());
                    self.state = State::ReadMessages(coroutine);
                }
                State::ReadMessages(coroutine) => {
                    let contents = match coroutine.resume(arg.take()) {
                        FsResult::Ok(contents) => contents,
                        FsResult::Io(io) => break ListMessagesResult::Io(io),
                        FsResult::Err(err) => {
                            let err = ListMessagesError::ListFilesError(err);
                            break ListMessagesResult::Err(err);
                        }
                    };

                    let messages = contents.into_iter().map(Message::from).collect();

                    break ListMessagesResult::Ok(messages);
                }
            }
        }
    }
}
