//! I/O-free coroutine renaming a Maildir directory.
//!
//! # Example
//!
//! ```rust,no_run
//! use io_maildir::{
//!     client::MaildirClient,
//!     maildir::rename::MaildirRename,
//!     path::MaildirPath,
//! };
//!
//! let client = MaildirClient::new("/path/to/root");
//!
//! let coroutine = MaildirRename::new(
//!     &client.store,
//!     MaildirPath::from("inbox"),
//!     MaildirPath::from("archive"),
//! );
//! client.run(coroutine).unwrap();
//! ```

use core::{fmt, mem};

use alloc::{collections::BTreeSet, format, vec::Vec};

use log::debug;
use thiserror::Error;

use crate::{
    coroutine::*,
    path::{MaildirFsPath, MaildirPath},
    store::MaildirStore,
};

/// Failure causes during a [`MaildirRename`] step.
#[derive(Clone, Debug, Error)]
pub enum MaildirRenameError {
    /// A reply arrived that does not match the awaited step.
    #[error("Maildir rename failed: unexpected arg {0:?}")]
    UnexpectedArg(Option<MaildirReply>),
    /// A name resolves to the store root, outside it or onto a
    /// subdirectory.
    #[error("Maildir rename failed: `{0}` names no folder below the store root")]
    NotAFolder(MaildirPath),
    /// A Maildir++ folder the rename would land on already exists.
    #[error("Maildir rename failed: {0} already exists")]
    TargetExists(MaildirFsPath),
}

/// Renames a Maildir from `from` to `to`, both logical mailbox paths
/// resolved through the store. Crossing parents is fine in either
/// layout: fs moves the directory tree, Maildir++ renames the flat
/// dotted entry and every `.<from>.*` sibling holding its children, as
/// the fs move takes them along.
#[derive(Debug)]
pub struct MaildirRename {
    state: State,
}

impl MaildirRename {
    /// Builds a coroutine renaming the Maildir at `from` to `to`, both
    /// logical mailbox paths resolved through `store`.
    ///
    /// A name [`MaildirStore::names_folder`] rejects is refused on the
    /// first resume, nothing renamed.
    pub fn new(store: &MaildirStore, from: MaildirPath, to: MaildirPath) -> Self {
        let mut names = [&from, &to].into_iter();
        if let Some(name) = names.find(|name| !store.names_folder(name)) {
            return Self {
                state: State::Refused(name.clone()),
            };
        }

        let from = store.resolve(&from);
        let to = store.resolve(&to);

        if store.maildirpp {
            return Self {
                state: State::Siblings {
                    root: store.root.clone(),
                    from,
                    to,
                },
            };
        }

        Self {
            state: State::Start {
                pairs: vec![(from, to)],
            },
        }
    }
}

/// The renames a Maildir++ rename of `from` to `to` makes: `from` itself
/// then every entry of `names`, the root's, named `<from>.*`.
///
/// Refuses a rename landing on an existing entry, so no folder is merged
/// into another.
fn maildirpp_pairs(
    from: MaildirFsPath,
    to: MaildirFsPath,
    names: &BTreeSet<MaildirFsPath>,
) -> Result<Vec<(MaildirFsPath, MaildirFsPath)>, MaildirRenameError> {
    let prefix = format!("{from}.");
    let mut pairs = vec![(from, to.clone())];

    for name in names {
        if let Some(rest) = name.as_str().strip_prefix(&prefix) {
            let target = MaildirFsPath::from(format!("{to}.{rest}"));
            pairs.push((name.clone(), target));
        }
    }

    if let Some((_, target)) = pairs.iter().find(|(_, target)| names.contains(target)) {
        return Err(MaildirRenameError::TargetExists(target.clone()));
    }

    Ok(pairs)
}

impl MaildirCoroutine for MaildirRename {
    type Yield = MaildirYield;
    type Return = Result<(), MaildirRenameError>;

