//! I/O-free coroutine locating many Maildir entries from one read of
//! the subdirectories, where [`MaildirEntryLocate`] reads them per ID.
//!
//! [`MaildirEntryLocate`]: crate::entry::locate::MaildirEntryLocate
//!
//! # Example
//!
//! ```rust,no_run
//! use io_maildir::{
//!     client::MaildirClient, entry::locate_many::MaildirEntriesLocate,
//! };
//!
//! let client = MaildirClient::new("/path/to/root");
//! let maildir = client.load_maildir("inbox").unwrap();
//!
//! let ids = ["1700000000.1.M0P1.host", "1700000001.2.M0P1.host"];
//! let coroutine = MaildirEntriesLocate::new(maildir, ids);
//!
//! for (id, entry) in client.run(coroutine).unwrap() {
//!     println!("found {id} at {}", entry.path);
//! }
//! ```

use core::fmt;

use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::{String, ToString},
};

use log::debug;
use thiserror::Error;

use crate::{
    coroutine::*,
    entry::{
        MAX_DIR_READS,
        locate::{MaildirEntryLocateOutput, is_entry_name, scan},
    },
    maildir::Maildir,
};

/// Failure causes during a [`MaildirEntriesLocate`] step.
#[derive(Clone, Debug, Error)]
pub enum MaildirEntriesLocateError {
    /// A reply arrived that does not match the awaited step.
    #[error("Maildir messages locate failed: unexpected arg {0:?}")]
    UnexpectedArg(Option<MaildirReply>),
}

/// Locates entries by exact logical ID or complete basename, all from
/// one read of `new/`, `tmp/` and `cur/`.
///
/// Each ID resolves to the entry [`MaildirEntryLocate`] would find; one
/// that names no entry is left out of the output. The subdirectories
/// are read again while a matching name is reported gone.
///
/// [`MaildirEntryLocate`]: crate::entry::locate::MaildirEntryLocate
#[derive(Clone, Debug)]
pub struct MaildirEntriesLocate {
    maildir: Maildir,
    ids: BTreeSet<String>,
    state: State,
}

impl MaildirEntriesLocate {
    /// Builds a coroutine locating the Maildir entries with the given
    /// IDs.
    pub fn new(maildir: Maildir, ids: impl IntoIterator<Item = impl ToString>) -> Self {
        let ids = ids.into_iter().map(|id| id.to_string());
        Self {
            maildir,
            ids: ids.filter(|id| is_entry_name(id)).collect(),
            state: State::Start,
        }
    }

    /// Asks for the `reads`th read of the three subdirectories.
    fn read(
        &mut self,
        reads: u8,
    ) -> MaildirCoroutineState<MaildirYield, <Self as MaildirCoroutine>::Return> {
        let maildir = &self.maildir;
        let paths = BTreeSet::from([maildir.new(), maildir.tmp(), maildir.cur()]);
        self.state = State::Read { reads };
        MaildirCoroutineState::Yielded(MaildirYield::WantsDirRead(paths))
    }
}

impl MaildirCoroutine for MaildirEntriesLocate {
    type Yield = MaildirYield;
    type Return = Result<BTreeMap<String, MaildirEntryLocateOutput>, MaildirEntriesLocateError>;

    fn resume(
        &mut self,
        arg: Option<MaildirReply>,
    ) -> MaildirCoroutineState<Self::Yield, Self::Return> {
        match (&mut self.state, arg) {
            (State::Start, None) => self.read(1),
            (State::Read { reads }, Some(MaildirReply::DirRead(entries))) => {
                let reads = *reads;
                let (found, gone) = scan(&self.maildir, &self.ids, entries);

                if gone && reads < MAX_DIR_READS {
                    return self.read(reads + 1);
                }

                debug!("located {} of {} entries", found.len(), self.ids.len());
                MaildirCoroutineState::Complete(Ok(found))
            }
            (_, arg) => {
                let err = MaildirEntriesLocateError::UnexpectedArg(arg);
                MaildirCoroutineState::Complete(Err(err))
            }
        }
    }
}

#[derive(Clone, Debug)]
enum State {
    Start,
    Read { reads: u8 },
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Start => f.write_str("start"),
            Self::Read { .. } => f.write_str("read new, tmp and cur"),
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use crate::{
        entry::{
            locate::{MaildirEntryLocate, MaildirEntryLocateError},
            locate_many::*,
        },
        maildir::MaildirSubdir,
        path::MaildirFsPath,
    };

    const FILE: MaildirFileKind = MaildirFileKind::File;
    const MISSING: MaildirFileKind = MaildirFileKind::Missing;

    fn maildir() -> Maildir {
        Maildir::from_path("root")
    }

