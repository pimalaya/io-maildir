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
    entry::MaildirEntry,
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
                if self.id.is_empty()
                    || self.id.starts_with('.')
                    || self.id.contains('\0')
                    || self.id.contains('/')
                    || (cfg!(windows) && (self.id.contains('\\') || self.id.contains(':')))
                {
                    return MaildirCoroutineState::Complete(Err(
                        MaildirEntryLocateError::NotFound(self.id.clone()),
                    ));
                }
                let new_path = self.maildir.new().join(&self.id);
                let tmp_path = self.maildir.tmp().join(&self.id);
                let probes = BTreeSet::from_iter([new_path.clone(), tmp_path.clone()]);
                self.state = State::Probe { new_path, tmp_path };
                MaildirCoroutineState::Yielded(MaildirYield::WantsFileExists(probes))
            }
            (State::Probe { new_path, tmp_path }, Some(MaildirReply::FileExists(probes))) => {
                if probes.get(new_path).copied().unwrap_or(false) {
                    let out = MaildirEntryLocateOutput {
                        path: new_path.clone(),
                        subdir: MaildirSubdir::New,
                        flags: MaildirFlags::from(&*new_path),
                    };
                    debug!("located entry");
                    trace!("path: {}", out.path);
                    return MaildirCoroutineState::Complete(Ok(out));
                }

                if probes.get(tmp_path).copied().unwrap_or(false) {
                    let out = MaildirEntryLocateOutput {
                        path: tmp_path.clone(),
                        subdir: MaildirSubdir::Tmp,
                        flags: MaildirFlags::from(&*tmp_path),
                    };
                    debug!("located entry");
                    trace!("path: {}", out.path);
                    return MaildirCoroutineState::Complete(Ok(out));
                }

                let paths = BTreeSet::from_iter([self.maildir.new(), self.maildir.cur()]);
                self.state = State::Read;
                MaildirCoroutineState::Yielded(MaildirYield::WantsDirRead(paths))
            }
            (State::Read, Some(MaildirReply::DirRead(entries))) => {
                let mut candidates = BTreeMap::new();
                for (dir, paths) in entries {
                    let subdir = if dir == self.maildir.new() {
                        MaildirSubdir::New
                    } else if dir == self.maildir.cur() {
                        MaildirSubdir::Cur
                    } else {
                        continue;
                    };
                    for path in paths {
                        let Some(name) = path.file_name() else {
                            continue;
                        };
                        if name.starts_with('.') {
                            continue;
                        }
                        if name == self.id
                            || MaildirEntry::from_path(path.clone()).id() == Some(self.id.as_str())
                        {
                            candidates.insert(path, subdir.clone());
                        }
                    }
                }
                let paths = candidates.keys().cloned().collect();
                if candidates.is_empty() {
                    return MaildirCoroutineState::Complete(Err(
                        MaildirEntryLocateError::NotFound(self.id.clone()),
                    ));
                }
                self.state = State::Candidates { candidates };
                MaildirCoroutineState::Yielded(MaildirYield::WantsFileExists(paths))
            }
            (State::Candidates { candidates }, Some(MaildirReply::FileExists(probes))) => {
                let found = [MaildirSubdir::New, MaildirSubdir::Cur]
                    .into_iter()
                    .find_map(|wanted| {
                        candidates.iter().find(|(path, subdir)| {
                            **subdir == wanted && probes.get(path).copied().unwrap_or(false)
                        })
                    });
                if let Some((path, subdir)) = found {
                    let out = MaildirEntryLocateOutput {
                        path: path.clone(),
                        subdir: subdir.clone(),
                        flags: MaildirFlags::from(path),
                    };
                    debug!("located entry");
                    trace!("path: {}", out.path);
                    MaildirCoroutineState::Complete(Ok(out))
                } else {
                    MaildirCoroutineState::Complete(Err(MaildirEntryLocateError::NotFound(
                        self.id.clone(),
                    )))
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
    Probe {
        new_path: MaildirFsPath,
        tmp_path: MaildirFsPath,
    },
    Read,
    Candidates {
        candidates: BTreeMap<MaildirFsPath, MaildirSubdir>,
    },
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Start => f.write_str("start"),
            Self::Probe { .. } => f.write_str("probe new and tmp"),
            Self::Read => f.write_str("read new and cur"),
            Self::Candidates { .. } => f.write_str("probe candidates"),
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::collections::{BTreeMap, BTreeSet};

    use crate::entry::locate::*;

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
        let err = match cor.resume(Some(MaildirReply::DirRead(entries))) {
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
        if cfg!(windows) {
            for id in ["a\\b", "a:b"] {
                let mut cor = MaildirEntryLocate::new(maildir(), id);
                assert!(matches!(
                    cor.resume(None),
                    MaildirCoroutineState::Complete(Err(MaildirEntryLocateError::NotFound(_)))
                ));
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn unix_backslash_is_a_valid_basename() {
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
    fn vanished_and_nonfile_candidates_are_skipped() {
        let out = locate_scanned_with_replies(
            "abc",
            &[],
            &["root/cur/abc:2,F", "root/cur/abc:2,S"],
            &[false, true],
        );
        assert_eq!(out.path, MaildirFsPath::from("root/cur/abc:2,S"));
        assert!(matches!(
            locate_scanned_result("abc", &[], &["root/cur/abc:2,F"], &[false]),
            Err(MaildirEntryLocateError::NotFound(_))
        ));
    }

    fn locate_error(id: &str, new: &[&str], cur: &[&str]) -> MaildirEntryLocateError {
        match locate_scanned_result(id, new, cur, &[]) {
            Err(err) => err,
            Ok(out) => panic!("expected NotFound, got {}", out.path),
        }
    }

    fn locate_scanned(id: &str, new: &[&str], cur: &[&str]) -> MaildirEntryLocateOutput {
        locate_scanned_with_replies(id, new, cur, &vec![true; new.len() + cur.len()])
    }

    fn locate_scanned_with_replies(
        id: &str,
        new: &[&str],
        cur: &[&str],
        replies: &[bool],
    ) -> MaildirEntryLocateOutput {
        locate_scanned_result(id, new, cur, replies)
            .unwrap_or_else(|err| panic!("expected entry, got {err:?}"))
    }

    fn locate_scanned_result(
        id: &str,
        new: &[&str],
        cur: &[&str],
        replies: &[bool],
    ) -> Result<MaildirEntryLocateOutput, MaildirEntryLocateError> {
        let mut cor = MaildirEntryLocate::new(maildir(), id);
        let _ = expect_wants_file_exists(&mut cor);
        let mut probes = BTreeMap::new();
        probes.insert(MaildirFsPath::from(alloc::format!("root/new/{id}")), false);
        probes.insert(MaildirFsPath::from(alloc::format!("root/tmp/{id}")), false);
        let dirs = match cor.resume(Some(MaildirReply::FileExists(probes))) {
            MaildirCoroutineState::Yielded(MaildirYield::WantsDirRead(paths)) => paths,
            state => panic!("expected directory read, got {state:?}"),
        };
        assert!(
            dirs.contains(&MaildirFsPath::from("root/new"))
                && dirs.contains(&MaildirFsPath::from("root/cur"))
        );
        let mut entries = BTreeMap::new();
        entries.insert(
            MaildirFsPath::from("root/new"),
            new.iter().map(|p| MaildirFsPath::from(*p)).collect(),
        );
        entries.insert(
            MaildirFsPath::from("root/cur"),
            cur.iter().map(|p| MaildirFsPath::from(*p)).collect(),
        );
        let candidates = match cor.resume(Some(MaildirReply::DirRead(entries))) {
            MaildirCoroutineState::Yielded(MaildirYield::WantsFileExists(paths)) => paths,
            MaildirCoroutineState::Complete(Err(err)) => return Err(err),
            state => panic!("expected candidate probe, got {state:?}"),
        };
        let mut file_reply = BTreeMap::new();
        for (index, path) in candidates.iter().enumerate() {
            file_reply.insert(path.clone(), replies.get(index).copied().unwrap_or(true));
        }
        match cor.resume(Some(MaildirReply::FileExists(file_reply))) {
            MaildirCoroutineState::Complete(result) => result,
            state => panic!("expected completion, got {state:?}"),
        }
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
