//! # Generator-shape coroutine driver
//!
//! Mirrors the shape of `core::ops::Coroutine`: a `Yield` associated type for
//! intermediate progress, a `Return` associated type for terminal output, and a
//! two-variant [`MaildirCoroutineState`] (`Yielded` / `Complete`).
//!
//! io-maildir is filesystem-flavoured: every coroutine in this crate picks
//! [`MaildirYield`] as its `Yield`, mixing filesystem `Wants*` requests with
//! environmental inputs (time / pid / counter / hostname) needed to mint
//! Maildir message identifiers, so the core holds no state of its own. The
//! terminal `Return` is always a per-coroutine `Result<Output, Error>`.
//!
//! [`MaildirClient::run`] drives any standard-Yield coroutine to completion
//! against the local filesystem.
//!
//! [`MaildirClient::run`]: crate::client::MaildirClient::run

use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::String,
    vec::Vec,
};

use crate::path::MaildirFsPath;

/// State yielded by a [`MaildirCoroutine::resume`] step.
///
/// Two-variant by design (matches std's `core::ops::CoroutineState`): any
/// further variation lives inside the per-coroutine `Yield` type.
#[derive(Debug)]
pub enum MaildirCoroutineState<Y, R> {
    /// Intermediate yield. The driver reacts to `Y` (perform the requested
    /// filesystem / environment op, feed the answer back) and resumes the
    /// coroutine again.
    Yielded(Y),
    /// Terminal yield. By convention `R = Result<Output, Error>`.
    Complete(R),
}

/// Standard-shape Maildir coroutine.
///
/// Implementors own their internal state machine and declare their per-step
/// `Yield` plus a terminal `Return`. The driver reacts to each `Yield` variant
/// (run a filesystem op, sample the clock, look up the hostname) and resumes
/// until `Complete`.
pub trait MaildirCoroutine {
    /// Intermediate value handed back on every step. Per-coroutine: each
    /// implementor picks exactly the variants it needs.
    type Yield;
    /// Terminal value. By convention `Result<Output, Error>`; the "ok" arm
    /// carries the operation's final output, the "error" arm carries the cause.
    type Return;

    /// Advances the coroutine one step.
    ///
    /// Pass [`None`] on the initial call or after a yield that does not request
    /// data (e.g. [`MaildirYield::WantsFileCreate`]). Pass `Some(reply)`
    /// carrying the answer to the previous yield (e.g.  the existence map for
    /// [`MaildirYield::WantsFileExists`]).
    fn resume(
        &mut self,
        arg: Option<MaildirReply>,
    ) -> MaildirCoroutineState<Self::Yield, Self::Return>;
}

/// Standard I/O Yield used by every io-maildir coroutine.
///
/// Mixes filesystem step requests with the four environmental inputs the
/// Maildir delivery protocol needs to mint message identifiers (time, pid,
/// counter, hostname).
#[derive(Debug)]
pub enum MaildirYield {
    /// Driver must check each path for existence as a regular file and resume
    /// with [`MaildirReply::FileExists`].
    WantsFileExists(BTreeSet<MaildirFsPath>),

    /// Driver must check each path for existence as a directory and resume with
    /// [`MaildirReply::DirExists`].
    WantsDirExists(BTreeSet<MaildirFsPath>),

    /// Driver must list each directory's entries with their kind and
    /// resume with [`MaildirReply::DirRead`].
    WantsDirRead(BTreeSet<MaildirFsPath>),

    /// Driver must read each file's bytes and resume with
    /// [`MaildirReply::FileRead`].
    WantsFileRead(BTreeSet<MaildirFsPath>),

    /// Driver must write each `(path, bytes)` pair and resume with
    /// [`MaildirReply::FileCreate`].
    WantsFileCreate(BTreeMap<MaildirFsPath, Vec<u8>>),

    /// Driver must create each directory (with parents) and resume with
    /// [`MaildirReply::DirCreate`].
    WantsDirCreate(BTreeSet<MaildirFsPath>),

    /// Driver must recursively remove each directory and resume with
    /// [`MaildirReply::DirRemove`].
    WantsDirRemove(BTreeSet<MaildirFsPath>),

    /// Driver must rename each `(from, to)` pair and resume with
    /// [`MaildirReply::Rename`].
    WantsRename(Vec<(MaildirFsPath, MaildirFsPath)>),

    /// Driver must copy each `(from, to)` pair and resume with
    /// [`MaildirReply::Copy`].
    WantsCopy(Vec<(MaildirFsPath, MaildirFsPath)>),

    /// Driver must flush each file to stable storage and resume with
    /// [`MaildirReply::FileSync`].
    WantsFileSync(BTreeSet<MaildirFsPath>),

    /// Driver must flush each directory to stable storage, so the names
    /// it holds survive a power cut, and resume with
    /// [`MaildirReply::DirSync`].
    WantsDirSync(BTreeSet<MaildirFsPath>),

