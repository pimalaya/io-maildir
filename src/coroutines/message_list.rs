//! I/O-free coroutine to list messages in a Maildir.

use std::{collections::HashSet, path::PathBuf};

use io_fs::{
    coroutines::{
        dir_read::{FsDirRead, FsDirReadError, FsDirReadResult},
        file_read::{FsFileRead, FsFileReadError, FsFileReadResult},
    },
    io::{FsInput, FsOutput},
};
use thiserror::Error;

use crate::{maildir::Maildir, message::Message};

/// Errors that can occur during the coroutine progression.
#[derive(Clone, Debug, Error)]
pub enum MaildirMessagesListError {
    /// An error occurred while reading a Maildir subdir.
    #[error("List Maildir messages: read directory error")]
    DirRead(#[source] FsDirReadError),

    /// An error occurred while reading message files.
    #[error("List Maildir messages: read files error")]
    FileRead(#[source] FsFileReadError),
}

/// Output emitted after the coroutine finishes its progression.
#[derive(Clone, Debug)]
pub enum MaildirMessagesListResult {
    /// The coroutine has successfully terminated its progression.
    Ok(HashSet<Message>),

    /// A filesystem I/O needs to be performed to make the coroutine
    /// progress.
    Io(FsInput),

    /// An error occurred during the coroutine progression.
    Err(MaildirMessagesListError),
}

fn is_visible_file(path: &str) -> bool {
    let p = PathBuf::from(path);
    if !p.is_file() {
        return false;
    }
    matches!(
        p.file_name().and_then(|n| n.to_str()),
        Some(name) if !name.starts_with('.')
    )
}

#[derive(Debug)]
enum State {
    ReadNew(FsDirRead),
    ReadCur(HashSet<String>, FsDirRead),
    ReadFiles(FsFileRead),
}

/// I/O-free coroutine to list all messages in a Maildir.
///
/// Scans both the `/new` and `/cur` subdirectories and reads every
/// message file.
#[derive(Debug)]
pub struct MaildirMessagesList {
    state: State,
    maildir: Maildir,
}

impl MaildirMessagesList {
    /// Creates a new coroutine that will list all messages in
    /// `maildir`.
    pub fn new(maildir: Maildir) -> Self {
        let state = State::ReadNew(FsDirRead::new([maildir.new().to_string_lossy()]));
        Self { state, maildir }
    }

    /// Makes the listing progress.
    pub fn resume(&mut self, mut arg: Option<FsOutput>) -> MaildirMessagesListResult {
        loop {
            match &mut self.state {
                State::ReadNew(coroutine) => {
                    let entries = match coroutine.resume(arg.take()) {
                        FsDirReadResult::Ok(entries) => entries,
                        FsDirReadResult::Io(input) => return MaildirMessagesListResult::Io(input),
                        FsDirReadResult::Err(err) => {
                            return MaildirMessagesListResult::Err(
                                MaildirMessagesListError::DirRead(err),
                            );
                        }
                    };

                    let paths: HashSet<String> = entries
                        .into_values()
                        .next()
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|p| is_visible_file(p))
                        .collect();

                    let coroutine = FsDirRead::new([self.maildir.cur().to_string_lossy()]);
                    self.state = State::ReadCur(paths, coroutine);
                }
                State::ReadCur(paths, coroutine) => {
                    let cur_entries = match coroutine.resume(arg.take()) {
                        FsDirReadResult::Ok(entries) => entries,
                        FsDirReadResult::Io(input) => return MaildirMessagesListResult::Io(input),
                        FsDirReadResult::Err(err) => {
                            return MaildirMessagesListResult::Err(
                                MaildirMessagesListError::DirRead(err),
                            );
                        }
                    };

                    let cur_paths = cur_entries.into_values().next().unwrap_or_default();
                    paths.extend(cur_paths.into_iter().filter(|p| is_visible_file(p)));

                    let coroutine = FsFileRead::new(paths.drain());
                    self.state = State::ReadFiles(coroutine);
                }
                State::ReadFiles(coroutine) => {
                    let contents = match coroutine.resume(arg.take()) {
                        FsFileReadResult::Ok(contents) => contents,
                        FsFileReadResult::Io(input) => return MaildirMessagesListResult::Io(input),
                        FsFileReadResult::Err(err) => {
                            return MaildirMessagesListResult::Err(
                                MaildirMessagesListError::FileRead(err),
                            );
                        }
                    };

                    let messages = contents
                        .into_iter()
                        .map(|(path, contents)| Message {
                            path: PathBuf::from(path),
                            contents,
                        })
                        .collect();
                    return MaildirMessagesListResult::Ok(messages);
                }
            }
        }
    }
}
