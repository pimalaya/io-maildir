//! I/O-free coroutine moving a Maildir entry to another Maildir.
//!
//! Moving relocates the entry into `target` under a freshly minted
//! Maildir unique name (time / pid / hostname, like
//! [`MaildirEntryStore`]) instead of reusing the source basename.
//! Reusing the source name would carry folder-specific metadata baked
//! into it by other tools, e.g. mbsync's `,U=<uid>` infix valid only in
//! the source folder, into the destination, corrupting its sync state
//! and risking a silent overwrite of a same-named entry. The source
//! flags are preserved.
//!
//! [`MaildirEntryStore`]: crate::entry::store::MaildirEntryStore
//!
//! # Example
//!
//! ```rust,no_run
//! use io_maildir::{client::MaildirClient, entry::r#move::MaildirEntryMove};
//!
//! let client = MaildirClient::new("/path/to/root");
//! let source = client.load_maildir("inbox").unwrap();
//! let target = client.load_maildir("archive").unwrap();
//!
//! let coroutine = MaildirEntryMove::new("1700000000.1.M0P1.host", source, target, None);
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
    entry::{locate::*, mint_id},
    flag::MaildirFlags,
    maildir::{Maildir, MaildirSubdir},
    maildir_try,
    path::MaildirFsPath,
};

