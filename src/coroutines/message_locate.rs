//! I/O-free coroutine to locate a Maildir message by its ID.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use log::trace;
use thiserror::Error;

use crate::{
    flag::{Flag, Flags},
    maildir::{Maildir, MaildirSubdir},
};

/// Errors that can occur during the coroutine progression.
#[derive(Clone, Debug, Error)]
pub enum MaildirMessageLocateError {
    #[error("Invalid Maildir locate arg: {0:?}")]
    Invalid(Option<MaildirMessageLocateArg>),

    /// No message with the given ID was found in the Maildir.
    #[error("Message {0} not found in Maildir")]
    NotFound(String),
}

/// Result returned by [`MaildirMessageLocate::resume`].
#[derive(Clone, Debug)]
pub enum MaildirMessageLocateResult {
    /// The coroutine has successfully terminated its progression.
    Ok {
        path: PathBuf,
        subdir: MaildirSubdir,
        flags: Flags,
    },

    /// The coroutine wants the caller to read the entries inside the
    /// given directories and feed back [`MaildirMessageLocateArg::DirRead`].
    WantsDirRead(BTreeSet<String>),

    /// The coroutine encountered an error.
    Err(MaildirMessageLocateError),
}

/// Argument fed back to [`MaildirMessageLocate::resume`] after the
/// caller performed the requested filesystem operation.
#[derive(Clone, Debug)]
pub enum MaildirMessageLocateArg {
    /// Response to [`MaildirMessageLocateResult::WantsDirRead`].
    ///
    /// Maps each requested directory path to the set of entry paths
    /// found inside it.
    DirRead(BTreeMap<String, BTreeSet<String>>),
}

/// I/O-free coroutine to locate a Maildir message file by its ID.
///
/// Searches `/new` and `/tmp` first (no I/O needed for those), then
/// scans `/cur` to find a file whose name starts with the given ID.
#[derive(Clone, Debug)]
pub struct MaildirMessageLocate {
    maildir: Maildir,
    id: String,
    wants_dir_read: bool,
}

impl MaildirMessageLocate {
    /// Creates a new coroutine that will search for message `id`
    /// inside `maildir`.
    pub fn new(maildir: Maildir, id: impl ToString) -> Self {
        Self {
            maildir,
            id: id.to_string(),
            wants_dir_read: false,
        }
    }

    /// Makes the locate progress.
    pub fn resume(
        &mut self,
        arg: Option<impl Into<MaildirMessageLocateArg>>,
    ) -> MaildirMessageLocateResult {
        match (self.wants_dir_read, arg.map(Into::into)) {
            (false, None) => {
                trace!("inspect /new and /tmp for {}", self.id);

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

                trace!("wants /cur read for {}", self.id);
                let paths =
                    BTreeSet::from_iter([self.maildir.cur().to_string_lossy().into_owned()]);
                self.wants_dir_read = true;
                MaildirMessageLocateResult::WantsDirRead(paths)
            }
            (true, Some(MaildirMessageLocateArg::DirRead(entries))) => {
                trace!("inspect /cur entries for {}", self.id);

                let paths = entries.into_values().next().unwrap_or_default();

                for path in paths {
                    let path = PathBuf::from(&path);

                    if !path.is_file() {
                        continue;
                    }

                    if let Some(result) = match_id(&path, &self.id) {
                        return result;
                    }
                }

                let err = MaildirMessageLocateError::NotFound(self.id.clone());
                MaildirMessageLocateResult::Err(err)
            }
            (_, arg) => {
                let err = MaildirMessageLocateError::Invalid(arg);
                MaildirMessageLocateResult::Err(err)
            }
        }
    }
}

fn match_id(path: &Path, id: &str) -> Option<MaildirMessageLocateResult> {
    let name = path.file_name().and_then(|n| n.to_str())?;
    if !name.starts_with(id) {
        return None;
    }

    let flags = match name.rsplit_once(',') {
        None => Flags::default(),
        Some((_, flags_str)) => Flags::from_iter(flags_str.chars().filter_map(Flag::from_char)),
    };

    Some(MaildirMessageLocateResult::Ok {
        path: path.to_path_buf(),
        subdir: MaildirSubdir::Cur,
        flags,
    })
}
