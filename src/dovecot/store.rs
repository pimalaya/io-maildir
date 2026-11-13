//! I/O-free coroutine writing the `dovecot-keywords` slot table at
//! the root of a Maildir.
//!
//! # Example
//!
//! ```rust,no_run
//! use std::collections::BTreeMap;
//!
//! use io_maildir::{client::MaildirClient, dovecot::store::MaildirDovecotStore};
//!
//! let client = MaildirClient::new("/path/to/root");
//! let maildir = client.load_maildir("inbox").unwrap();
//!
//! let mut table = BTreeMap::new();
//! table.insert('a', "Important".to_string());
//! table.insert('b', "Personal".to_string());
//!
//! let coroutine = MaildirDovecotStore::new(&maildir, &table);
//! client.run(coroutine).unwrap();
//! ```

use core::{fmt, mem, str};

use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::String,
    vec::Vec,
};

use log::debug;
use thiserror::Error;

use crate::{
    coroutine::*, dovecot::utils::rewrite_dovecot_keywords, entry::mint_id, maildir::Maildir,
    path::MaildirFsPath,
};

const FILENAME: &str = "dovecot-keywords";

/// Failure causes during a [`MaildirDovecotStore`] step.
#[derive(Clone, Debug, Error)]
pub enum MaildirDovecotStoreError {
    /// A reply arrived that does not match the awaited step.
    #[error("Maildir dovecot store failed: unexpected arg {0:?}")]
    UnexpectedArg(Option<MaildirReply>),
    /// The file holds a line that is no keyword slot, or is not UTF-8.
    #[error("Maildir dovecot store failed: {0} does not parse and is left as it is")]
    Malformed(MaildirFsPath),
}

/// Persists the `dovecot-keywords` slot table at the root of a Maildir.
///
/// The file is read first, so the slots beyond `a..z` it holds are kept,
/// and one that does not parse is refused rather than replaced. The new
/// content is written under `tmp/` then renamed over the file.
#[derive(Debug)]
pub struct MaildirDovecotStore {
    maildir: Maildir,
    path: MaildirFsPath,
    table: BTreeMap<char, String>,
    state: State,
}

impl MaildirDovecotStore {
    /// Builds a coroutine persisting the `dovecot-keywords` slot table
    /// at the root of a Maildir.
    pub fn new(maildir: &Maildir, table: &BTreeMap<char, String>) -> Self {
        Self {
            maildir: maildir.clone(),
            path: maildir.path().join(FILENAME),
            table: table.clone(),
            state: State::Start,
        }
    }

    /// Builds the new content from the file's own, then asks for the
    /// time, the first input of the tmp name.
    fn rewrite(
        &mut self,
        bytes: &[u8],
    ) -> MaildirCoroutineState<MaildirYield, <Self as MaildirCoroutine>::Return> {
        let text = str::from_utf8(bytes).ok();
        let Some(text) = text.and_then(|text| rewrite_dovecot_keywords(text, &self.table)) else {
            let err = MaildirDovecotStoreError::Malformed(self.path.clone());
            return MaildirCoroutineState::Complete(Err(err));
        };

        self.state = State::ReadTime {
            payload: text.into_bytes(),
        };
        MaildirCoroutineState::Yielded(MaildirYield::WantsTime)
    }
}

impl MaildirCoroutine for MaildirDovecotStore {
    type Yield = MaildirYield;
    type Return = Result<(), MaildirDovecotStoreError>;

