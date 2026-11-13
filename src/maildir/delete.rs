//! I/O-free coroutine deleting a Maildir and all its contents.
//!
//! # Example
//!
//! ```rust,no_run
//! use io_maildir::{
//!     client::MaildirClient,
//!     maildir::delete::MaildirDelete,
//!     path::MaildirPath,
//! };
//!
//! let client = MaildirClient::new("/path/to/root");
//!
//! let coroutine = MaildirDelete::new(&client.store, MaildirPath::from("inbox"));
//! client.run(coroutine).unwrap();
//! ```

use core::{fmt, mem};

use alloc::{
    collections::{BTreeMap, BTreeSet},
    format,
};

use log::debug;
use thiserror::Error;

use crate::{
    coroutine::*,
    path::{MaildirFsPath, MaildirPath},
    store::MaildirStore,
};

/// Failure causes during a [`MaildirDelete`] step.
#[derive(Clone, Debug, Error)]
pub enum MaildirDeleteError {
    /// A reply arrived that does not match the awaited step.
    #[error("Maildir delete failed: unexpected arg {0:?}")]
    UnexpectedArg(Option<MaildirReply>),
    /// The name resolves to the store root, outside it or onto a
    /// subdirectory.
    #[error("Maildir delete failed: `{0}` names no folder below the store root")]
    NotAFolder(MaildirPath),
}

/// Recursively removes a Maildir directory. On Maildir++ every
/// `.<name>.*` sibling holding its children goes too, as the fs removal
/// takes them along.
#[derive(Debug)]
pub struct MaildirDelete {
    state: State,
}

impl MaildirDelete {
    /// Builds a coroutine recursively removing the Maildir resolved
    /// from `name` under `store`.
    ///
    /// A name [`MaildirStore::names_folder`] rejects is refused on the
    /// first resume, nothing removed.
    pub fn new(store: &MaildirStore, name: MaildirPath) -> Self {
        if !store.names_folder(&name) {
            return Self {
                state: State::Refused(name),
            };
        }

        let path = store.resolve(&name);

        if store.maildirpp {
            let root = store.root.clone();
            return Self {
                state: State::Siblings { root, path },
            };
        }

        let paths = BTreeSet::from_iter([path]);
        Self {
            state: State::Start { paths },
        }
    }
}

impl MaildirCoroutine for MaildirDelete {
    type Yield = MaildirYield;
    type Return = Result<(), MaildirDeleteError>;

    fn resume(
        &mut self,
        arg: Option<MaildirReply>,
    ) -> MaildirCoroutineState<Self::Yield, Self::Return> {
        match (&mut self.state, arg) {
            (State::Refused(name), None) => {
                let err = MaildirDeleteError::NotAFolder(mem::take(name));
                MaildirCoroutineState::Complete(Err(err))
            }
            (State::Siblings { root, .. }, None) => {
                let paths = BTreeSet::from([mem::take(root)]);
                MaildirCoroutineState::Yielded(MaildirYield::WantsDirRead(paths))
            }
            (State::Siblings { path, .. }, Some(MaildirReply::DirRead(entries))) => {
                let path = mem::take(path);
                let prefix = format!("{path}.");
                let mut paths = BTreeSet::from([path]);
                let children = entries.into_values().flat_map(BTreeMap::into_keys);
                paths.extend(children.filter(|name| name.as_str().starts_with(&prefix)));
                self.state = State::Remove;
                MaildirCoroutineState::Yielded(MaildirYield::WantsDirRemove(paths))
            }
            (State::Start { paths }, None) => {
                let paths = mem::take(paths);
                self.state = State::Remove;
                MaildirCoroutineState::Yielded(MaildirYield::WantsDirRemove(paths))
            }
            (State::Remove, Some(MaildirReply::DirRemove)) => {
                debug!("deleted maildir");
                MaildirCoroutineState::Complete(Ok(()))
            }
            (_, arg) => {
                let err = MaildirDeleteError::UnexpectedArg(arg);
                MaildirCoroutineState::Complete(Err(err))
            }
        }
    }
}

#[derive(Debug)]
enum State {
    Refused(MaildirPath),
    Siblings {
        root: MaildirFsPath,
        path: MaildirFsPath,
    },
    Start {
        paths: BTreeSet<MaildirFsPath>,
    },
    Remove,
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused(_) => f.write_str("refuse"),
            Self::Siblings { .. } => f.write_str("read siblings"),
            Self::Start { .. } => f.write_str("start"),
            Self::Remove => f.write_str("remove maildir"),
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use crate::maildir::delete::*;

    fn fs_store() -> MaildirStore {
        MaildirStore {
            root: MaildirFsPath::from("root"),
            maildirpp: false,
        }
    }

