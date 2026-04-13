//! I/O-free coroutine to locate a Maildir message by its ID.

use std::path::PathBuf;

use io_fs::{
    coroutines::dir_read::{FsDirRead, FsDirReadError, FsDirReadResult},
    io::{FsInput, FsOutput},
};
use thiserror::Error;

use crate::{
    flag::{Flag, Flags},
    maildir::{Maildir, MaildirSubdir},
};

/// Errors that can occur during the coroutine progression.
#[derive(Clone, Debug, Error)]
pub enum MaildirMessageLocateError {
    /// An error occurred while reading the `/cur` subdirectory.
    #[error("Read Maildir /cur subdir error")]
    DirRead(#[source] FsDirReadError),

    /// No message with the given ID was found in the Maildir.
    #[error("Message {0} not found in Maildir")]
    NotFound(String),
}

/// Output emitted after the coroutine finishes its progression.
#[derive(Clone, Debug)]
pub enum MaildirMessageLocateResult {
    /// The coroutine has successfully terminated its progression.
    Ok {
        path: PathBuf,
        subdir: MaildirSubdir,
        flags: Flags,
    },

    /// A filesystem I/O needs to be performed to make the coroutine
    /// progress.
    Io(FsInput),

    /// An error occurred during the coroutine progression.
    Err(MaildirMessageLocateError),
}

#[derive(Debug)]
enum State {
    InspectNewAndTmp,
    InspectCur(FsDirRead),
}

/// I/O-free coroutine to locate a Maildir message file by its ID.
///
/// Searches `/new` and `/tmp` first (no I/O needed for those), then
/// scans `/cur` to find a file whose name starts with the given ID.
#[derive(Debug)]
pub struct MaildirMessageLocate {
    maildir: Maildir,
    id: String,
    state: State,
}

impl MaildirMessageLocate {
    /// Creates a new coroutine that will search for message `id`
    /// inside `maildir`.
    pub fn new(maildir: Maildir, id: impl ToString) -> Self {
        Self {
            maildir,
            id: id.to_string(),
            state: State::InspectNewAndTmp,
        }
    }

    /// Makes the locate progress.
    pub fn resume(&mut self, mut arg: Option<FsOutput>) -> MaildirMessageLocateResult {
        loop {
            match &mut self.state {
                State::InspectNewAndTmp => {
                    let path = self.maildir.new().join(&self.id);
                    if path.is_file() {
                        return MaildirMessageLocateResult::Ok {
                            path,
                            subdir: MaildirSubdir::New,
                            flags: Flags::default(),
                        };
                    }

                    let path = self.maildir.tmp().join(&self.id);
                    if path.is_file() {
                        return MaildirMessageLocateResult::Ok {
                            path,
                            subdir: MaildirSubdir::Tmp,
                            flags: Flags::default(),
                        };
                    }

                    self.state =
                        State::InspectCur(FsDirRead::new([self.maildir.cur().to_string_lossy()]));
                }
                State::InspectCur(coroutine) => {
                    let entries = match coroutine.resume(arg.take()) {
                        FsDirReadResult::Ok(entries) => entries,
                        FsDirReadResult::Io(input) => return MaildirMessageLocateResult::Io(input),
                        FsDirReadResult::Err(err) => {
                            return MaildirMessageLocateResult::Err(
                                MaildirMessageLocateError::DirRead(err),
                            );
                        }
                    };

                    let paths = entries.into_values().next().unwrap_or_default();

                    for path in paths {
                        let path_buf = PathBuf::from(&path);

                        if !path_buf.is_file() {
                            continue;
                        }

                        let Some(name) = path_buf.file_name().and_then(|n| n.to_str()) else {
                            continue;
                        };

                        if name.starts_with(&self.id) {
                            let flags = match name.rsplit_once(',') {
                                None => Flags::default(),
                                Some((_, flags_str)) => {
                                    Flags::from_iter(flags_str.chars().filter_map(Flag::from_char))
                                }
                            };

                            return MaildirMessageLocateResult::Ok {
                                path: path_buf,
                                subdir: MaildirSubdir::Cur,
                                flags,
                            };
                        }
                    }

                    let err = MaildirMessageLocateError::NotFound(self.id.clone());
                    return MaildirMessageLocateResult::Err(err);
                }
            }
        }
    }
}