    fn resume(
        &mut self,
        arg: Option<MaildirReply>,
    ) -> MaildirCoroutineState<Self::Yield, Self::Return> {
        match (&mut self.state, arg) {
            (State::Start, None) => {
                let paths = BTreeSet::from([self.path.clone()]);
                self.state = State::Probe;
                MaildirCoroutineState::Yielded(MaildirYield::WantsFileExists(paths))
            }
            (State::Probe, Some(MaildirReply::FileExists(map))) => {
                if !map.get(&self.path).copied().unwrap_or(false) {
                    return self.rewrite(&[]);
                }

                let paths = BTreeSet::from([self.path.clone()]);
                self.state = State::Read;
                MaildirCoroutineState::Yielded(MaildirYield::WantsFileRead(paths))
            }
            (State::Read, Some(MaildirReply::FileRead(mut map))) => {
                let bytes = map.remove(&self.path).unwrap_or_default();
                self.rewrite(&bytes)
            }
            (State::ReadTime { payload }, Some(MaildirReply::Time { secs, nanos })) => {
                self.state = State::ReadPid {
                    payload: mem::take(payload),
                    secs,
                    nanos,
                };
                MaildirCoroutineState::Yielded(MaildirYield::WantsPid)
            }
            (
                State::ReadPid {
                    payload,
                    secs,
                    nanos,
                },
                Some(MaildirReply::Pid(pid)),
            ) => {
                self.state = State::ReadCounter {
                    payload: mem::take(payload),
                    secs: *secs,
                    nanos: *nanos,
                    pid,
                };
                MaildirCoroutineState::Yielded(MaildirYield::WantsCounter)
            }
            (
                State::ReadCounter {
                    payload,
                    secs,
                    nanos,
                    pid,
                },
                Some(MaildirReply::Counter(counter)),
            ) => {
                self.state = State::ReadHostname {
                    payload: mem::take(payload),
                    secs: *secs,
                    nanos: *nanos,
                    pid: *pid,
                    counter,
                };
                MaildirCoroutineState::Yielded(MaildirYield::WantsHostname)
            }
            (
                State::ReadHostname {
                    payload,
                    secs,
                    nanos,
                    pid,
                    counter,
                },
                Some(MaildirReply::Hostname(hostname)),
            ) => {
                let separator = self.maildir.separator;
                let id = mint_id(*secs, *nanos, *pid, *counter, &hostname, separator);
                let tmp_path = self.maildir.tmp().join(&id);
                let files = BTreeMap::from([(tmp_path.clone(), mem::take(payload))]);
                self.state = State::CreateTmp { tmp_path };
                MaildirCoroutineState::Yielded(MaildirYield::WantsFileCreate(files))
            }
            (State::CreateTmp { tmp_path }, Some(MaildirReply::FileCreate)) => {
                let tmp_path = mem::take(tmp_path);
                let paths = BTreeSet::from([tmp_path.clone()]);
                self.state = State::SyncTmp { tmp_path };
                MaildirCoroutineState::Yielded(MaildirYield::WantsFileSync(paths))
            }
            (State::SyncTmp { tmp_path }, Some(MaildirReply::FileSync)) => {
                let pairs = vec![(mem::take(tmp_path), self.path.clone())];
                self.state = State::Rename;
                MaildirCoroutineState::Yielded(MaildirYield::WantsRename(pairs))
            }
            (State::Rename, Some(MaildirReply::Rename)) => {
                let paths = BTreeSet::from([self.maildir.path().clone()]);
                self.state = State::SyncDir;
                MaildirCoroutineState::Yielded(MaildirYield::WantsDirSync(paths))
            }
            (State::SyncDir, Some(MaildirReply::DirSync)) => {
                debug!("stored dovecot keywords");
                MaildirCoroutineState::Complete(Ok(()))
            }
            (_, arg) => {
                let err = MaildirDovecotStoreError::UnexpectedArg(arg);
                MaildirCoroutineState::Complete(Err(err))
            }
        }
    }
}

#[derive(Debug)]
enum State {
    Start,
    Probe,
    Read,
    ReadTime {
        payload: Vec<u8>,
    },
    ReadPid {
        payload: Vec<u8>,
        secs: u64,
        nanos: u32,
    },
    ReadCounter {
        payload: Vec<u8>,
        secs: u64,
        nanos: u32,
        pid: u32,
    },
    ReadHostname {
        payload: Vec<u8>,
        secs: u64,
        nanos: u32,
        pid: u32,
        counter: u32,
    },
    CreateTmp {
        tmp_path: MaildirFsPath,
    },
    SyncTmp {
        tmp_path: MaildirFsPath,
    },
    Rename,
    SyncDir,
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Start => f.write_str("start"),
            Self::Probe => f.write_str("probe keywords table"),
            Self::Read => f.write_str("read keywords table"),
            Self::ReadTime { .. } => f.write_str("read time"),
            Self::ReadPid { .. } => f.write_str("read pid"),
            Self::ReadCounter { .. } => f.write_str("read counter"),
            Self::ReadHostname { .. } => f.write_str("read hostname"),
            Self::CreateTmp { .. } => f.write_str("create tmp keywords table"),
            Self::SyncTmp { .. } => f.write_str("sync tmp keywords table"),
            Self::Rename => f.write_str("rename into place"),
            Self::SyncDir => f.write_str("sync maildir"),
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::collections::BTreeSet;

    use crate::dovecot::store::*;

    fn maildir() -> Maildir {
        Maildir::from_path("root")
    }

    fn keywords_path() -> MaildirFsPath {
        MaildirFsPath::from("root/dovecot-keywords")
    }

    fn table(slots: &[(char, &str)]) -> BTreeMap<char, String> {
        let slots = slots.iter();
        slots
            .map(|(letter, name)| (*letter, String::from(*name)))
            .collect()
    }

