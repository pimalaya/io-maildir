//! I/O-free coroutine locating a Maildir entry by its ID.
//!
//! # Example
//!
//! ```rust,no_run
//! use io_maildir::{
//!     client::MaildirClient,
//!     entry::locate::{MaildirEntryLocate, MaildirEntryLocateOutput},
//! };
//!
//! let client = MaildirClient::new("/path/to/root");
//! let maildir = client.load_maildir("inbox").unwrap();
//!
//! let coroutine = MaildirEntryLocate::new(maildir, "1700000000.1.M0P1.host");
//! let MaildirEntryLocateOutput { path, subdir, flags } = client.run(coroutine).unwrap();
//!
//! println!("found {path} in /{subdir} with flags `{flags}`");
//! ```

use core::fmt;

use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::{String, ToString},
};

use log::{debug, trace};
use thiserror::Error;

use crate::{
    coroutine::*,
    entry::{MAX_DIR_READS, is_reserved},
    flag::MaildirFlags,
    maildir::{Maildir, MaildirSubdir},
    path::MaildirFsPath,
};

/// Failure causes during a [`MaildirEntryLocate`] step.
#[derive(Clone, Debug, Error)]
pub enum MaildirEntryLocateError {
    /// A reply arrived that does not match the awaited step.
    #[error("Maildir message locate failed: unexpected arg {0:?}")]
    UnexpectedArg(Option<MaildirReply>),
    /// No entry with the given ID exists in the Maildir.
    #[error("Maildir message locate failed: message {0} not found")]
    NotFound(String),
}

/// Successful output of [`MaildirEntryLocate`].
#[derive(Clone, Debug)]
pub struct MaildirEntryLocateOutput {
    /// The filesystem path of the located entry.
    pub path: MaildirFsPath,
    /// The subdir the entry was found in.
    pub subdir: MaildirSubdir,
    /// The flags parsed from the entry file name.
    pub flags: MaildirFlags,
}

/// Locates an entry by exact logical ID or complete basename.
///
/// A complete basename is probed in `new/`, `tmp/` then `cur/`; on a
/// miss `new/` and `cur/` are read once and matched by logical ID, and
/// read again while a matching name is reported gone.
#[derive(Clone, Debug)]
pub struct MaildirEntryLocate {
    maildir: Maildir,
    id: String,
    state: State,
}

impl MaildirEntryLocate {
    /// Builds a coroutine locating the Maildir entry with the given ID.
    pub fn new(maildir: Maildir, id: impl ToString) -> Self {
        Self {
            maildir,
            id: id.to_string(),
            state: State::Start,
        }
    }

    /// The paths the id has as a complete name, in lookup order.
    fn exact_paths(&self) -> [(MaildirFsPath, MaildirSubdir); 3] {
        [
            (self.maildir.new().join(&self.id), MaildirSubdir::New),
            (self.maildir.tmp().join(&self.id), MaildirSubdir::Tmp),
            (self.maildir.cur().join(&self.id), MaildirSubdir::Cur),
        ]
    }

    /// Asks for the `reads`th read of `new/` and `cur/`.
    fn read(
        &mut self,
        reads: u8,
    ) -> MaildirCoroutineState<MaildirYield, <Self as MaildirCoroutine>::Return> {
        let paths = BTreeSet::from([self.maildir.new(), self.maildir.cur()]);
        self.state = State::Read { reads };
        MaildirCoroutineState::Yielded(MaildirYield::WantsDirRead(paths))
    }
}

impl MaildirCoroutine for MaildirEntryLocate {
    type Yield = MaildirYield;
    type Return = Result<MaildirEntryLocateOutput, MaildirEntryLocateError>;

