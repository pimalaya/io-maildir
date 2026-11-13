//! I/O-free coroutine copying a Maildir entry to another Maildir.
//!
//! Copying is a fresh delivery into `target`: a brand-new Maildir
//! unique name is minted (time / pid / hostname, like
//! [`MaildirEntryStore`]) instead of reusing the source basename.
//! Reusing the source name would carry folder-specific metadata baked
//! into it by other tools, e.g. mbsync's `,U=<uid>` infix valid only in
//! the source folder, into the destination, corrupting its sync state
//! and risking a silent overwrite of a same-named entry. The source
//! flags are preserved.
//!
//! It is a delivery in the other sense too: the bytes are copied into
//! the target `/tmp` and renamed into place, as [`MaildirEntryStore`]
//! writes them. Nothing is ever enumerated under its final name before
//! every byte is behind it, so a process dying mid-copy leaves at worst
//! a stray file in `/tmp` rather than a truncated message in `/cur`.
//!
//! [`MaildirEntryStore`]: crate::entry::store::MaildirEntryStore
//!
//! # Example
//!
//! ```rust,no_run
//! use io_maildir::{client::MaildirClient, entry::copy::MaildirEntryCopy};
//!
//! let client = MaildirClient::new("/path/to/root");
//! let source = client.load_maildir("inbox").unwrap();
//! let target = client.load_maildir("archive").unwrap();
//!
//! let coroutine = MaildirEntryCopy::new("1700000000.1.M0P1.host", source, target, None);
//! client.run(coroutine).unwrap();
//! ```

use core::{fmt, mem};

use alloc::{
    collections::BTreeSet,
    string::{String, ToString},
};

use log::debug;
use thiserror::Error;

use crate::{
    coroutine::*,
    entry::{INFORMATIONAL_SUFFIX_SEPARATOR, locate::*, mint_id},
    flag::MaildirFlags,
    maildir::{Maildir, MaildirSubdir},
    maildir_try,
    path::MaildirFsPath,
};

/// Failure causes during a [`MaildirEntryCopy`] step.
#[derive(Clone, Debug, Error)]
pub enum MaildirEntryCopyError {
    /// A reply arrived that does not match the awaited step.
    #[error("Maildir message copy failed: unexpected arg {0:?}")]
    UnexpectedArg(Option<MaildirReply>),
    /// The inner locate step failed.
    #[error(transparent)]
    Locate(#[from] MaildirEntryLocateError),
}

/// Copies a Maildir entry into `target`, returning the id the copy
/// got; `None` target_subdir keeps the source subdir.
#[derive(Debug)]
pub struct MaildirEntryCopy {
    target: Maildir,
    target_subdir: Option<MaildirSubdir>,
    state: State,
}

impl MaildirEntryCopy {
    /// Builds a coroutine copying the Maildir entry into the target
    /// Maildir.
    pub fn new(
        id: impl ToString,
        source: Maildir,
        target: Maildir,
        target_subdir: Option<MaildirSubdir>,
    ) -> Self {
        Self {
            state: State::Locate(MaildirEntryLocate::new(source, id)),
            target,
            target_subdir,
        }
    }
}

impl MaildirCoroutine for MaildirEntryCopy {
    type Yield = MaildirYield;
    type Return = Result<String, MaildirEntryCopyError>;