    fn maildirpp_store() -> MaildirStore {
        MaildirStore {
            root: MaildirFsPath::from("root"),
            maildirpp: true,
        }
    }

    #[test]
    fn fs_removes_resolved_path() {
        let mut cor = MaildirDelete::new(&fs_store(), MaildirPath::from("inbox"));

        let paths = expect_wants_dir_remove(&mut cor);
        assert_eq!(paths.len(), 1);
        assert!(paths.contains(&MaildirFsPath::from("root/inbox")));

        expect_complete_ok(&mut cor, Some(MaildirReply::DirRemove));
    }

    #[test]
    fn maildirpp_removes_dotted_flat_path() {
        let mut cor = MaildirDelete::new(&maildirpp_store(), MaildirPath::from("Foo/Bar"));

        let names = [
            "root/.Foo.Bar",
            "root/.Foo.Bar.Kid",
            "root/.Foo.Barn",
            "root/.Foo",
        ];
        let paths = expect_remove_after_read(&mut cor, &names);
        let paths: Vec<&str> = paths.iter().map(|path| path.as_str()).collect();
        assert_eq!(paths, ["root/.Foo.Bar", "root/.Foo.Bar.Kid"]);
    }

    fn expect_remove_after_read(
        cor: &mut MaildirDelete,
        names: &[&str],
    ) -> BTreeSet<MaildirFsPath> {
        match cor.resume(None) {
            MaildirCoroutineState::Yielded(MaildirYield::WantsDirRead(paths)) => {
                assert_eq!(paths, BTreeSet::from([MaildirFsPath::from("root")]));
            }
            state => panic!("expected WantsDirRead, got {state:?}"),
        }

        let names = names
            .iter()
            .map(|name| (MaildirFsPath::from(*name), MaildirFileKind::Dir))
            .collect();
        let entries = BTreeMap::from([(MaildirFsPath::from("root"), names)]);
        expect_wants_dir_remove_after(cor, MaildirReply::DirRead(entries))
    }

    fn expect_wants_dir_remove_after(
        cor: &mut MaildirDelete,
        reply: MaildirReply,
    ) -> BTreeSet<MaildirFsPath> {
        match cor.resume(Some(reply)) {
            MaildirCoroutineState::Yielded(MaildirYield::WantsDirRemove(paths)) => paths,
            state => panic!("expected WantsDirRemove, got {state:?}"),
        }
    }

    #[test]
    fn the_root_and_paths_leaving_it_are_refused() {
        for name in ["", ".", "./", "..", "Foo/.."] {
            for store in [fs_store(), maildirpp_store()] {
                let mut cor = MaildirDelete::new(&store, MaildirPath::from(name));
                let err = expect_complete_err(&mut cor, None);
                assert!(matches!(err, MaildirDeleteError::NotAFolder(_)), "{name}");
            }
        }
    }

    #[test]
    fn the_subdirectories_of_a_maildir_are_refused_in_the_fs_layout() {
        for name in ["cur", "new", "tmp", "Work/new", "tmp/Work"] {
            let mut cor = MaildirDelete::new(&fs_store(), MaildirPath::from(name));
            let err = expect_complete_err(&mut cor, None);
            assert!(matches!(err, MaildirDeleteError::NotAFolder(_)), "{name}");

            let mut cor = MaildirDelete::new(&maildirpp_store(), MaildirPath::from(name));
            let state = cor.resume(None);
            assert!(matches!(state, MaildirCoroutineState::Yielded(_)), "{name}");
        }
    }

    #[test]
    fn unexpected_reply_returns_error() {
        let mut cor = MaildirDelete::new(&fs_store(), MaildirPath::from("inbox"));
        let _ = expect_wants_dir_remove(&mut cor);

        let err = expect_complete_err(&mut cor, Some(MaildirReply::DirCreate));
        assert!(matches!(err, MaildirDeleteError::UnexpectedArg(_)));
    }

    fn expect_wants_dir_remove(cor: &mut MaildirDelete) -> BTreeSet<MaildirFsPath> {
        match cor.resume(None) {
            MaildirCoroutineState::Yielded(MaildirYield::WantsDirRemove(paths)) => paths,
            state => panic!("expected WantsDirRemove, got {state:?}"),
        }
    }

    fn expect_complete_ok(cor: &mut MaildirDelete, arg: Option<MaildirReply>) {
        match cor.resume(arg) {
            MaildirCoroutineState::Complete(Ok(())) => {}
            state => panic!("expected Complete(Ok), got {state:?}"),
        }
    }

    fn expect_complete_err(
        cor: &mut MaildirDelete,
        arg: Option<MaildirReply>,
    ) -> MaildirDeleteError {
        match cor.resume(arg) {
            MaildirCoroutineState::Complete(Err(err)) => err,
            state => panic!("expected Complete(Err), got {state:?}"),
        }
    }
}