    fn resume(
        &mut self,
        arg: Option<MaildirReply>,
    ) -> MaildirCoroutineState<Self::Yield, Self::Return> {
        match (&mut self.state, arg) {
            (State::Start, None) => {
                if !is_entry_name(&self.id, self.maildir.separator) {
                    return MaildirCoroutineState::Complete(Err(
                        MaildirEntryLocateError::NotFound(self.id.clone()),
                    ));
                }
                let probes = self.exact_paths().into_iter().map(|(path, _)| path);
                self.state = State::Probe;
                MaildirCoroutineState::Yielded(MaildirYield::WantsFileExists(probes.collect()))
            }
            (State::Probe, Some(MaildirReply::FileExists(probes))) => {
                let mut exact = self.exact_paths().into_iter();
                let found = exact.find(|(path, _)| probes.get(path).copied().unwrap_or(false));

                if let Some((path, subdir)) = found {
                    let entry = self.maildir.entry(path);
                    let out = MaildirEntryLocateOutput {
                        flags: entry.flags(),
                        path: entry.into(),
                        subdir,
                    };
                    debug!("located entry");
                    trace!("path: {}", out.path);
                    return MaildirCoroutineState::Complete(Ok(out));
                }

                self.read(1)
            }
            (State::Read { reads }, Some(MaildirReply::DirRead(entries))) => {
                let reads = *reads;
                let ids = BTreeSet::from([self.id.clone()]);
                let (mut found, gone) = scan(&self.maildir, &ids, entries);

                match found.remove(&self.id) {
                    Some(out) => {
                        debug!("located entry");
                        trace!("path: {}", out.path);
                        MaildirCoroutineState::Complete(Ok(out))
                    }
                    None if gone && reads < MAX_DIR_READS => self.read(reads + 1),
                    None => {
                        let err = MaildirEntryLocateError::NotFound(self.id.clone());
                        MaildirCoroutineState::Complete(Err(err))
                    }
                }
            }
            (_, arg) => {
                let err = MaildirEntryLocateError::UnexpectedArg(arg);
                MaildirCoroutineState::Complete(Err(err))
            }
        }
    }
}

#[derive(Clone, Debug)]
enum State {
    Start,
    Probe,
    Read { reads: u8 },
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Start => f.write_str("start"),
            Self::Probe => f.write_str("probe new, tmp and cur"),
            Self::Read { .. } => f.write_str("read new and cur"),
        }
    }
}

/// Returns `true` when `id` can name an entry of a Maildir whose info
/// separator is `separator`: not empty, not hidden and holding no
/// reserved character.
pub(crate) fn is_entry_name(id: &str, separator: char) -> bool {
    let reserved = |c| is_reserved(c, separator);
    !id.is_empty() && !id.starts_with('.') && !id.contains(reserved)
}

/// Finds the best match of each of `ids` among the `entries` of the
/// subdirectories of `maildir`, and tells whether a matching name was
/// gone when its kind was read.
///
/// A complete name wins over a logical id, then `new/` over `tmp/` over
/// `cur/`, then the first path in order. `tmp/` matches by complete name
/// only, and only a regular file matches.
pub(crate) fn scan(
    maildir: &Maildir,
    ids: &BTreeSet<String>,
    entries: BTreeMap<MaildirFsPath, BTreeMap<MaildirFsPath, MaildirFileKind>>,
) -> (BTreeMap<String, MaildirEntryLocateOutput>, bool) {
    let mut best: BTreeMap<&String, (u8, MaildirFsPath, MaildirSubdir)> = BTreeMap::new();
    let mut gone = false;

    for (dir, names) in entries {
        let (subdir, rank) = if dir == maildir.new() {
            (MaildirSubdir::New, 0)
        } else if dir == maildir.tmp() {
            (MaildirSubdir::Tmp, 1)
        } else if dir == maildir.cur() {
            (MaildirSubdir::Cur, 2)
        } else {
            continue;
        };

        for (path, kind) in names {
            let Some(name) = path.file_name().filter(|name| !name.starts_with('.')) else {
                continue;
            };

            let logical = match name.rsplit_once(maildir.separator) {
                Some((id, _)) if subdir != MaildirSubdir::Tmp => ids.get(id),
                _ => None,
            };
            let matches = [(ids.get(name), rank), (logical, rank + 3)];

            for (id, rank) in matches {
                let Some(id) = id else {
                    continue;
                };

                if kind == MaildirFileKind::Missing {
                    gone = true;
                }

                let held = best.get(id).map(|(rank, path, _)| (*rank, path));
                if kind == MaildirFileKind::File && held.is_none_or(|held| (rank, &path) < held) {
                    best.insert(id, (rank, path.clone(), subdir.clone()));
                }
            }
        }
    }

    let found = best.into_iter().map(|(id, (_, path, subdir))| {
        let entry = maildir.entry(path);
        let out = MaildirEntryLocateOutput {
            flags: entry.flags(),
            path: entry.into(),
            subdir,
        };
        (id.clone(), out)
    });

    (found.collect(), gone)
}

