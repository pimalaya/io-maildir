//! I/O-free coroutine to list messages in a Maildir.

use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    mem,
    path::PathBuf,
};

use log::trace;
use thiserror::Error;

use crate::{maildir::Maildir, message::Message};

/// Errors that can occur during the coroutine progression.
#[derive(Clone, Debug, Error)]
pub enum MaildirMessagesListError {
    #[error("Invalid Maildir messages list arg {0:?} for state {1:?}")]
    Invalid(Option<MaildirMessagesListArg>, State),
}

/// Result returned by [`MaildirMessagesList::resume`].
#[derive(Clone, Debug)]
pub enum MaildirMessagesListResult {
    /// The coroutine has successfully terminated its progression.
    Ok(HashSet<Message>),

    /// The coroutine wants the caller to read the entries inside the
    /// given directories and feed back [`MaildirMessagesListArg::DirRead`].
    WantsDirRead(BTreeSet<String>),

    /// The coroutine wants the caller to read the contents of the
    /// given files and feed back [`MaildirMessagesListArg::FileRead`].
    WantsFileRead(BTreeSet<String>),

    /// The coroutine encountered an error.
    Err(MaildirMessagesListError),
}

/// Internal progression state of [`MaildirMessagesList`].
#[derive(Clone, Debug, Default)]
pub enum State {
    Start(Maildir),
    ReadNew(Maildir),
    ReadCur(HashSet<String>),
    ReadFiles,
    #[default]
    Invalid,
}

/// Argument fed back to [`MaildirMessagesList::resume`] after the
/// caller performed the requested filesystem operation.
#[derive(Clone, Debug)]
pub enum MaildirMessagesListArg {
    /// Response to [`MaildirMessagesListResult::WantsDirRead`].
    DirRead(BTreeMap<String, BTreeSet<String>>),

    /// Response to [`MaildirMessagesListResult::WantsFileRead`].
    FileRead(BTreeMap<String, Vec<u8>>),
}

/// I/O-free coroutine to list all messages in a Maildir.
///
/// Scans both the `/new` and `/cur` subdirectories and reads every
/// message file.
#[derive(Debug)]
pub struct MaildirMessagesList {
    state: State,
}

impl MaildirMessagesList {
    /// Creates a new coroutine that will list all messages in
    /// `maildir`.
    pub fn new(maildir: Maildir) -> Self {
        Self {
            state: State::Start(maildir),
        }
    }

    /// Makes the listing progress.
    pub fn resume(
        &mut self,
        arg: Option<impl Into<MaildirMessagesListArg>>,
    ) -> MaildirMessagesListResult {
        match (mem::take(&mut self.state), arg.map(Into::into)) {
            (State::Start(maildir), None) => {
                trace!("wants /new read");

                let paths = BTreeSet::from_iter([maildir.new().to_string_lossy().into_owned()]);
                self.state = State::ReadNew(maildir);
                MaildirMessagesListResult::WantsDirRead(paths)
            }
            (State::ReadNew(maildir), Some(MaildirMessagesListArg::DirRead(entries))) => {
                trace!("read /new entries, wants /cur read");

                let new_paths: HashSet<String> = entries
                    .into_values()
                    .next()
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|p| is_visible_file(p))
                    .collect();

                let paths = BTreeSet::from_iter([maildir.cur().to_string_lossy().into_owned()]);
                self.state = State::ReadCur(new_paths);
                MaildirMessagesListResult::WantsDirRead(paths)
            }
            (State::ReadCur(mut new_paths), Some(MaildirMessagesListArg::DirRead(entries))) => {
                trace!("read /cur entries, wants file read");

                let cur_paths = entries.into_values().next().unwrap_or_default();
                new_paths.extend(cur_paths.into_iter().filter(|p| is_visible_file(p)));

                let paths = BTreeSet::from_iter(new_paths);
                self.state = State::ReadFiles;
                MaildirMessagesListResult::WantsFileRead(paths)
            }
            (State::ReadFiles, Some(MaildirMessagesListArg::FileRead(contents))) => {
                trace!("read message files");

                let messages = contents
                    .into_iter()
                    .map(|(path, contents)| Message::from((PathBuf::from(path), contents)))
                    .collect();

                MaildirMessagesListResult::Ok(messages)
            }
            (state, arg) => {
                let err = MaildirMessagesListError::Invalid(arg, state);
                MaildirMessagesListResult::Err(err)
            }
        }
    }
}

fn is_visible_file(path: &str) -> bool {
    let path = PathBuf::from(path);

    if !path.is_file() {
        return false;
    }

    matches!(
        path.file_name().and_then(|n| n.to_str()),
        Some(name) if !name.starts_with('.')
    )
}