    fn resume(
        &mut self,
        arg: Option<MaildirReply>,
    ) -> MaildirCoroutineState<Self::Yield, Self::Return> {
        match (&mut self.state, arg) {
            (State::Refused(name), None) => {
                let err = MaildirRenameError::NotAFolder(mem::take(name));
                MaildirCoroutineState::Complete(Err(err))
            }
            (State::Siblings { root, .. }, None) => {
                let root = mem::take(root);
                let paths = BTreeSet::from([root]);
                MaildirCoroutineState::Yielded(MaildirYield::WantsDirRead(paths))
            }
            (State::Siblings { from, to, .. }, Some(MaildirReply::DirRead(entries))) => {
                let names = entries.into_values().flatten().collect();
                match maildirpp_pairs(mem::take(from), mem::take(to), &names) {
                    Ok(pairs) => {
                        self.state = State::Rename;
                        MaildirCoroutineState::Yielded(MaildirYield::WantsRename(pairs))
                    }
                    Err(err) => MaildirCoroutineState::Complete(Err(err)),
                }
            }
            (State::Start { pairs }, None) => {
                let pairs = mem::take(pairs);
                self.state = State::Rename;
                MaildirCoroutineState::Yielded(MaildirYield::WantsRename(pairs))
            }
            (State::Rename, Some(MaildirReply::Rename)) => {
                debug!("renamed maildir");
                MaildirCoroutineState::Complete(Ok(()))
            }
            (_, arg) => {
                let err = MaildirRenameError::UnexpectedArg(arg);
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
        from: MaildirFsPath,
        to: MaildirFsPath,
    },
    Start {
        pairs: Vec<(MaildirFsPath, MaildirFsPath)>,
    },
    Rename,
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused(_) => f.write_str("refuse"),
            Self::Siblings { .. } => f.write_str("read siblings"),
            Self::Start { .. } => f.write_str("start"),
            Self::Rename => f.write_str("rename maildir"),
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::collections::BTreeMap;

    use crate::maildir::rename::*;

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
    fn fs_resolves_both_sides() {
        let mut cor = MaildirRename::new(
            &fs_store(),
            MaildirPath::from("Old"),
            MaildirPath::from("New"),
        );

        let pairs = expect_wants_rename(&mut cor);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].0, MaildirFsPath::from("root/Old"));
        assert_eq!(pairs[0].1, MaildirFsPath::from("root/New"));

        expect_complete_ok(&mut cor, Some(MaildirReply::Rename));
    }

