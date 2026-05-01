//! I/O-free coroutine to store a message in a Maildir.

use std::{
    collections::BTreeMap,
    mem,
    path::PathBuf,
    process,
    sync::atomic::{AtomicUsize, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use gethostname::gethostname;
use log::trace;
use thiserror::Error;

use crate::{
    flag::Flags,
    maildir::{Maildir, MaildirSubdir},
    message::INFORMATIONAL_SUFFIX_SEPARATOR,
};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Errors that can occur during the coroutine progression.
#[derive(Clone, Debug, Error)]
pub enum MaildirMessageStoreError {
    #[error("Invalid Maildir message store arg {0:?} for state {1:?}")]
    Invalid(Option<MaildirMessageStoreArg>, State),
}

/// Result returned by [`MaildirMessageStore::resume`].
#[derive(Clone, Debug)]
pub enum MaildirMessageStoreResult {
    /// The coroutine has successfully terminated its progression.
    Ok { id: String, path: PathBuf },

    /// The coroutine wants the caller to create the given files and
    /// feed back [`MaildirMessageStoreArg::FileCreate`].
    WantsFileCreate(BTreeMap<String, Vec<u8>>),

    /// The coroutine wants the caller to rename each `(from, to)`
    /// pair and feed back [`MaildirMessageStoreArg::Rename`].
    WantsRename(Vec<(String, String)>),

    /// The coroutine encountered an error.
    Err(MaildirMessageStoreError),
}

/// Internal progression state of [`MaildirMessageStore`].
#[derive(Clone, Debug, Default)]
pub enum State {
    Start(Vec<u8>),
    Created,
    Renamed,
    #[default]
    Invalid,
}

/// Argument fed back to [`MaildirMessageStore::resume`] after the
/// caller performed the requested filesystem operation.
#[derive(Clone, Debug)]
pub enum MaildirMessageStoreArg {
    /// Response to [`MaildirMessageStoreResult::WantsFileCreate`].
    FileCreate,

    /// Response to [`MaildirMessageStoreResult::WantsRename`].
    Rename,
}

/// I/O-free coroutine to store a message in a Maildir.
///
/// Follows the Maildir delivery protocol: write to `/tmp` first, then
/// atomically rename to the target subdir (`/new` or `/cur`).
#[derive(Debug)]
pub struct MaildirMessageStore {
    id: String,
    tmp_path: String,
    final_path: String,
    state: State,
}

impl MaildirMessageStore {
    /// Creates a new coroutine that will store `contents` as a new
    /// message in `maildir` under `subdir` with the given `flags`.
    pub fn new(maildir: Maildir, subdir: MaildirSubdir, flags: Flags, contents: Vec<u8>) -> Self {
        let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        let secs = ts.as_secs();
        let nanos = ts.subsec_nanos();
        let counter = COUNTER.fetch_add(1, Ordering::SeqCst);
        let pid = process::id();
        let hostname = gethostname().into_string().unwrap_or_default();
        let id = format!("{secs}.#{counter:x}M{nanos}P{pid}.{hostname}");

        let mut id_with_info = id.clone();
        if let MaildirSubdir::Cur = subdir {
            id_with_info.push(INFORMATIONAL_SUFFIX_SEPARATOR);
            id_with_info.push_str("2,");
            id_with_info.push_str(&flags.to_string());
        }

        let tmp_path = format!("{}/{id}", maildir.tmp().to_string_lossy());
        let final_path = format!(
            "{}/{id_with_info}",
            maildir.subdir(&subdir).to_string_lossy()
        );

        Self {
            id,
            tmp_path,
            final_path,
            state: State::Start(contents),
        }
    }

    /// Makes the message store progress.
    pub fn resume(
        &mut self,
        arg: Option<impl Into<MaildirMessageStoreArg>>,
    ) -> MaildirMessageStoreResult {
        match (mem::take(&mut self.state), arg.map(Into::into)) {
            (State::Start(contents), None) => {
                trace!("wants tmp file create at {}", self.tmp_path);

                let files = BTreeMap::from_iter([(self.tmp_path.clone(), contents)]);
                self.state = State::Created;
                MaildirMessageStoreResult::WantsFileCreate(files)
            }
            (State::Created, Some(MaildirMessageStoreArg::FileCreate)) => {
                trace!("created tmp file, wants rename to {}", self.final_path);

                let pairs = vec![(self.tmp_path.clone(), self.final_path.clone())];
                self.state = State::Renamed;
                MaildirMessageStoreResult::WantsRename(pairs)
            }
            (State::Renamed, Some(MaildirMessageStoreArg::Rename)) => {
                trace!("renamed tmp file to {}", self.final_path);

                MaildirMessageStoreResult::Ok {
                    id: self.id.clone(),
                    path: PathBuf::from(&self.final_path),
                }
            }
            (state, arg) => {
                let err = MaildirMessageStoreError::Invalid(arg, state);
                MaildirMessageStoreResult::Err(err)
            }
        }
    }
}