/// Failure causes during a [`MaildirEntryMove`] step.
#[derive(Clone, Debug, Error)]
pub enum MaildirEntryMoveError {
    /// A reply arrived that does not match the awaited step.
    #[error("Maildir message move failed: unexpected arg {0:?}")]
    UnexpectedArg(Option<MaildirReply>),
    /// The inner locate step failed.
    #[error(transparent)]
    Locate(#[from] MaildirEntryLocateError),
}

/// Moves a Maildir entry into `target`; `None` target_subdir keeps
/// the source subdir.
#[derive(Debug)]
pub struct MaildirEntryMove {
    target: Maildir,
    target_subdir: Option<MaildirSubdir>,
    state: State,
}

impl MaildirEntryMove {
    /// Builds a coroutine moving the Maildir entry into the target
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

impl MaildirCoroutine for MaildirEntryMove {
    type Yield = MaildirYield;
    type Return = Result<String, MaildirEntryMoveError>;

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
                self.state = State::ReadCounter {
                    source: mem::take(source),
                    subdir: subdir.clone(),
                    flags: mem::take(flags),
                    secs: *secs,
                    nanos: *nanos,
                    pid,
                };
                MaildirCoroutineState::Yielded(MaildirYield::WantsCounter)
            }
            (
                State::ReadCounter {
                    source,
                    subdir,
                    flags,
                    secs,
                    nanos,
                    pid,
                },
                Some(MaildirReply::Counter(counter)),
            ) => {
                self.state = State::ReadHostname {
                    source: mem::take(source),
                    subdir: subdir.clone(),
                    flags: mem::take(flags),
                    secs: *secs,
                    nanos: *nanos,
                    pid: *pid,
                    counter,
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
                    counter,
                },
                Some(MaildirReply::Hostname(hostname)),
            ) => {
                let separator = self.target.separator;
                let id = mint_id(*secs, *nanos, *pid, *counter, &hostname, separator);
                let target = build_target_path(&self.target, subdir, &id, flags);
                let dirs = [&*source, &target].map(|path| path.parent().unwrap_or_default());
                let dirs = dirs.into_iter().map(MaildirFsPath::from).collect();
                let pairs = vec![(mem::take(source), target)];
                self.state = State::Rename { id, dirs };
                MaildirCoroutineState::Yielded(MaildirYield::WantsRename(pairs))
            }
            (State::Rename { id, dirs }, Some(MaildirReply::Rename)) => {
                let paths = mem::take(dirs);
                self.state = State::SyncDirs { id: mem::take(id) };
                MaildirCoroutineState::Yielded(MaildirYield::WantsDirSync(paths))
            }
            (State::SyncDirs { id }, Some(MaildirReply::DirSync)) => {
                debug!("moved entry");
                MaildirCoroutineState::Complete(Ok(mem::take(id)))
            }
            (_, arg) => {
                let err = MaildirEntryMoveError::UnexpectedArg(arg);
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
    ReadCounter {
        source: MaildirFsPath,
        subdir: MaildirSubdir,
        flags: MaildirFlags,
        secs: u64,
        nanos: u32,
        pid: u32,
    },
    ReadHostname {
        source: MaildirFsPath,
        subdir: MaildirSubdir,
        flags: MaildirFlags,
        secs: u64,
        nanos: u32,
        pid: u32,
        counter: u32,
    },
    Rename {
        id: String,
        dirs: BTreeSet<MaildirFsPath>,
    },
    SyncDirs {
        id: String,
    },
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Locate(_) => f.write_str("locate source"),
            Self::ReadTime { .. } => f.write_str("read time"),
            Self::ReadPid { .. } => f.write_str("read pid"),
            Self::ReadCounter { .. } => f.write_str("read counter"),
            Self::ReadHostname { .. } => f.write_str("read hostname"),
            Self::Rename { .. } => f.write_str("rename into place"),
            Self::SyncDirs { .. } => f.write_str("sync subdirectories"),
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
            let name = format!("{id}{}2,{flags}", target.separator);
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

    use crate::entry::r#move::*;

    fn source() -> Maildir {
        Maildir::from_path("root/src")
    }

    fn target() -> Maildir {
        Maildir::from_path("root/dst")
    }

    #[test]
    fn unexpected_reply_returns_error() {
        let mut cor = MaildirEntryMove::new("abc", source(), target(), None);
        expect_wants_file_exists(&mut cor);

        let err = expect_complete_err(&mut cor, Some(MaildirReply::DirCreate));
        assert!(matches!(err, MaildirEntryMoveError::Locate(_)));
    }

    #[test]
    fn cur_move_mints_fresh_id_and_preserves_flags() {
        // Source carries mbsync's `,U=999` infix and `FS` flags.
        let mut cor = MaildirEntryMove::new("1700000000.abc.host,U=999", source(), target(), None);

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
        match cor.resume(Some(MaildirReply::files(entries))) {
            MaildirCoroutineState::Yielded(MaildirYield::WantsTime) => {}
            state => panic!("expected WantsTime, got {state:?}"),
        }
        match cor.resume(Some(MaildirReply::Time { secs: 1, nanos: 2 })) {
            MaildirCoroutineState::Yielded(MaildirYield::WantsPid) => {}
            state => panic!("expected WantsPid, got {state:?}"),
        }
        match cor.resume(Some(MaildirReply::Pid(3))) {
            MaildirCoroutineState::Yielded(MaildirYield::WantsCounter) => {}
            state => panic!("expected WantsCounter, got {state:?}"),
        }
        match cor.resume(Some(MaildirReply::Counter(4))) {
            MaildirCoroutineState::Yielded(MaildirYield::WantsHostname) => {}
            state => panic!("expected WantsHostname, got {state:?}"),
        }
        match cor.resume(Some(MaildirReply::Hostname(String::from("host")))) {
            MaildirCoroutineState::Yielded(MaildirYield::WantsRename(pairs)) => {
                let (from, to) = &pairs[0];
                assert_eq!(
                    from,
                    &MaildirFsPath::from("root/src/cur/1700000000.abc.host,U=999:2,FS")
                );
                let to = to.as_str();
                assert!(to.starts_with("root/dst/cur/1."), "got {to}");
                assert!(!to.contains(",U=999"), "carried foreign UID: {to}");
                // Flags preserved; rendered in canonical (sorted) order.
                assert!(to.ends_with(":2,FS"), "flags not preserved: {to}");
            }
            state => panic!("expected WantsRename, got {state:?}"),
        }
        match cor.resume(Some(MaildirReply::Rename)) {
            MaildirCoroutineState::Yielded(MaildirYield::WantsDirSync(paths)) => {
                let dirs = ["root/dst/cur", "root/src/cur"].map(MaildirFsPath::from);
                assert_eq!(paths, BTreeSet::from(dirs));
            }
            state => panic!("expected WantsDirSync, got {state:?}"),
        }
        match cor.resume(Some(MaildirReply::DirSync)) {
            MaildirCoroutineState::Complete(Ok(id)) => assert!(id.starts_with("1."), "got {id}"),
            state => panic!("expected Complete(Ok), got {state:?}"),
        }
    }

    fn expect_wants_file_exists(cor: &mut MaildirEntryMove) {
        match cor.resume(None) {
            MaildirCoroutineState::Yielded(MaildirYield::WantsFileExists(_)) => {}
            state => panic!("expected WantsFileExists, got {state:?}"),
        }
    }

    fn expect_complete_err(
        cor: &mut MaildirEntryMove,
        arg: Option<MaildirReply>,
    ) -> MaildirEntryMoveError {
        match cor.resume(arg) {
            MaildirCoroutineState::Complete(Err(err)) => err,
            state => panic!("expected Complete(Err), got {state:?}"),
        }
    }
}