    #[test]
    fn maildirpp_flattens_both_sides() {
        let mut cor = MaildirRename::new(
            &maildirpp_store(),
            MaildirPath::from("Foo/Bar"),
            MaildirPath::from("Baz/Qux"),
        );

        expect_wants_root_read(&mut cor);
        let pairs = expect_rename_after_read(&mut cor, &["root/.Foo.Bar", "root/.Other"]);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].0, MaildirFsPath::from("root/.Foo.Bar"));
        assert_eq!(pairs[0].1, MaildirFsPath::from("root/.Baz.Qux"));
    }

    #[test]
    fn maildirpp_renames_the_children_along() {
        let mut cor = MaildirRename::new(
            &maildirpp_store(),
            MaildirPath::from("Lists/rust"),
            MaildirPath::from("Lists/go"),
        );

        expect_wants_root_read(&mut cor);
        let names = [
            "root/.Lists.rust",
            "root/.Lists.rust.Kaffee",
            "root/.Lists.rust.Kaffee.Deep",
            "root/.Lists.rustacean",
            "root/cur",
        ];
        let pairs = expect_rename_after_read(&mut cor, &names);

        let pairs: Vec<(&str, &str)> = pairs
            .iter()
            .map(|(from, to)| (from.as_str(), to.as_str()))
            .collect();
        assert_eq!(
            pairs,
            [
                ("root/.Lists.rust", "root/.Lists.go"),
                ("root/.Lists.rust.Kaffee", "root/.Lists.go.Kaffee"),
                ("root/.Lists.rust.Kaffee.Deep", "root/.Lists.go.Kaffee.Deep"),
            ]
        );
    }

    #[test]
    fn maildirpp_refuses_a_child_landing_on_an_existing_folder() {
        let mut cor = MaildirRename::new(
            &maildirpp_store(),
            MaildirPath::from("A"),
            MaildirPath::from("B"),
        );

        expect_wants_root_read(&mut cor);
        let names = ["root/.A", "root/.A.Kid", "root/.B.Kid"];
        let entries = BTreeMap::from([(
            MaildirFsPath::from("root"),
            names.into_iter().map(MaildirFsPath::from).collect(),
        )]);
        let err = expect_complete_err(&mut cor, Some(MaildirReply::DirRead(entries)));
        assert!(
            matches!(err, MaildirRenameError::TargetExists(path) if path.as_str() == "root/.B.Kid")
        );
    }

    fn expect_wants_root_read(cor: &mut MaildirRename) {
        match cor.resume(None) {
            MaildirCoroutineState::Yielded(MaildirYield::WantsDirRead(paths)) => {
                assert_eq!(paths, BTreeSet::from([MaildirFsPath::from("root")]));
            }
            state => panic!("expected WantsDirRead, got {state:?}"),
        }
    }

    fn expect_rename_after_read(
        cor: &mut MaildirRename,
        names: &[&str],
    ) -> Vec<(MaildirFsPath, MaildirFsPath)> {
        let names = names
            .iter()
            .map(|name| MaildirFsPath::from(*name))
            .collect();
        let entries = BTreeMap::from([(MaildirFsPath::from("root"), names)]);
        match cor.resume(Some(MaildirReply::DirRead(entries))) {
            MaildirCoroutineState::Yielded(MaildirYield::WantsRename(pairs)) => pairs,
            state => panic!("expected WantsRename, got {state:?}"),
        }
    }

    #[test]
    fn the_root_and_paths_leaving_it_are_refused_on_either_side() {
        for (from, to) in [(".", "New"), ("", "New"), ("Old", "."), ("Old", "../x")] {
            for store in [fs_store(), maildirpp_store()] {
                let from = MaildirPath::from(from);
                let mut cor = MaildirRename::new(&store, from, MaildirPath::from(to));
                let err = expect_complete_err(&mut cor, None);
                assert!(matches!(err, MaildirRenameError::NotAFolder(_)), "{to}");
            }
        }
    }

    #[test]
    fn the_subdirectories_of_a_maildir_are_refused_in_the_fs_layout() {
        for (from, to) in [("cur", "Kept"), ("Work/tmp", "Kept"), ("Work", "new")] {
            let from = MaildirPath::from(from);
            let mut cor = MaildirRename::new(&fs_store(), from.clone(), MaildirPath::from(to));
            let err = expect_complete_err(&mut cor, None);
            assert!(matches!(err, MaildirRenameError::NotAFolder(_)), "{to}");

            let mut cor = MaildirRename::new(&maildirpp_store(), from, MaildirPath::from(to));
            expect_wants_root_read(&mut cor);
        }
    }

    #[test]
    fn unexpected_reply_returns_error() {
        let mut cor = MaildirRename::new(
            &fs_store(),
            MaildirPath::from("Old"),
            MaildirPath::from("New"),
        );
        let _ = expect_wants_rename(&mut cor);

        let err = expect_complete_err(&mut cor, Some(MaildirReply::Copy));
        assert!(matches!(err, MaildirRenameError::UnexpectedArg(_)));
    }

    fn expect_wants_rename(cor: &mut MaildirRename) -> Vec<(MaildirFsPath, MaildirFsPath)> {
        match cor.resume(None) {
            MaildirCoroutineState::Yielded(MaildirYield::WantsRename(pairs)) => pairs,
            state => panic!("expected WantsRename, got {state:?}"),
        }
    }

    fn expect_complete_ok(cor: &mut MaildirRename, arg: Option<MaildirReply>) {
        match cor.resume(arg) {
            MaildirCoroutineState::Complete(Ok(())) => {}
            state => panic!("expected Complete(Ok), got {state:?}"),
        }
    }

    fn expect_complete_err(
        cor: &mut MaildirRename,
        arg: Option<MaildirReply>,
    ) -> MaildirRenameError {
        match cor.resume(arg) {
            MaildirCoroutineState::Complete(Err(err)) => err,
            state => panic!("expected Complete(Err), got {state:?}"),
        }
    }
}
