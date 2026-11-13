//! I/O-free coroutine adding flags to a Maildir entry.
//!
//! # Example
//!
//! ```rust,no_run
//! use io_maildir::{
//!     client::MaildirClient,
//!     flag::{add::MaildirFlagsAdd, MaildirFlag, MaildirFlags},
//! };
//!
//! let client = MaildirClient::new("/path/to/root");
//! let maildir = client.load_maildir("inbox").unwrap();
//!
//! let flags = MaildirFlags::from_iter([MaildirFlag::Seen]);
//! let coroutine = MaildirFlagsAdd::new(maildir, "1700000000.1.M0P1.host", flags);
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

/// Failure causes during a [`MaildirFlagsAdd`] step.
#[derive(Clone, Debug, Error)]
pub enum MaildirFlagsAddError {
    /// A reply arrived that does not match the awaited step.
    #[error("Maildir flags add failed: unexpected arg {0:?}")]
    UnexpectedArg(Option<MaildirReply>),
    /// The inner locate step failed.
    #[error(transparent)]
    Locate(#[from] MaildirEntryLocateError),
    /// The entry carries an info section other than `2,<flags>`, such
    /// as an experimental `:1,` one, which a flag write would lose.
    #[error("Maildir flags add failed: entry {0} carries a non-flag info section")]
    ForeignInfo(MaildirFsPath),
}

/// Adds flags to a Maildir entry.
///
/// Entries in `/cur` are renamed with the merged flag set. A `/new`
/// entry moves to `/cur` when its flags change to a nonempty set, and
/// stays in `/new` with its plain logical ID when its flags are cleared.
/// An unchanged entry and an entry in `/tmp` are left untouched.
#[derive(Debug)]
pub struct MaildirFlagsAdd {
    maildir: Maildir,
    id: String,
    flags: MaildirFlags,
    state: State,
}

impl MaildirFlagsAdd {
    /// Builds a coroutine adding flags to a Maildir entry.
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

impl MaildirCoroutine for MaildirFlagsAdd {
    type Yield = MaildirYield;
    type Return = Result<(), MaildirFlagsAddError>;