    /// Driver must supply the current Unix time and resume with
    /// [`MaildirReply::Time`].
    WantsTime,

    /// Driver must supply the current process id and resume with
    /// [`MaildirReply::Pid`].
    WantsPid,

    /// Driver must supply a number it gave to no other request in this
    /// process, such as a count of its deliveries, and resume with
    /// [`MaildirReply::Counter`].
    WantsCounter,

    /// Driver must supply the host name and resume with
    /// [`MaildirReply::Hostname`].
    WantsHostname,
}

/// Reply fed back into [`MaildirCoroutine::resume`] by the driver.
///
/// One variant per [`MaildirYield`] request; the coroutine asserts the variant
/// it expects and ignores the rest.
#[derive(Clone, Debug)]
pub enum MaildirReply {
    /// Answer to [`MaildirYield::WantsFileExists`].
    FileExists(BTreeMap<MaildirFsPath, bool>),

    /// Answer to [`MaildirYield::WantsDirExists`].
    DirExists(BTreeMap<MaildirFsPath, bool>),

    /// Answer to [`MaildirYield::WantsDirRead`]: per directory, every
    /// entry path with its kind.
    DirRead(BTreeMap<MaildirFsPath, BTreeMap<MaildirFsPath, MaildirFileKind>>),

    /// Answer to [`MaildirYield::WantsFileRead`].
    FileRead(BTreeMap<MaildirFsPath, Vec<u8>>),

    /// Acknowledgement of [`MaildirYield::WantsFileCreate`].
    FileCreate,

    /// Acknowledgement of [`MaildirYield::WantsDirCreate`].
    DirCreate,

    /// Acknowledgement of [`MaildirYield::WantsDirRemove`].
    DirRemove,

    /// Acknowledgement of [`MaildirYield::WantsRename`].
    Rename,

    /// Acknowledgement of [`MaildirYield::WantsCopy`].
    Copy,

    /// Acknowledgement of [`MaildirYield::WantsFileSync`].
    FileSync,

    /// Acknowledgement of [`MaildirYield::WantsDirSync`].
    DirSync,

    /// Answer to [`MaildirYield::WantsTime`].
    Time {
        /// Whole seconds elapsed since the Unix epoch.
        secs: u64,
        /// Sub-second remainder in nanoseconds.
        nanos: u32,
    },

    /// Answer to [`MaildirYield::WantsPid`].
    Pid(u32),

    /// Answer to [`MaildirYield::WantsCounter`].
    Counter(u32),

    /// Answer to [`MaildirYield::WantsHostname`].
    Hostname(String),
}

#[cfg(test)]
impl MaildirReply {
    /// Builds a [`Self::DirRead`] whose every entry is a regular file.
    pub(crate) fn files(entries: BTreeMap<MaildirFsPath, BTreeSet<MaildirFsPath>>) -> Self {
        let files = |names: BTreeSet<MaildirFsPath>| {
            let names = names.into_iter();
            names.map(|name| (name, MaildirFileKind::File)).collect()
        };
        let entries = entries.into_iter();
        Self::DirRead(entries.map(|(dir, names)| (dir, files(names))).collect())
    }

    /// Builds the [`Self::DirRead`] of `dirs` over the entries of `tree`.
    pub(crate) fn read(dirs: BTreeSet<MaildirFsPath>, tree: &[(&str, MaildirFileKind)]) -> Self {
        let children = |dir: &MaildirFsPath| {
            let entries = tree
                .iter()
                .map(|(path, kind)| (MaildirFsPath::from(*path), *kind));
            entries
                .filter(|(path, _)| path.parent() == Some(dir.as_str()))
                .collect()
        };
        let mut entries = BTreeMap::new();
        for dir in dirs {
            let names = children(&dir);
            entries.insert(dir, names);
        }
        Self::DirRead(entries)
    }
}

/// Kind of a directory entry, a symlink counting as what it points to.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum MaildirFileKind {
    /// A regular file.
    File,
    /// A directory.
    Dir,
    /// Anything else: a socket, a device, a dangling symlink.
    Other,
    /// The entry was listed, then gone when its kind was read: renamed or
    /// removed meanwhile. A driver reading both at once never reports it.
    Missing,
}

/// Coroutine `?`: forwards `Yielded` (via `Into`), short-circuits on `Err`,
/// evaluates to the inner `Ok` value.
#[macro_export]
macro_rules! maildir_try {
    ($coroutine:expr, $arg:expr $(,)?) => {
        match $crate::coroutine::MaildirCoroutine::resume($coroutine, $arg) {
            $crate::coroutine::MaildirCoroutineState::Yielded(y) => {
                return $crate::coroutine::MaildirCoroutineState::Yielded(y.into());
            }
            $crate::coroutine::MaildirCoroutineState::Complete(Err(err)) => {
                return $crate::coroutine::MaildirCoroutineState::Complete(Err(err.into()));
            }
            $crate::coroutine::MaildirCoroutineState::Complete(Ok(value)) => value,
        }
    };
}
