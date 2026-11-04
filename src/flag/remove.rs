//! I/O-free coroutine removing flags from a Maildir entry.
//!
//! # Example
//!
//! ```rust,no_run
//! use io_maildir::{
//!     client::MaildirClient,
//!     flag::{remove::MaildirFlagsRemove, MaildirFlag, MaildirFlags},
//! };
//!
//! let client = MaildirClient::new("/path/to/root");
//! let maildir = client.load_maildir("inbox").unwrap();
//!
//! let flags = MaildirFlags::from_iter([MaildirFlag::Seen]);
//! let coroutine = MaildirFlagsRemove::new(maildir, "1700000000.1.M0P1.host", flags);
//! client.run(coroutine).unwrap();
//! ```

use core::fmt;

use alloc::string::{String, ToString};

use log::debug;
use thiserror::Error;

use crate::{
    coroutine::*,
    entry::INFORMATIONAL_SUFFIX_SEPARATOR,
    entry::locate::*,
    flag::MaildirFlags,
    maildir::{Maildir, MaildirSubdir},
    maildir_try,
    path::MaildirFsPath,
};

/// Failure causes during a [`MaildirFlagsRemove`] step.
#[derive(Clone, Debug, Error)]
pub enum MaildirFlagsRemoveError {
    /// A reply arrived that does not match the awaited step.
    #[error("Maildir flags remove failed: unexpected arg {0:?}")]
    UnexpectedArg(Option<MaildirReply>),
    /// The inner locate step failed.
    #[error(transparent)]
    Locate(#[from] MaildirEntryLocateError),
}

/// Removes flags from a Maildir entry. Changed flags on `/new` entries
/// are written using the same transition rules as add and set.
#[derive(Debug)]
pub struct MaildirFlagsRemove {
    maildir: Maildir,
    id: String,
    flags: MaildirFlags,
    state: State,
}

impl MaildirFlagsRemove {
    /// Builds a coroutine removing flags from a Maildir entry.
    pub fn new(maildir: Maildir, id: impl ToString, flags: MaildirFlags) -> Self {
        let id = id.to_string();
        Self {
            state: State::Locate(MaildirEntryLocate::new(maildir.clone(), &id)),
            maildir,
            id,
            flags,
        }
    }
}

impl MaildirCoroutine for MaildirFlagsRemove {
    type Yield = MaildirYield;
    type Return = Result<(), MaildirFlagsRemoveError>;

    fn resume(
        &mut self,
        arg: Option<MaildirReply>,
    ) -> MaildirCoroutineState<Self::Yield, Self::Return> {
        match (&mut self.state, arg) {
            (State::Locate(c), arg) => {
                let out = maildir_try!(c, arg);

                match out.subdir.clone() {
                    MaildirSubdir::Tmp => {
                        debug!("removed flags");
                        MaildirCoroutineState::Complete(Ok(()))
                    }
                    MaildirSubdir::Cur | MaildirSubdir::New => {
                        let original_flags = out.flags;
                        let mut existing = original_flags.clone();
                        existing.difference(&self.flags);
                        if out.subdir == MaildirSubdir::New && existing == original_flags {
                            debug!("removed flags");
                            return MaildirCoroutineState::Complete(Ok(()));
                        }
                        let id = crate::entry::MaildirEntry::from_path(out.path.clone())
                            .id()
                            .unwrap_or(&self.id)
                            .to_string();
                        let new_path = if out.subdir == MaildirSubdir::New && existing.is_empty() {
                            self.maildir.new().join(&id)
                        } else if out.subdir == MaildirSubdir::New {
                            cur_path_with_flags(&self.maildir, &id, &existing)
                        } else {
                            rename_with_flags(&out.path, &id, &existing)
                        };
                        let pairs = vec![(out.path, new_path)];
                        self.state = State::Rename;
                        MaildirCoroutineState::Yielded(MaildirYield::WantsRename(pairs))
                    }
                }
            }
            (State::Rename, Some(MaildirReply::Rename)) => {
                debug!("removed flags");
                MaildirCoroutineState::Complete(Ok(()))
            }
            (_, arg) => {
                let err = MaildirFlagsRemoveError::UnexpectedArg(arg);
                MaildirCoroutineState::Complete(Err(err))
            }
        }
    }
}

#[derive(Debug)]
enum State {
    Locate(MaildirEntryLocate),
    Rename,
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Locate(_) => f.write_str("locate message"),
            Self::Rename => f.write_str("rename entry"),
        }
    }
}