    /// Locates `ids` in a Maildir holding the next of `reads` at each
    /// directory read, returning what was found and the requests it
    /// took.
    fn locate(
        ids: &[&str],
        reads: &[&[(&str, MaildirFileKind)]],
    ) -> (BTreeMap<String, MaildirEntryLocateOutput>, usize) {
        let mut cor = MaildirEntriesLocate::new(maildir(), ids);
        let mut requests = 0;
        let mut arg = None;

        loop {
            let dirs = match cor.resume(arg.take()) {
                MaildirCoroutineState::Complete(out) => return (out.unwrap(), requests),
                MaildirCoroutineState::Yielded(MaildirYield::WantsDirRead(dirs)) => dirs,
                state => panic!("expected WantsDirRead, got {state:?}"),
            };

            let tree = reads[requests.min(reads.len() - 1)];
            requests += 1;
            arg = Some(MaildirReply::read(dirs, tree));
        }
    }

    /// Locates `id` alone with [`MaildirEntryLocate`] in a Maildir
    /// holding `tree`.
    fn locate_one(id: &str, tree: &[(&str, MaildirFileKind)]) -> Option<MaildirFsPath> {
        let mut cor = MaildirEntryLocate::new(maildir(), id);
        let mut arg = None;

        loop {
            arg = Some(match cor.resume(arg.take()) {
                MaildirCoroutineState::Complete(Ok(out)) => return Some(out.path),
                MaildirCoroutineState::Complete(Err(MaildirEntryLocateError::NotFound(_))) => {
                    return None;
                }
                MaildirCoroutineState::Yielded(MaildirYield::WantsFileExists(paths)) => {
                    let mut probes = BTreeMap::new();
                    for path in paths {
                        let exists = tree.contains(&(path.as_str(), FILE));
                        probes.insert(path, exists);
                    }
                    MaildirReply::FileExists(probes)
                }
                MaildirCoroutineState::Yielded(MaildirYield::WantsDirRead(dirs)) => {
                    MaildirReply::read(dirs, tree)
                }
                state => panic!("unexpected {state:?}"),
            });
        }
    }

    #[test]
    fn any_number_of_ids_costs_one_request() {
        let tree = [
            ("root/new/a", FILE),
            ("root/cur/b:2,S", FILE),
            ("root/cur/c:2,FS", FILE),
            ("root/cur/d:2,", FILE),
        ];

        let (found, requests) = locate(&["a", "b", "c:2,FS", "missing"], &[&tree]);
        let paths: Vec<&str> = found.values().map(|out| out.path.as_str()).collect();

        assert_eq!(paths, ["root/new/a", "root/cur/b:2,S", "root/cur/c:2,FS"]);
        assert_eq!(found["b"].subdir, MaildirSubdir::Cur);
        assert_eq!(found["c:2,FS"].flags.to_string(), "FS");
        assert_eq!(requests, 1);
    }

    #[test]
    fn each_id_resolves_as_it_does_alone() {
        let tree = [
            ("root/new/a:2,F", FILE),
            ("root/cur/a", FILE),
            ("root/tmp/b", FILE),
            ("root/cur/b", FILE),
            ("root/tmp/c:2,S", FILE),
            ("root/new/d:2,S", FILE),
            ("root/new/d:2,F", FILE),
            ("root/cur/d:2,", FILE),
            ("root/cur/e:2,S", MaildirFileKind::Dir),
            ("root/cur/.f", FILE),
            ("root/cur/g1:2,S", FILE),
        ];
        let ids = ["a", "b", "c", "c:2,S", "d", "e", ".f", "g", "", "a/b"];

        let (found, _) = locate(&ids, &[&tree]);

        for id in ids {
            let alone = locate_one(id, &tree);
            let batched = found.get(id).map(|out| out.path.clone());
            assert_eq!(batched, alone, "{id:?}");
        }
    }

    #[test]
    fn a_match_gone_while_read_is_looked_for_again() {
        let first = [("root/cur/a:2,", MISSING), ("root/cur/b:2,S", FILE)];
        let second = [("root/cur/a:2,S", FILE), ("root/cur/b:2,S", FILE)];

        let (found, requests) = locate(&["a", "b"], &[&first, &second]);
        assert_eq!(found["a"].path, MaildirFsPath::from("root/cur/a:2,S"));
        assert_eq!(requests, 2);

        let (found, requests) = locate(&["a", "b"], &[&first]);
        assert!(!found.contains_key("a") && found.contains_key("b"));
        assert_eq!(requests, 3);
    }

    #[test]
    fn unexpected_reply_returns_error() {
        let mut cor = MaildirEntriesLocate::new(maildir(), ["a"]);
        let _ = cor.resume(None);

        match cor.resume(Some(MaildirReply::DirCreate)) {
            MaildirCoroutineState::Complete(Err(MaildirEntriesLocateError::UnexpectedArg(_))) => {}
            state => panic!("expected Complete(Err), got {state:?}"),
        }
    }
}