    fn resume(
        &mut self,
        arg: Option<MaildirReply>,
    ) -> MaildirCoroutineState<Self::Yield, Self::Return> {
        match (&mut self.state, arg) {
            (State::Locate(c), arg) => {
                let out = maildir_try!(c, arg);

                if out.path.file_name().is_some_and(has_foreign_info) {
                    let err = MaildirFlagsAddError::ForeignInfo(out.path);
                    return MaildirCoroutineState::Complete(Err(err));
                }

                match out.subdir.clone() {
                    MaildirSubdir::Tmp => {
                        debug!("added flags");
                        MaildirCoroutineState::Complete(Ok(()))
                    }
                    MaildirSubdir::Cur | MaildirSubdir::New => {
                        let original_flags = out.flags;
                        let mut flags = original_flags.clone();
                        flags.extend(self.flags.clone());
                        if out.subdir == MaildirSubdir::New && flags == original_flags {
                            debug!("added flags");
                            return MaildirCoroutineState::Complete(Ok(()));
                        }
                        let id = crate::entry::MaildirEntry::from_path(out.path.clone())
                            .id()
                            .unwrap_or(&self.id)
                            .to_string();
                        let new_path = if out.subdir == MaildirSubdir::New && flags.is_empty() {
                            self.maildir.new().join(&id)
                        } else {
                            cur_path_with_flags(&self.maildir, &id, &flags)
                        };
                        let pairs = vec![(out.path, new_path)];
                        self.state = State::Rename;
                        MaildirCoroutineState::Yielded(MaildirYield::WantsRename(pairs))
                    }
                }
            }
            (State::Rename, Some(MaildirReply::Rename)) => {
                debug!("added flags");
                MaildirCoroutineState::Complete(Ok(()))
            }
            (_, arg) => {
                let err = MaildirFlagsAddError::UnexpectedArg(arg);
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

    use crate::flag::{MaildirFlag, add::*};

    fn maildir() -> Maildir {
        Maildir::from_path("root")
    }

    fn seen() -> MaildirFlags {
        MaildirFlags::from_iter([MaildirFlag::Seen])
    }

    #[test]
    fn cur_subdir_renames_with_merged_flags() {
        let mut cor = MaildirFlagsAdd::new(maildir(), "abc", seen());

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
        let pairs = expect_wants_rename(&mut cor, Some(MaildirReply::files(entries)));
        let (from, to) = &pairs[0];
        assert_eq!(from, &MaildirFsPath::from("root/cur/abc:2,F"));
        assert_eq!(to, &MaildirFsPath::from("root/cur/abc:2,FS"));

        expect_complete_ok(&mut cor, Some(MaildirReply::Rename));
    }

    #[test]
    fn new_subdir_renames_into_cur() {
        let mut cor = MaildirFlagsAdd::new(maildir(), "abc", seen());

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
        let mut cor = MaildirFlagsAdd::new(maildir(), "abc", MaildirFlags::default());

        expect_wants_file_exists(&mut cor);

        let mut probes = BTreeMap::new();
        probes.insert(MaildirFsPath::from("root/new/abc"), true);
        probes.insert(MaildirFsPath::from("root/tmp/abc"), false);
        expect_complete_ok(&mut cor, Some(MaildirReply::FileExists(probes)));
    }

    #[test]
    fn flagged_new_entry_adds_flags_and_moves_to_cur() {
        let mut cor = MaildirFlagsAdd::new(maildir(), "abc", seen());
        expect_wants_file_exists(&mut cor);

        let mut probes = BTreeMap::new();
        probes.insert(MaildirFsPath::from("root/new/abc"), false);
        probes.insert(MaildirFsPath::from("root/tmp/abc"), false);
        match cor.resume(Some(MaildirReply::FileExists(probes))) {
            MaildirCoroutineState::Yielded(MaildirYield::WantsDirRead(_)) => {}
            state => panic!("expected WantsDirRead, got {state:?}"),
        }

        let mut entries = BTreeMap::new();
        entries.insert(
            MaildirFsPath::from("root/new"),
            BTreeSet::from([MaildirFsPath::from("root/new/abc:2,F")]),
        );
        entries.insert(MaildirFsPath::from("root/cur"), BTreeSet::new());
        let pairs = expect_wants_rename(&mut cor, Some(MaildirReply::files(entries)));
        assert_eq!(
            pairs,
            vec![("root/new/abc:2,F".into(), "root/cur/abc:2,FS".into())]
        );
    }

    #[test]
    fn adding_an_existing_new_flag_leaves_the_entry_untouched() {
        let mut cor = MaildirFlagsAdd::new(
            maildir(),
            "abc",
            MaildirFlags::from_iter([MaildirFlag::Flagged]),
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
        expect_complete_ok(&mut cor, Some(MaildirReply::files(entries)));
    }

    #[test]
    fn full_basename_uses_logical_id_once() {
        let mut cor = MaildirFlagsAdd::new(maildir(), "abc:2,F", seen());
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
        let pairs = expect_wants_rename(&mut cor, Some(MaildirReply::files(entries)));
        assert_eq!(pairs[0].1, MaildirFsPath::from("root/cur/abc:2,FS"));
    }

    #[test]
    fn tmp_subdir_returns_noop_ok() {
        let mut cor = MaildirFlagsAdd::new(maildir(), "abc", seen());

        expect_wants_file_exists(&mut cor);

        let mut probes = BTreeMap::new();
        probes.insert(MaildirFsPath::from("root/new/abc"), false);
        probes.insert(MaildirFsPath::from("root/tmp/abc"), true);
        expect_complete_ok(&mut cor, Some(MaildirReply::FileExists(probes)));
    }

    #[test]
    fn an_experimental_info_section_is_refused_not_rewritten() {
        let mut cor = MaildirFlagsAdd::new(maildir(), "abc", seen());

        expect_wants_file_exists(&mut cor);

        let mut probes = BTreeMap::new();
        probes.insert(MaildirFsPath::from("root/new/abc"), false);
        probes.insert(MaildirFsPath::from("root/tmp/abc"), false);
        match cor.resume(Some(MaildirReply::FileExists(probes))) {
            MaildirCoroutineState::Yielded(MaildirYield::WantsDirRead(_)) => {}
            state => panic!("expected WantsDirRead, got {state:?}"),
        }

        let mut entries = BTreeMap::new();
        let set = BTreeSet::from_iter([MaildirFsPath::from("root/cur/abc:1,experimental")]);
        entries.insert(MaildirFsPath::from("root/cur"), set);
        let err = expect_complete_err(&mut cor, Some(MaildirReply::files(entries)));
        assert!(matches!(err, MaildirFlagsAddError::ForeignInfo(_)), "{err}");
    }

    #[test]
    fn unexpected_reply_returns_error() {
        let mut cor = MaildirFlagsAdd::new(maildir(), "abc", MaildirFlags::default());
        expect_wants_file_exists(&mut cor);

        let err = expect_complete_err(&mut cor, Some(MaildirReply::DirCreate));
        assert!(matches!(err, MaildirFlagsAddError::Locate(_)));
    }

    fn expect_wants_file_exists(cor: &mut MaildirFlagsAdd) {
        match cor.resume(None) {
            MaildirCoroutineState::Yielded(MaildirYield::WantsFileExists(_)) => {}
            state => panic!("expected WantsFileExists, got {state:?}"),
        }
    }

    fn expect_wants_rename(
        cor: &mut MaildirFlagsAdd,
        arg: Option<MaildirReply>,
    ) -> Vec<(MaildirFsPath, MaildirFsPath)> {
        match cor.resume(arg) {
            MaildirCoroutineState::Yielded(MaildirYield::WantsRename(pairs)) => pairs,
            state => panic!("expected WantsRename, got {state:?}"),
        }
    }

    fn expect_complete_ok(cor: &mut MaildirFlagsAdd, arg: Option<MaildirReply>) {
        match cor.resume(arg) {
            MaildirCoroutineState::Complete(Ok(())) => {}
            state => panic!("expected Complete(Ok), got {state:?}"),
        }
    }

    fn expect_complete_err(
        cor: &mut MaildirFlagsAdd,
        arg: Option<MaildirReply>,
    ) -> MaildirFlagsAddError {
        match cor.resume(arg) {
            MaildirCoroutineState::Complete(Err(err)) => err,
            state => panic!("expected Complete(Err), got {state:?}"),
        }
    }
}
