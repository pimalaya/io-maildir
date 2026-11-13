//! I/O-free coroutine replacing the flags of a Maildir entry.
//!
//! # Example
//!
//! ```rust,no_run
//! use io_maildir::{
//!     client::MaildirClient,
//!     flag::{set::MaildirFlagsSet, MaildirFlag, MaildirFlags},
//! };
//!
//! let client = MaildirClient::new("/path/to/root");
//! let maildir = client.load_maildir("inbox").unwrap();
//!
//! let flags = MaildirFlags::from_iter([MaildirFlag::Seen, MaildirFlag::Flagged]);
//! let coroutine = MaildirFlagsSet::new(maildir, "1700000000.1.M0P1.host", flags);
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
    flag::{MaildirFlags, has_foreign_info},
    maildir::{Maildir, MaildirSubdir},
    maildir_try,
    path::MaildirFsPath,
};

/// Failure causes during a [`MaildirFlagsSet`] step.
#[derive(Clone, Debug, Error)]
pub enum MaildirFlagsSetError {
    /// A reply arrived that does not match the awaited step.
    #[error("Maildir flags set failed: unexpected arg {0:?}")]
    UnexpectedArg(Option<MaildirReply>),
    /// The inner locate step failed.
    #[error(transparent)]
    Locate(#[from] MaildirEntryLocateError),
    /// The entry carries an info section other than `2,<flags>`, such
    /// as an experimental `:1,` one, which a flag write would lose.
    #[error("Maildir flags set failed: entry {0} carries a non-flag info section")]
    ForeignInfo(MaildirFsPath),
}

/// Replaces the flags of a Maildir entry.
///
/// The info section is rebuilt from the given set alone: unlike an add
/// or a remove, any letter the entry carried and the set does not name,
/// a dovecot keyword slot included, is dropped.
///
/// Entries in `/cur` are renamed with the new flag set. A `/new` entry
/// moves to `/cur` when its flags change to a nonempty set, and stays in
/// `/new` under its plain logical ID when cleared. An unchanged entry
/// and an entry in `/tmp` are left untouched.
#[derive(Debug)]
pub struct MaildirFlagsSet {
    maildir: Maildir,
    id: String,
    flags: MaildirFlags,
    state: State,
}

impl MaildirFlagsSet {
    /// Builds a coroutine replacing the flags of a Maildir entry.
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

impl MaildirCoroutine for MaildirFlagsSet {
    type Yield = MaildirYield;
    type Return = Result<(), MaildirFlagsSetError>;

