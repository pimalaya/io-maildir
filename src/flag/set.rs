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
    flag::MaildirFlags,
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
}

/// Replaces the flags of a Maildir entry.
///
/// An entry in `/cur` is renamed with the new flag set. One in `/new`
/// moves to `/cur` as it gains its first flag, since a name in `/new`
/// carries no info suffix to hold one. Setting no flag leaves it where
/// it is, and an entry in `/tmp` is never touched, being another
/// process's delivery in flight.
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

                match out.subdir {
                    MaildirSubdir::Tmp => {
                        debug!("set flags");
                        MaildirCoroutineState::Complete(Ok(()))
                    }
                    MaildirSubdir::New if self.flags.is_empty() => {
                        debug!("set flags");
                        MaildirCoroutineState::Complete(Ok(()))
                    }
                    MaildirSubdir::Cur | MaildirSubdir::New => {
                        let new_path = cur_path_with_flags(&self.maildir, &self.id, &self.flags);
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
        match cor.resume(arg) {
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