    fn resume(
        &mut self,
        arg: Option<MaildirReply>,
    ) -> MaildirCoroutineState<Self::Yield, Self::Return> {
        match (&mut self.state, arg) {
            (State::Locate(c), arg) => {
                let out = maildir_try!(c, arg);
                let subdir = self.target_subdir.clone().unwrap_or(out.subdir);
                self.state = State::ReadTime {
                    source: out.path,
                    subdir,
                    flags: out.flags,
                };
                MaildirCoroutineState::Yielded(MaildirYield::WantsTime)
            }
            (
                State::ReadTime {
                    source,
                    subdir,
                    flags,
                },
                Some(MaildirReply::Time { secs, nanos }),
            ) => {
                self.state = State::ReadPid {
                    source: mem::take(source),
                    subdir: subdir.clone(),
                    flags: mem::take(flags),
                    secs,
                    nanos,
                };
                MaildirCoroutineState::Yielded(MaildirYield::WantsPid)
            }
            (
                State::ReadPid {
                    source,
                    subdir,
                    flags,
                    secs,
                    nanos,
                },
                Some(MaildirReply::Pid(pid)),
            ) => {
                self.state = State::ReadHostname {
                    source: mem::take(source),
                    subdir: subdir.clone(),
                    flags: mem::take(flags),
                    secs: *secs,
                    nanos: *nanos,
                    pid,
                };
                MaildirCoroutineState::Yielded(MaildirYield::WantsHostname)
            }
            (
                State::ReadHostname {
                    source,
                    subdir,
                    flags,
                    secs,
                    nanos,
                    pid,
                },
                Some(MaildirReply::Hostname(hostname)),
            ) => {
                let id = mint_id(*secs, *nanos, *pid, &hostname);
                let tmp_path = self.target.tmp().join(&id);
                let final_path = build_target_path(&self.target, subdir, &id, flags);
                let pairs = vec![(mem::take(source), tmp_path.clone())];
                self.state = State::Copy {
                    id,
                    tmp_path,
                    final_path,
                };
                MaildirCoroutineState::Yielded(MaildirYield::WantsCopy(pairs))
            }
            (
                State::Copy {
                    id,
                    tmp_path,
                    final_path,
                },
                Some(MaildirReply::Copy),
            ) => {
                let paths = BTreeSet::from([tmp_path.clone()]);
                self.state = State::SyncTmp {
                    id: mem::take(id),
                    tmp_path: mem::take(tmp_path),
                    final_path: mem::take(final_path),
                };
                MaildirCoroutineState::Yielded(MaildirYield::WantsFileSync(paths))
            }
            (
                State::SyncTmp {
                    id,
                    tmp_path,
                    final_path,
                },
                Some(MaildirReply::FileSync),
            ) => {
                // NOTE: a /tmp target renames onto itself, which POSIX
                // defines as a successful no-op, as in MaildirEntryStore.
                let dir = MaildirFsPath::from(final_path.parent().unwrap_or_default());
                let pairs = vec![(mem::take(tmp_path), mem::take(final_path))];
                self.state = State::Rename {
                    id: mem::take(id),
                    dir,
                };
                MaildirCoroutineState::Yielded(MaildirYield::WantsRename(pairs))
            }
            (State::Rename { id, dir }, Some(MaildirReply::Rename)) => {
                let paths = BTreeSet::from([mem::take(dir)]);
                self.state = State::SyncDir { id: mem::take(id) };
                MaildirCoroutineState::Yielded(MaildirYield::WantsDirSync(paths))
            }
            (State::SyncDir { id }, Some(MaildirReply::DirSync)) => {
                debug!("copied entry as {id}");
                MaildirCoroutineState::Complete(Ok(mem::take(id)))
            }
            (_, arg) => {
                let err = MaildirEntryCopyError::UnexpectedArg(arg);
                MaildirCoroutineState::Complete(Err(err))
            }
        }
    }
}

#[derive(Debug)]
enum State {
    Locate(MaildirEntryLocate),
    ReadTime {
        source: MaildirFsPath,
        subdir: MaildirSubdir,
        flags: MaildirFlags,
    },
    ReadPid {
        source: MaildirFsPath,
        subdir: MaildirSubdir,
        flags: MaildirFlags,
        secs: u64,
        nanos: u32,
    },
    ReadHostname {
        source: MaildirFsPath,
        subdir: MaildirSubdir,
        flags: MaildirFlags,
        secs: u64,
        nanos: u32,
        pid: u32,
    },
    Copy {
        id: String,
        tmp_path: MaildirFsPath,
        final_path: MaildirFsPath,
    },
    SyncTmp {
        id: String,
        tmp_path: MaildirFsPath,
        final_path: MaildirFsPath,
    },
    Rename {
        id: String,
        dir: MaildirFsPath,
    },
    SyncDir {
        id: String,
    },
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Locate(_) => f.write_str("locate source"),
            Self::ReadTime { .. } => f.write_str("read time"),
            Self::ReadPid { .. } => f.write_str("read pid"),
            Self::ReadHostname { .. } => f.write_str("read hostname"),
            Self::Copy { .. } => f.write_str("copy into tmp"),
            Self::SyncTmp { .. } => f.write_str("sync tmp entry"),
            Self::Rename { .. } => f.write_str("rename into place"),
            Self::SyncDir { .. } => f.write_str("sync subdirectory"),
        }
    }
}

fn build_target_path(
    target: &Maildir,
    subdir: &MaildirSubdir,
    id: &str,
    flags: &MaildirFlags,
) -> MaildirFsPath {
    match subdir {
        MaildirSubdir::Cur => {
            let name = format!("{id}{INFORMATIONAL_SUFFIX_SEPARATOR}2,{flags}");
            target.cur().join(&name)
        }
        MaildirSubdir::New => target.new().join(id),
        MaildirSubdir::Tmp => target.tmp().join(id),
    }
}

#[cfg(test)]
mod tests {
    use alloc::{
        collections::{BTreeMap, BTreeSet},
        string::String,
    };

    use crate::entry::copy::*;

    fn source() -> Maildir {
        Maildir::from_path("root/src")
    }

    fn target() -> Maildir {
        Maildir::from_path("root/dst")
    }

    #[test]
    fn unexpected_reply_returns_error() {
        let mut cor = MaildirEntryCopy::new("abc", source(), target(), None);
        expect_wants_file_exists(&mut cor);

        let err = expect_complete_err(&mut cor, Some(MaildirReply::DirCreate));
        assert!(matches!(err, MaildirEntryCopyError::Locate(_)));
    }