    fn resume(
        &mut self,
        arg: Option<MaildirReply>,
    ) -> MaildirCoroutineState<Self::Yield, Self::Return> {
        match (&mut self.state, arg) {
            (State::Locate(c), arg) => {
                let out = maildir_try!(c, arg);

                if out.path.file_name().is_some_and(has_foreign_info) {
                    let err = MaildirFlagsSetError::ForeignInfo(out.path);
                    return MaildirCoroutineState::Complete(Err(err));
                }

                match out.subdir.clone() {
                    MaildirSubdir::Tmp => {
                        debug!("set flags");
                        MaildirCoroutineState::Complete(Ok(()))
                    }
                    MaildirSubdir::Cur | MaildirSubdir::New => {
                        if out.subdir == MaildirSubdir::New && self.flags == out.flags {
                            debug!("set flags");
                            return MaildirCoroutineState::Complete(Ok(()));
                        }
                        let id = crate::entry::MaildirEntry::from_path(out.path.clone())
                            .id()
                            .unwrap_or(&self.id)
                            .to_string();
                        let new_path = if out.subdir == MaildirSubdir::New && self.flags.is_empty()
                        {
                            self.maildir.new().join(&id)
                        } else {
                            cur_path_with_flags(&self.maildir, &id, &self.flags)
                        };
                        let pairs = vec![(out.path, new_path)];
                        self.state = State::Rename;
                        MaildirCoroutineState::Yielded(MaildirYield::WantsRename(pairs))
                    }
                }
            }
            (State::Rename, Some(MaildirReply::Rename)) => {
                debug!("set flags");
                MaildirCoroutineState::Complete(Ok(()))
            }
            (_, arg) => {
                let err = MaildirFlagsSetError::UnexpectedArg(arg);
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

fn cur_path_with_flags(maildir: &Maildir, id: &str, flags: &MaildirFlags) -> MaildirFsPath {
    let mut name = String::from(id);
    name.push(INFORMATIONAL_SUFFIX_SEPARATOR);
    name.push_str("2,");
    name.push_str(&flags.to_string());
    maildir.cur().join(&name)
}

#[cfg(test)]
mod tests {
    use alloc::{
        collections::{BTreeMap, BTreeSet},
        vec::Vec,
    };

    use crate::flag::{MaildirFlag, set::*};

    fn maildir() -> Maildir {
        Maildir::from_path("root")
    }

    fn seen() -> MaildirFlags {
        MaildirFlags::from_iter([MaildirFlag::Seen])
    }

    #[test]
    fn cur_subdir_renames_with_the_new_flags() {
        let mut cor = MaildirFlagsSet::new(maildir(), "abc", seen());

        expect_wants_file_exists(&mut cor);

        let mut probes = BTreeMap::new();
        probes.insert(MaildirFsPath::from("root/new/abc"), false);
        probes.insert(MaildirFsPath::from("root/tmp/abc"), false);
        match cor.resume(Some(MaildirReply::FileExists(probes))) {
            MaildirCoroutineState::Yielded(MaildirYield::WantsDirRead(_)) => {}
            state => panic!("expected WantsDirRead, got {state:?}"),
        }

        let mut entries = BTreeMap::new();
        let mut set = BTreeSet::new();
        set.insert(MaildirFsPath::from("root/cur/abc:2,F"));
        entries.insert(MaildirFsPath::from("root/cur"), set);
        let pairs = expect_wants_rename(&mut cor, Some(MaildirReply::DirRead(entries)));
        let (from, to) = &pairs[0];
        assert_eq!(from, &MaildirFsPath::from("root/cur/abc:2,F"));
        assert_eq!(to, &MaildirFsPath::from("root/cur/abc:2,S"));

        expect_complete_ok(&mut cor, Some(MaildirReply::Rename));
    }

    #[test]
    fn cur_subdir_replaces_the_letters_the_set_does_not_name() {
        let mut flags = seen();
        flags.extend_letters(['c']);
        let mut cor = MaildirFlagsSet::new(maildir(), "abc", flags);

        expect_wants_file_exists(&mut cor);

        let mut probes = BTreeMap::new();
        probes.insert(MaildirFsPath::from("root/new/abc"), false);
        probes.insert(MaildirFsPath::from("root/tmp/abc"), false);
        match cor.resume(Some(MaildirReply::FileExists(probes))) {
            MaildirCoroutineState::Yielded(MaildirYield::WantsDirRead(_)) => {}
            state => panic!("expected WantsDirRead, got {state:?}"),
        }

        let mut entries = BTreeMap::new();
        let mut set = BTreeSet::new();
        set.insert(MaildirFsPath::from("root/cur/abc:2,FXab"));
        entries.insert(MaildirFsPath::from("root/cur"), set);
        let pairs = expect_wants_rename(&mut cor, Some(MaildirReply::DirRead(entries)));
        assert_eq!(pairs[0].1, MaildirFsPath::from("root/cur/abc:2,Sc"));
    }

    #[test]
    fn new_subdir_renames_into_cur() {
        let mut cor = MaildirFlagsSet::new(maildir(), "abc", seen());

        expect_wants_file_exists(&mut cor);

        let mut probes = BTreeMap::new();
        probes.insert(MaildirFsPath::from("root/new/abc"), true);
        probes.insert(MaildirFsPath::from("root/tmp/abc"), false);
        let pairs = expect_wants_rename(&mut cor, Some(MaildirReply::FileExists(probes)));
        let (from, to) = &pairs[0];
        assert_eq!(from, &MaildirFsPath::from("root/new/abc"));
        assert_eq!(to, &MaildirFsPath::from("root/cur/abc:2,S"));

        expect_complete_ok(&mut cor, Some(MaildirReply::Rename));
    }

    #[test]
    fn new_subdir_without_flags_returns_noop_ok() {
        let mut cor = MaildirFlagsSet::new(maildir(), "abc", MaildirFlags::default());

        expect_wants_file_exists(&mut cor);

        let mut probes = BTreeMap::new();
        probes.insert(MaildirFsPath::from("root/new/abc"), true);
        probes.insert(MaildirFsPath::from("root/tmp/abc"), false);
        expect_complete_ok(&mut cor, Some(MaildirReply::FileExists(probes)));
    }

    #[test]
    fn flagged_new_entry_set_empty_clears_suffix_and_stays_new() {
        let mut cor = MaildirFlagsSet::new(maildir(), "abc", MaildirFlags::default());
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
            vec![("root/new/abc:2,F".into(), "root/new/abc".into())]
        );
    }

    #[test]
    fn full_basename_uses_logical_id_once() {
        let mut cor = MaildirFlagsSet::new(maildir(), "abc:2,F", seen());
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
        assert_eq!(pairs[0].1, MaildirFsPath::from("root/cur/abc:2,S"));
    }

    #[test]
    fn tmp_subdir_returns_noop_ok() {
        let mut cor = MaildirFlagsSet::new(maildir(), "abc", seen());

        expect_wants_file_exists(&mut cor);

        let mut probes = BTreeMap::new();
        probes.insert(MaildirFsPath::from("root/new/abc"), false);
        probes.insert(MaildirFsPath::from("root/tmp/abc"), true);
        expect_complete_ok(&mut cor, Some(MaildirReply::FileExists(probes)));
    }

    #[test]
    fn unexpected_reply_returns_error() {
        let mut cor = MaildirFlagsSet::new(maildir(), "abc", MaildirFlags::default());
        expect_wants_file_exists(&mut cor);

        let err = expect_complete_err(&mut cor, Some(MaildirReply::DirCreate));
        assert!(matches!(err, MaildirFlagsSetError::Locate(_)));
    }

    fn expect_wants_file_exists(cor: &mut MaildirFlagsSet) {
        match cor.resume(None) {
            MaildirCoroutineState::Yielded(MaildirYield::WantsFileExists(_)) => {}
            state => panic!("expected WantsFileExists, got {state:?}"),
        }
    }

    fn expect_wants_rename(
        cor: &mut MaildirFlagsSet,
        arg: Option<MaildirReply>,
    ) -> Vec<(MaildirFsPath, MaildirFsPath)> {
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

    fn expect_complete_ok(cor: &mut MaildirFlagsSet, arg: Option<MaildirReply>) {
        match cor.resume(arg) {
            MaildirCoroutineState::Complete(Ok(())) => {}
            state => panic!("expected Complete(Ok), got {state:?}"),
        }
    }

    fn expect_complete_err(
        cor: &mut MaildirFlagsSet,
        arg: Option<MaildirReply>,
    ) -> MaildirFlagsSetError {
        match cor.resume(arg) {
            MaildirCoroutineState::Complete(Err(err)) => err,
            state => panic!("expected Complete(Err), got {state:?}"),
        }
    }
}