fn rename_with_flags(path: &MaildirFsPath, id: &str, flags: &MaildirFlags) -> MaildirFsPath {
    let mut name = String::from(id);
    name.push(INFORMATIONAL_SUFFIX_SEPARATOR);
    name.push_str("2,");
    name.push_str(&flags.to_string());
    path.with_file_name(&name)
}

fn cur_path_with_flags(maildir: &Maildir, id: &str, flags: &MaildirFlags) -> MaildirFsPath {
    let mut name = String::from(id);
    name.push(INFORMATIONAL_SUFFIX_SEPARATOR);
    name.push_str("2,");
    name.push_str(&flags.to_string());
    maildir.cur().join(&name)
}

#[cfg(test)]
mod tests {
    use alloc::collections::{BTreeMap, BTreeSet};

    use crate::flag::remove::*;

    fn maildir() -> Maildir {
        Maildir::from_path("root")
    }

    #[test]
    fn new_subdir_returns_noop_ok() {
        let mut cor = MaildirFlagsRemove::new(maildir(), "abc", MaildirFlags::default());

        expect_wants_file_exists(&mut cor);

        let mut probes = BTreeMap::new();
        probes.insert(MaildirFsPath::from("root/new/abc"), true);
        probes.insert(MaildirFsPath::from("root/tmp/abc"), false);
        expect_complete_ok(&mut cor, Some(MaildirReply::FileExists(probes)));
    }

    #[test]
    fn flagged_new_entry_removing_last_flag_stays_new_with_plain_name() {
        let mut cor = MaildirFlagsRemove::new(
            maildir(),
            "abc",
            MaildirFlags::from_iter([crate::flag::MaildirFlag::Flagged]),
        );
        expect_wants_file_exists(&mut cor);
        let mut probes = BTreeMap::new();
        probes.insert(MaildirFsPath::from("root/new/abc"), false);
        probes.insert(MaildirFsPath::from("root/tmp/abc"), false);
        assert!(matches!(
            cor.resume(Some(MaildirReply::FileExists(probes))),
            MaildirCoroutineState::Yielded(MaildirYield::WantsDirRead(_))
        ));
        let mut entries = BTreeMap::new();
        entries.insert(
            MaildirFsPath::from("root/new"),
            BTreeSet::from([MaildirFsPath::from("root/new/abc:2,F")]),
        );
        entries.insert(MaildirFsPath::from("root/cur"), BTreeSet::new());
        assert!(matches!(
            cor.resume(Some(MaildirReply::DirRead(entries))),
            MaildirCoroutineState::Yielded(MaildirYield::WantsFileExists(_))
        ));
        let candidates = BTreeMap::from([(MaildirFsPath::from("root/new/abc:2,F"), true)]);
        let pairs = expect_wants_rename(&mut cor, Some(MaildirReply::FileExists(candidates)));
        assert_eq!(
            pairs,
            alloc::vec![("root/new/abc:2,F".into(), "root/new/abc".into())]
        );
    }

    #[test]
    fn flagged_new_entry_preserves_remaining_flags_in_cur() {
        let mut cor = MaildirFlagsRemove::new(
            maildir(),
            "abc",
            MaildirFlags::from_iter([crate::flag::MaildirFlag::Flagged]),
        );
        expect_wants_file_exists(&mut cor);
        let mut probes = BTreeMap::new();
        probes.insert(MaildirFsPath::from("root/new/abc"), false);
        probes.insert(MaildirFsPath::from("root/tmp/abc"), false);
        assert!(matches!(
            cor.resume(Some(MaildirReply::FileExists(probes))),
            MaildirCoroutineState::Yielded(MaildirYield::WantsDirRead(_))
        ));
        let mut entries = BTreeMap::new();
        entries.insert(
            MaildirFsPath::from("root/new"),
            BTreeSet::from([MaildirFsPath::from("root/new/abc:2,FSx")]),
        );
        entries.insert(MaildirFsPath::from("root/cur"), BTreeSet::new());
        assert!(matches!(
            cor.resume(Some(MaildirReply::DirRead(entries))),
            MaildirCoroutineState::Yielded(MaildirYield::WantsFileExists(_))
        ));
        let candidates = BTreeMap::from([(MaildirFsPath::from("root/new/abc:2,FSx"), true)]);
        let pairs = expect_wants_rename(&mut cor, Some(MaildirReply::FileExists(candidates)));
        assert_eq!(pairs[0].1, MaildirFsPath::from("root/cur/abc:2,Sx"));
    }