#[cfg(test)]
mod tests {
    use alloc::{
        collections::{BTreeMap, BTreeSet},
        vec::Vec,
    };

    use crate::entry::locate::*;

    const FILE: MaildirFileKind = MaildirFileKind::File;
    const MISSING: MaildirFileKind = MaildirFileKind::Missing;

    fn maildir() -> Maildir {
        Maildir::from_path("root")
    }

    #[test]
    fn found_in_new_returns_ok() {
        let mut cor = MaildirEntryLocate::new(maildir(), "abc");

        let probes = expect_wants_file_exists(&mut cor);
        let new_path = MaildirFsPath::from("root/new/abc");
        let tmp_path = MaildirFsPath::from("root/tmp/abc");
        assert!(probes.contains(&new_path));
        assert!(probes.contains(&tmp_path));

        let mut map = BTreeMap::new();
        map.insert(new_path.clone(), true);
        map.insert(tmp_path, false);
        let out = expect_complete_ok(&mut cor, Some(MaildirReply::FileExists(map)));
        assert_eq!(out.path, new_path);
        assert_eq!(out.subdir, MaildirSubdir::New);
    }

    #[test]
    fn not_found_returns_error() {
        let mut cor = MaildirEntryLocate::new(maildir(), "abc");

        expect_wants_file_exists(&mut cor);

        let mut probe = BTreeMap::new();
        probe.insert(MaildirFsPath::from("root/new/abc"), false);
        probe.insert(MaildirFsPath::from("root/tmp/abc"), false);
        match cor.resume(Some(MaildirReply::FileExists(probe))) {
            MaildirCoroutineState::Yielded(MaildirYield::WantsDirRead(paths)) => {
                assert!(paths.contains(&MaildirFsPath::from("root/cur")));
                assert!(paths.contains(&MaildirFsPath::from("root/new")));
            }
            state => panic!("expected WantsDirRead, got {state:?}"),
        }

        let mut entries = BTreeMap::new();
        entries.insert(MaildirFsPath::from("root/cur"), BTreeSet::new());
        entries.insert(MaildirFsPath::from("root/new"), BTreeSet::new());
        let err = match cor.resume(Some(MaildirReply::files(entries))) {
            MaildirCoroutineState::Complete(Err(err)) => err,
            state => panic!("expected NotFound, got {state:?}"),
        };
        assert!(matches!(err, MaildirEntryLocateError::NotFound(_)));
    }

    #[test]
    fn suffixed_new_is_found_by_logical_id_with_flags() {
        let out = locate_scanned("abc,U=5", &["root/new/abc,U=5:2,F"], &[]);
        assert_eq!(out.path, MaildirFsPath::from("root/new/abc,U=5:2,F"));
        assert_eq!(out.subdir, MaildirSubdir::New);
        assert!(out.flags.contains(&crate::flag::MaildirFlag::Flagged));
    }

    #[test]
    fn prefix_cur_candidate_is_not_a_match() {
        assert!(matches!(
            locate_error("abc", &[], &["root/cur/abc123:2,S"]),
            MaildirEntryLocateError::NotFound(_)
        ));
        assert!(matches!(
            locate_error("1", &[], &["root/cur/123:2,S"]),
            MaildirEntryLocateError::NotFound(_)
        ));
    }

    #[test]
    fn exact_complete_basename_and_stale_suffix_are_handled_exactly() {
        let out = locate_scanned("abc:2,S", &[], &["root/cur/abc:2,S"]);
        assert_eq!(out.path, MaildirFsPath::from("root/cur/abc:2,S"));
        assert_eq!(out.subdir, MaildirSubdir::Cur);
        assert!(out.flags.contains(&crate::flag::MaildirFlag::Seen));
        assert!(matches!(
            locate_error("abc:2,F", &[], &["root/cur/abc:2,S"]),
            MaildirEntryLocateError::NotFound(_)
        ));
    }

    #[test]
    fn scan_priority_is_new_then_cur_then_lexicographic() {
        let out = locate_scanned(
            "abc",
            &["root/new/abc:2,F", "root/new/abc:2,S"],
            &["root/cur/abc:2,S"],
        );
        assert_eq!(out.path, MaildirFsPath::from("root/new/abc:2,F"));
        assert_eq!(out.subdir, MaildirSubdir::New);
    }