    #[test]
    fn cur_copy_stages_in_tmp_mints_fresh_id_and_preserves_flags() {
        // Source carries mbsync's `,U=999` infix and `FS` flags.
        let mut cor = MaildirEntryCopy::new("1700000000.abc.host,U=999", source(), target(), None);

        // Locate: probe new/tmp, miss, scan cur, find the entry.
        expect_wants_file_exists(&mut cor);
        let mut probe = BTreeMap::new();
        probe.insert(
            MaildirFsPath::from("root/src/new/1700000000.abc.host,U=999"),
            false,
        );
        probe.insert(
            MaildirFsPath::from("root/src/tmp/1700000000.abc.host,U=999"),
            false,
        );
        match cor.resume(Some(MaildirReply::FileExists(probe))) {
            MaildirCoroutineState::Yielded(MaildirYield::WantsDirRead(_)) => {}
            state => panic!("expected WantsDirRead, got {state:?}"),
        }
        let mut entries = BTreeMap::new();
        let mut set = BTreeSet::new();
        set.insert(MaildirFsPath::from(
            "root/src/cur/1700000000.abc.host,U=999:2,FS",
        ));
        entries.insert(MaildirFsPath::from("root/src/cur"), set);
        // Locate completes; the first delivery step asks for time.
        match cor.resume(Some(MaildirReply::DirRead(entries))) {
            MaildirCoroutineState::Yielded(MaildirYield::WantsFileExists(paths)) => {
                assert_eq!(paths.len(), 1);
                let mut exists = BTreeMap::new();
                exists.insert(paths.iter().next().unwrap().clone(), true);
                match cor.resume(Some(MaildirReply::FileExists(exists))) {
                    MaildirCoroutineState::Yielded(MaildirYield::WantsTime) => {}
                    state => panic!("expected WantsTime, got {state:?}"),
                }
            }
            state => panic!("expected WantsTime, got {state:?}"),
        }
        match cor.resume(Some(MaildirReply::Time { secs: 1, nanos: 2 })) {
            MaildirCoroutineState::Yielded(MaildirYield::WantsPid) => {}
            state => panic!("expected WantsPid, got {state:?}"),
        }
        match cor.resume(Some(MaildirReply::Pid(3))) {
            MaildirCoroutineState::Yielded(MaildirYield::WantsHostname) => {}
            state => panic!("expected WantsHostname, got {state:?}"),
        }
        let staged = match cor.resume(Some(MaildirReply::Hostname(String::from("host")))) {
            MaildirCoroutineState::Yielded(MaildirYield::WantsCopy(pairs)) => {
                let (from, to) = &pairs[0];
                assert_eq!(
                    from,
                    &MaildirFsPath::from("root/src/cur/1700000000.abc.host,U=999:2,FS")
                );
                // The bytes land in the target /tmp, never under the name
                // the destination is enumerated by.
                assert!(to.as_str().starts_with("root/dst/tmp/1."), "got {to}");
                to.clone()
            }
            state => panic!("expected WantsCopy, got {state:?}"),
        };
        match cor.resume(Some(MaildirReply::Copy)) {
            MaildirCoroutineState::Yielded(MaildirYield::WantsFileSync(paths)) => {
                assert_eq!(paths, BTreeSet::from([staged.clone()]));
            }
            state => panic!("expected WantsFileSync, got {state:?}"),
        }
        match cor.resume(Some(MaildirReply::FileSync)) {
            MaildirCoroutineState::Yielded(MaildirYield::WantsRename(pairs)) => {
                let (from, to) = &pairs[0];
                assert_eq!(from, &staged);
                let to = to.as_str();
                // Fresh id under target/cur, no `,U=999`, flags preserved.
                assert!(to.starts_with("root/dst/cur/1."), "got {to}");
                assert!(!to.contains(",U=999"), "carried foreign UID: {to}");
                // Flags preserved; rendered in canonical (sorted) order.
                assert!(to.ends_with(":2,FS"), "flags not preserved: {to}");
            }
            state => panic!("expected WantsRename, got {state:?}"),
        }
        match cor.resume(Some(MaildirReply::Rename)) {
            MaildirCoroutineState::Yielded(MaildirYield::WantsDirSync(paths)) => {
                assert_eq!(paths, BTreeSet::from([MaildirFsPath::from("root/dst/cur")]));
            }
            state => panic!("expected WantsDirSync, got {state:?}"),
        }
        match cor.resume(Some(MaildirReply::DirSync)) {
            MaildirCoroutineState::Complete(Ok(id)) => assert!(id.starts_with("1."), "got {id}"),
            state => panic!("expected Complete(Ok), got {state:?}"),
        }
    }

    fn expect_wants_file_exists(cor: &mut MaildirEntryCopy) {
        match cor.resume(None) {
            MaildirCoroutineState::Yielded(MaildirYield::WantsFileExists(_)) => {}
            state => panic!("expected WantsFileExists, got {state:?}"),
        }
    }

    fn expect_complete_err(
        cor: &mut MaildirEntryCopy,
        arg: Option<MaildirReply>,
    ) -> MaildirEntryCopyError {
        match cor.resume(arg) {
            MaildirCoroutineState::Complete(Err(err)) => err,
            state => panic!("expected Complete(Err), got {state:?}"),
        }
    }
}