    #[test]
    fn full_basename_uses_logical_id_once() {
        let mut cor = MaildirFlagsRemove::new(
            maildir(),
            "abc:2,F",
            MaildirFlags::from_iter([crate::flag::MaildirFlag::Flagged]),
        );
        expect_wants_file_exists(&mut cor);
        let mut probes = BTreeMap::new();
        probes.insert(MaildirFsPath::from("root/new/abc:2,F"), false);
        probes.insert(MaildirFsPath::from("root/tmp/abc:2,F"), false);
        assert!(matches!(
            cor.resume(Some(MaildirReply::FileExists(probes))),
            MaildirCoroutineState::Yielded(MaildirYield::WantsDirRead(_))
        ));
        let mut entries = BTreeMap::new();
        entries.insert(MaildirFsPath::from("root/new"), BTreeSet::new());
        entries.insert(
            MaildirFsPath::from("root/cur"),
            BTreeSet::from([MaildirFsPath::from("root/cur/abc:2,F")]),
        );
        assert!(matches!(
            cor.resume(Some(MaildirReply::DirRead(entries))),
            MaildirCoroutineState::Yielded(MaildirYield::WantsFileExists(_))
        ));
        let candidates = BTreeMap::from([(MaildirFsPath::from("root/cur/abc:2,F"), true)]);
        let pairs = expect_wants_rename(&mut cor, Some(MaildirReply::FileExists(candidates)));
        assert_eq!(pairs[0].1, MaildirFsPath::from("root/cur/abc:2,"));
    }

    #[test]
    fn unexpected_reply_returns_error() {
        let mut cor = MaildirFlagsRemove::new(maildir(), "abc", MaildirFlags::default());
        expect_wants_file_exists(&mut cor);

        let err = expect_complete_err(&mut cor, Some(MaildirReply::DirCreate));
        assert!(matches!(err, MaildirFlagsRemoveError::Locate(_)));
    }

    fn expect_wants_file_exists(cor: &mut MaildirFlagsRemove) {
        match cor.resume(None) {
            MaildirCoroutineState::Yielded(MaildirYield::WantsFileExists(_)) => {}
            state => panic!("expected WantsFileExists, got {state:?}"),
        }
    }

    fn expect_wants_rename(
        cor: &mut MaildirFlagsRemove,
        arg: Option<MaildirReply>,
    ) -> alloc::vec::Vec<(MaildirFsPath, MaildirFsPath)> {
        let state = cor.resume(arg);
        let state = match state {
            MaildirCoroutineState::Yielded(MaildirYield::WantsFileExists(paths)) => {
                cor.resume(Some(MaildirReply::FileExists(BTreeMap::from_iter(
                    paths.into_iter().map(|path| (path, true)),
                ))))
            }
            state => state,
        };
        match state {
            MaildirCoroutineState::Yielded(MaildirYield::WantsRename(pairs)) => pairs,
            state => panic!("expected WantsRename, got {state:?}"),
        }
    }

    fn expect_complete_ok(cor: &mut MaildirFlagsRemove, arg: Option<MaildirReply>) {
        match cor.resume(arg) {
            MaildirCoroutineState::Complete(Ok(())) => {}
            state => panic!("expected Complete(Ok), got {state:?}"),
        }
    }

    fn expect_complete_err(
        cor: &mut MaildirFlagsRemove,
        arg: Option<MaildirReply>,
    ) -> MaildirFlagsRemoveError {
        match cor.resume(arg) {
            MaildirCoroutineState::Complete(Err(err)) => err,
            state => panic!("expected Complete(Err), got {state:?}"),
        }
    }
}