    #[test]
    fn exact_tmp_flags_are_parsed_and_suffixed_tmp_is_not_scanned() {
        let mut cor = MaildirEntryLocate::new(maildir(), "abc:2,F");
        let probes = expect_wants_file_exists(&mut cor);
        let mut reply = BTreeMap::new();
        reply.insert(MaildirFsPath::from("root/new/abc:2,F"), false);
        reply.insert(MaildirFsPath::from("root/tmp/abc:2,F"), true);
        let out = expect_complete_ok(&mut cor, Some(MaildirReply::FileExists(reply)));
        assert!(probes.contains(&out.path));
        assert_eq!(out.subdir, MaildirSubdir::Tmp);
        assert!(out.flags.contains(&crate::flag::MaildirFlag::Flagged));

        assert!(matches!(
            locate_error("abc", &[], &[]),
            MaildirEntryLocateError::NotFound(_)
        ));
    }

    #[test]
    fn invalid_inputs_fail_before_yielding() {
        for id in ["", ".hidden", "a/b", "a\0b"] {
            let mut cor = MaildirEntryLocate::new(maildir(), id);
            assert!(matches!(
                cor.resume(None),
                MaildirCoroutineState::Complete(Err(MaildirEntryLocateError::NotFound(_)))
            ));
        }

        let mut maildir = maildir();
        maildir.separator = ';';

        for id in ["a\\b", "a:b"] {
            let mut cor = MaildirEntryLocate::new(maildir.clone(), id);
            assert!(matches!(
                cor.resume(None),
                MaildirCoroutineState::Complete(Err(MaildirEntryLocateError::NotFound(_)))
            ));
        }
    }

    #[test]
    fn a_backslash_is_a_valid_basename_on_the_colon() {
        let id = "a\\b";
        let mut cor = MaildirEntryLocate::new(maildir(), id);
        let _ = expect_wants_file_exists(&mut cor);
        let mut probes = BTreeMap::new();
        probes.insert(MaildirFsPath::from("root/new/a\\b"), true);
        probes.insert(MaildirFsPath::from("root/tmp/a\\b"), false);
        let out = expect_complete_ok(&mut cor, Some(MaildirReply::FileExists(probes)));
        assert_eq!(out.path, MaildirFsPath::from("root/new/a\\b"));
        assert_eq!(out.subdir, MaildirSubdir::New);
    }

    #[test]
    fn candidates_that_are_no_files_are_skipped() {
        let tree = [
            ("root/cur/abc:2,F", MaildirFileKind::Dir),
            ("root/cur/abc:2,R", MaildirFileKind::Other),
            ("root/cur/abc:2,S", FILE),
        ];
        let (out, _) = locate("abc", &[&tree]);
        assert_eq!(out.unwrap().path, MaildirFsPath::from("root/cur/abc:2,S"));

        let (out, _) = locate("abc", &[&tree[..2]]);
        assert!(matches!(out, Err(MaildirEntryLocateError::NotFound(_))));
    }

    #[test]
    fn a_candidate_gone_while_read_is_located_under_the_name_it_took() {
        let first = [("root/cur/abc:2,", MISSING)];
        let second = [("root/cur/abc:2,S", FILE)];

        let (out, requests) = locate("abc", &[&first, &second]);
        assert_eq!(out.unwrap().path, MaildirFsPath::from("root/cur/abc:2,S"));
        assert_eq!(requests, 3);
    }

    #[test]
    fn a_candidate_that_stays_gone_is_not_found() {
        let (out, requests) = locate("abc", &[&[("root/cur/abc:2,", MISSING)]]);
        assert!(matches!(out, Err(MaildirEntryLocateError::NotFound(_))));
        assert_eq!(requests, 4);
    }

    #[test]
    fn a_complete_basename_costs_one_request_and_an_id_two() {
        let tree = [("root/cur/abc:2,S", FILE), ("root/cur/abd:2,S", FILE)];

        let (out, requests) = locate("abc:2,S", &[&tree]);
        assert_eq!(out.unwrap().subdir, MaildirSubdir::Cur);
        assert_eq!(requests, 1);

        let (out, requests) = locate("abc", &[&tree]);
        assert_eq!(out.unwrap().path, MaildirFsPath::from("root/cur/abc:2,S"));
        assert_eq!(requests, 2);
    }