    /// Stores `table` over a keyword file holding `file`, returning the
    /// filesystem steps asked for and the bytes written.
    fn store(
        table: &BTreeMap<char, String>,
        file: Option<&[u8]>,
    ) -> Result<(Vec<&'static str>, Vec<u8>), MaildirDovecotStoreError> {
        let mut cor = MaildirDovecotStore::new(&maildir(), table);
        let mut steps = Vec::new();
        let mut written = Vec::new();
        let mut tmp = MaildirFsPath::default();
        let mut arg = None;

        loop {
            let wants = match cor.resume(arg.take()) {
                MaildirCoroutineState::Complete(Ok(())) => return Ok((steps, written)),
                MaildirCoroutineState::Complete(Err(err)) => return Err(err),
                MaildirCoroutineState::Yielded(wants) => wants,
            };

            arg = Some(match wants {
                MaildirYield::WantsFileExists(paths) => {
                    assert_eq!(paths, BTreeSet::from([keywords_path()]));
                    steps.push("exists");
                    let exists = BTreeMap::from([(keywords_path(), file.is_some())]);
                    MaildirReply::FileExists(exists)
                }
                MaildirYield::WantsFileRead(paths) => {
                    assert_eq!(paths, BTreeSet::from([keywords_path()]));
                    steps.push("read");
                    let bytes = file.unwrap().to_vec();
                    MaildirReply::FileRead(BTreeMap::from([(keywords_path(), bytes)]))
                }
                MaildirYield::WantsTime => MaildirReply::Time { secs: 1, nanos: 2 },
                MaildirYield::WantsPid => MaildirReply::Pid(3),
                MaildirYield::WantsCounter => MaildirReply::Counter(4),
                MaildirYield::WantsHostname => MaildirReply::Hostname(String::from("host")),
                MaildirYield::WantsFileCreate(files) => {
                    steps.push("create");
                    let (path, bytes) = files.into_iter().next().unwrap();
                    assert_eq!(path, MaildirFsPath::from("root/tmp/1.#4M2P3.host"));
                    tmp = path;
                    written = bytes;
                    MaildirReply::FileCreate
                }
                MaildirYield::WantsFileSync(paths) => {
                    steps.push("sync file");
                    assert_eq!(paths, BTreeSet::from([tmp.clone()]));
                    MaildirReply::FileSync
                }
                MaildirYield::WantsRename(pairs) => {
                    steps.push("rename");
                    assert_eq!(pairs, [(tmp.clone(), keywords_path())]);
                    MaildirReply::Rename
                }
                MaildirYield::WantsDirSync(paths) => {
                    steps.push("sync dir");
                    assert_eq!(paths, BTreeSet::from([MaildirFsPath::from("root")]));
                    MaildirReply::DirSync
                }
                wants => panic!("unexpected {wants:?}"),
            });
        }
    }

    #[test]
    fn the_file_is_replaced_through_tmp_and_synced() {
        let table = table(&[('a', "$Junk"), ('b', "Work")]);
        let (steps, written) = store(&table, Some(b"0 $Junk\n")).unwrap();

        let expected = [
            "exists",
            "read",
            "create",
            "sync file",
            "rename",
            "sync dir",
        ];
        assert_eq!(steps, expected);
        assert_eq!(written, b"0 $Junk\n1 Work\n");
    }

    #[test]
    fn a_missing_file_is_written_from_the_table() {
        let (steps, written) = store(&table(&[('a', "Project")]), None).unwrap();

        assert_eq!(
            steps,
            ["exists", "create", "sync file", "rename", "sync dir"]
        );
        assert_eq!(written, b"0 Project\n");
    }

    #[test]
    fn the_lines_beyond_the_letters_are_kept_in_place() {
        let table = table(&[('a', "$Junk"), ('c', "Work")]);
        let file = b"30 Far\n0 $Junk\n\n1 Dropped\n26 Beyond\n";
        let (_, written) = store(&table, Some(file)).unwrap();

        assert_eq!(written, b"30 Far\n0 $Junk\n26 Beyond\n2 Work\n");
    }

    #[test]
    fn a_file_that_does_not_parse_is_left_alone() {
        let table = table(&[('a', "$Junk"), ('b', "Work")]);
        let files: [&[u8]; 4] = [b"0 $Junk\nnot a slot\n", b"0 \xff\n", b"0\n", b"-1 x\n"];

        for file in files {
            let err = store(&table, Some(file)).unwrap_err();
            assert!(matches!(err, MaildirDovecotStoreError::Malformed(_)));
        }
    }

    #[test]
    fn unexpected_reply_returns_error() {
        let mut cor = MaildirDovecotStore::new(&maildir(), &BTreeMap::new());
        let _ = cor.resume(None);

        match cor.resume(Some(MaildirReply::DirCreate)) {
            MaildirCoroutineState::Complete(Err(MaildirDovecotStoreError::UnexpectedArg(_))) => {}
            state => panic!("expected Complete(Err), got {state:?}"),
        }
    }
}