    #[test]
    fn an_exact_name_wins_over_a_scanned_one() {
        let tree = [
            ("root/cur/abc", FILE),
            ("root/new/abc:2,F", FILE),
            ("root/tmp/abd", FILE),
            ("root/cur/abd", FILE),
        ];

        let (out, _) = locate("abc", &[&tree]);
        assert_eq!(out.unwrap().path, MaildirFsPath::from("root/cur/abc"));
        let (out, _) = locate("abd", &[&tree]);
        assert_eq!(out.unwrap().subdir, MaildirSubdir::Tmp);
    }

    /// Locates `id` in a Maildir holding the next of `reads` at each
    /// directory read, returning the outcome and the requests it took.
    fn locate(
        id: &str,
        reads: &[&[(&str, MaildirFileKind)]],
    ) -> (
        Result<MaildirEntryLocateOutput, MaildirEntryLocateError>,
        usize,
    ) {
        let mut cor = MaildirEntryLocate::new(maildir(), id);
        let mut requests = 0;
        let mut read = 0;
        let mut arg = None;

        loop {
            let wants = match cor.resume(arg.take()) {
                MaildirCoroutineState::Complete(out) => return (out, requests),
                MaildirCoroutineState::Yielded(wants) => wants,
            };

            let tree = reads[read.min(reads.len() - 1)];
            requests += 1;

            arg = Some(match wants {
                MaildirYield::WantsFileExists(paths) => {
                    let mut probes = BTreeMap::new();
                    for path in paths {
                        let exists = tree.contains(&(path.as_str(), FILE));
                        probes.insert(path, exists);
                    }
                    MaildirReply::FileExists(probes)
                }
                MaildirYield::WantsDirRead(dirs) => {
                    read += 1;
                    MaildirReply::read(dirs, tree)
                }
                wants => panic!("unexpected {wants:?}"),
            });
        }
    }

    fn locate_error(id: &str, new: &[&str], cur: &[&str]) -> MaildirEntryLocateError {
        match locate_scanned_result(id, new, cur) {
            Err(err) => err,
            Ok(out) => panic!("expected NotFound, got {}", out.path),
        }
    }

    fn locate_scanned(id: &str, new: &[&str], cur: &[&str]) -> MaildirEntryLocateOutput {
        locate_scanned_result(id, new, cur)
            .unwrap_or_else(|err| panic!("expected entry, got {err:?}"))
    }

    fn locate_scanned_result(
        id: &str,
        new: &[&str],
        cur: &[&str],
    ) -> Result<MaildirEntryLocateOutput, MaildirEntryLocateError> {
        let paths = new.iter().chain(cur);
        let tree: Vec<(&str, MaildirFileKind)> = paths.map(|path| (*path, FILE)).collect();
        locate(id, &[&tree]).0
    }

    #[test]
    fn unexpected_reply_returns_error() {
        let mut cor = MaildirEntryLocate::new(maildir(), "abc");
        expect_wants_file_exists(&mut cor);

        let err = expect_complete_err(&mut cor, Some(MaildirReply::DirCreate));
        assert!(matches!(err, MaildirEntryLocateError::UnexpectedArg(_)));
    }

    fn expect_wants_file_exists(cor: &mut MaildirEntryLocate) -> BTreeSet<MaildirFsPath> {
        match cor.resume(None) {
            MaildirCoroutineState::Yielded(MaildirYield::WantsFileExists(paths)) => paths,
            state => panic!("expected WantsFileExists, got {state:?}"),
        }
    }

    fn expect_complete_ok(
        cor: &mut MaildirEntryLocate,
        arg: Option<MaildirReply>,
    ) -> MaildirEntryLocateOutput {
        match cor.resume(arg) {
            MaildirCoroutineState::Complete(Ok(out)) => out,
            state => panic!("expected Complete(Ok), got {state:?}"),
        }
    }

    fn expect_complete_err(
        cor: &mut MaildirEntryLocate,
        arg: Option<MaildirReply>,
    ) -> MaildirEntryLocateError {
        match cor.resume(arg) {
            MaildirCoroutineState::Complete(Err(err)) => err,
            state => panic!("expected Complete(Err), got {state:?}"),
        }
    }
}
