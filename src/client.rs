//! # Standard, blocking Maildir client
//!
//! Holds a single filesystem root [`PathBuf`] and exposes one method
//! per coroutine. There is no network layer and no long-lived session
//! context: every method runs its coroutine to completion by
//! performing the requested filesystem operations via [`std::fs`] in
//! a resume loop.
//!
//! [`PathBuf`]: std::path::PathBuf

use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::{String, ToString},
    vec::Vec,
};
use std::{
    collections::HashSet,
    fs, io,
    path::{Path, PathBuf},
};

use log::trace;
use thiserror::Error;

use crate::{
    coroutines::{
        flags_add::*, flags_remove::*, flags_set::*, maildir_create::*, maildir_delete::*,
        maildir_list::*, maildir_rename::*, message_copy::*, message_get::*, message_list::*,
        message_locate::*, message_move::*, message_store::*,
    },
    flag::Flags,
    maildir::{Maildir, MaildirSubdir},
    message::Message,
};

/// Errors returned by [`MaildirClient`].
#[derive(Debug, Error)]
pub enum MaildirClientError {
    #[error(transparent)]
    FlagsAdd(#[from] MaildirFlagsAddError),
    #[error(transparent)]
    FlagsRemove(#[from] MaildirFlagsRemoveError),
    #[error(transparent)]
    FlagsSet(#[from] MaildirFlagsSetError),

    #[error(transparent)]
    MaildirCreate(#[from] MaildirCreateError),
    #[error(transparent)]
    MaildirDelete(#[from] MaildirDeleteError),
    #[error(transparent)]
    MaildirList(#[from] MaildirListError),
    #[error(transparent)]
    MaildirRename(#[from] MaildirRenameError),

    #[error(transparent)]
    MessageCopy(#[from] MaildirMessageCopyError),
    #[error(transparent)]
    MessageGet(#[from] MaildirMessageGetError),
    #[error(transparent)]
    MessageLocate(#[from] MaildirMessageLocateError),
    #[error(transparent)]
    MessagesList(#[from] MaildirMessagesListError),
    #[error(transparent)]
    MessageMove(#[from] MaildirMessageMoveError),
    #[error(transparent)]
    MessageStore(#[from] MaildirMessageStoreError),

    #[error(transparent)]
    Io(#[from] io::Error),
}

/// Std-blocking Maildir client wrapping a filesystem root.
#[derive(Debug)]
pub struct MaildirClient {
    root: PathBuf,
}

fn read_dirs(paths: BTreeSet<String>) -> Result<BTreeMap<String, BTreeSet<String>>, io::Error> {
    let mut entries = BTreeMap::new();

    for path in paths {
        trace!("read_dir {path}");
        let mut names = BTreeSet::new();

        for entry in fs::read_dir(&path)? {
            let entry = entry?;
            names.insert(entry.path().to_string_lossy().into_owned());
        }

        entries.insert(path, names);
    }

    Ok(entries)
}

fn read_files(paths: BTreeSet<String>) -> Result<BTreeMap<String, Vec<u8>>, io::Error> {
    let mut contents = BTreeMap::new();

    for path in paths {
        trace!("read_file {path}");
        let bytes = fs::read(&path)?;
        contents.insert(path, bytes);
    }

    Ok(contents)
}

fn rename_paths(pairs: Vec<(String, String)>) -> Result<(), io::Error> {
    for (from, to) in pairs {
        trace!("rename {from} -> {to}");
        fs::rename(&from, &to)?;
    }
    Ok(())
}

impl MaildirClient {
    /// Builds a client rooted at `root`. The root is the directory
    /// where Maildirs are listed, created or deleted by name, and is
    /// also used as a base for resolving Maildir names in convenience
    /// methods.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Returns the filesystem root this client operates on.
    pub fn root(&self) -> &Path {
        &self.root
    }

    // ---- Maildir lifecycle ------------------------------------------------

    /// Runs [`MaildirCreate`]: creates the Maildir at `path` (the
    /// `root`, `cur`, `new`, `tmp` quartet).
    pub fn create_maildir(&self, path: impl AsRef<Path>) -> Result<(), MaildirClientError> {
        let mut coroutine = MaildirCreate::new(path);
        let mut arg: Option<MaildirCreateArg> = None;

        loop {
            match coroutine.resume(arg.take()) {
                MaildirCreateResult::Ok => return Ok(()),
                MaildirCreateResult::WantsDirCreate(paths) => {
                    for path in paths {
                        trace!("create_dir_all {path}");
                        fs::create_dir_all(&path)?;
                    }
                    arg = Some(MaildirCreateArg::DirCreate);
                }
                MaildirCreateResult::Err(err) => return Err(err.into()),
            }
        }
    }

    /// Runs [`MaildirDelete`]: recursively removes the Maildir rooted
    /// at `path`.
    pub fn delete_maildir(&self, path: impl AsRef<Path>) -> Result<(), MaildirClientError> {
        let mut coroutine = MaildirDelete::new(path);
        let mut arg: Option<MaildirDeleteArg> = None;

        loop {
            match coroutine.resume(arg.take()) {
                MaildirDeleteResult::Ok => return Ok(()),
                MaildirDeleteResult::WantsDirRemove(paths) => {
                    for path in paths {
                        trace!("remove_dir_all {path}");
                        fs::remove_dir_all(&path)?;
                    }
                    arg = Some(MaildirDeleteArg::DirRemove);
                }
                MaildirDeleteResult::Err(err) => return Err(err.into()),
            }
        }
    }

    /// Runs [`MaildirList`]: lists every valid Maildir directly under
    /// [`self.root`](Self::root).
    pub fn list_maildirs(&self) -> Result<HashSet<Maildir>, MaildirClientError> {
        let mut coroutine = MaildirList::new(&self.root);
        let mut arg: Option<MaildirListArg> = None;

        loop {
            match coroutine.resume(arg.take()) {
                MaildirListResult::Ok(maildirs) => return Ok(maildirs),
                MaildirListResult::WantsDirRead(paths) => {
                    let entries = read_dirs(paths)?;
                    arg = Some(MaildirListArg::DirRead(entries));
                }
                MaildirListResult::Err(err) => return Err(err.into()),
            }
        }
    }

    /// Runs [`MaildirRename`]: renames the Maildir at `path` to `name`
    /// (keeping the same parent directory).
    pub fn rename_maildir(
        &self,
        path: impl AsRef<Path>,
        name: impl ToString,
    ) -> Result<(), MaildirClientError> {
        let mut coroutine = MaildirRename::new(path, name);
        let mut arg: Option<MaildirRenameArg> = None;

        loop {
            match coroutine.resume(arg.take()) {
                MaildirRenameResult::Ok => return Ok(()),
                MaildirRenameResult::WantsRename(pairs) => {
                    rename_paths(pairs)?;
                    arg = Some(MaildirRenameArg::Rename);
                }
                MaildirRenameResult::Err(err) => return Err(err.into()),
            }
        }
    }

    // ---- Flags ------------------------------------------------------------

    /// Runs [`MaildirFlagsAdd`]: adds `flags` to message `id` in
    /// `maildir`. Messages in `/new` or `/tmp` are left unchanged.
    pub fn add_flags(
        &self,
        maildir: Maildir,
        id: impl ToString,
        flags: Flags,
    ) -> Result<(), MaildirClientError> {
        let mut coroutine = MaildirFlagsAdd::new(maildir, id, flags);
        let mut arg: Option<MaildirFlagsAddArg> = None;

        loop {
            match coroutine.resume(arg.take()) {
                MaildirFlagsAddResult::Ok => return Ok(()),
                MaildirFlagsAddResult::WantsDirRead(paths) => {
                    let entries = read_dirs(paths)?;
                    arg = Some(MaildirFlagsAddArg::DirRead(entries));
                }
                MaildirFlagsAddResult::WantsRename(pairs) => {
                    rename_paths(pairs)?;
                    arg = Some(MaildirFlagsAddArg::Rename);
                }
                MaildirFlagsAddResult::Err(err) => return Err(err.into()),
            }
        }
    }

    /// Runs [`MaildirFlagsRemove`]: removes `flags` from message `id`
    /// in `maildir`. Messages in `/new` or `/tmp` are left unchanged.
    pub fn remove_flags(
        &self,
        maildir: Maildir,
        id: impl ToString,
        flags: Flags,
    ) -> Result<(), MaildirClientError> {
        let mut coroutine = MaildirFlagsRemove::new(maildir, id, flags);
        let mut arg: Option<MaildirFlagsRemoveArg> = None;

        loop {
            match coroutine.resume(arg.take()) {
                MaildirFlagsRemoveResult::Ok => return Ok(()),
                MaildirFlagsRemoveResult::WantsDirRead(paths) => {
                    let entries = read_dirs(paths)?;
                    arg = Some(MaildirFlagsRemoveArg::DirRead(entries));
                }
                MaildirFlagsRemoveResult::WantsRename(pairs) => {
                    rename_paths(pairs)?;
                    arg = Some(MaildirFlagsRemoveArg::Rename);
                }
                MaildirFlagsRemoveResult::Err(err) => return Err(err.into()),
            }
        }
    }

    /// Runs [`MaildirFlagsSet`]: replaces the flags of message `id` in
    /// `maildir` with `flags`. Messages in `/new` or `/tmp` are left
    /// unchanged.
    pub fn set_flags(
        &self,
        maildir: Maildir,
        id: impl ToString,
        flags: Flags,
    ) -> Result<(), MaildirClientError> {
        let mut coroutine = MaildirFlagsSet::new(maildir, id, flags);
        let mut arg: Option<MaildirFlagsSetArg> = None;

        loop {
            match coroutine.resume(arg.take()) {
                MaildirFlagsSetResult::Ok => return Ok(()),
                MaildirFlagsSetResult::WantsDirRead(paths) => {
                    let entries = read_dirs(paths)?;
                    arg = Some(MaildirFlagsSetArg::DirRead(entries));
                }
                MaildirFlagsSetResult::WantsRename(pairs) => {
                    rename_paths(pairs)?;
                    arg = Some(MaildirFlagsSetArg::Rename);
                }
                MaildirFlagsSetResult::Err(err) => return Err(err.into()),
            }
        }
    }

    // ---- Messages ---------------------------------------------------------

    /// Runs [`MaildirMessageLocate`]: finds the on-disk path of
    /// message `id` inside `maildir` and returns its subdir and flags.
    pub fn locate(
        &self,
        maildir: Maildir,
        id: impl ToString,
    ) -> Result<(PathBuf, MaildirSubdir, Flags), MaildirClientError> {
        let mut coroutine = MaildirMessageLocate::new(maildir, id);
        let mut arg: Option<MaildirMessageLocateArg> = None;

        loop {
            match coroutine.resume(arg.take()) {
                MaildirMessageLocateResult::Ok {
                    path,
                    subdir,
                    flags,
                } => return Ok((path, subdir, flags)),
                MaildirMessageLocateResult::WantsDirRead(paths) => {
                    let entries = read_dirs(paths)?;
                    arg = Some(MaildirMessageLocateArg::DirRead(entries));
                }
                MaildirMessageLocateResult::Err(err) => return Err(err.into()),
            }
        }
    }

    /// Runs [`MaildirMessageGet`]: locates message `id` in `maildir`
    /// and reads its contents from disk.
    pub fn get(&self, maildir: Maildir, id: impl ToString) -> Result<Message, MaildirClientError> {
        let mut coroutine = MaildirMessageGet::new(maildir, id);
        let mut arg: Option<MaildirMessageGetArg> = None;

        loop {
            match coroutine.resume(arg.take()) {
                MaildirMessageGetResult::Ok(message) => return Ok(message),
                MaildirMessageGetResult::WantsDirRead(paths) => {
                    let entries = read_dirs(paths)?;
                    arg = Some(MaildirMessageGetArg::DirRead(entries));
                }
                MaildirMessageGetResult::WantsFileRead(paths) => {
                    let contents = read_files(paths)?;
                    arg = Some(MaildirMessageGetArg::FileRead(contents));
                }
                MaildirMessageGetResult::Err(err) => return Err(err.into()),
            }
        }
    }

    /// Runs [`MaildirMessagesList`]: scans both `/new` and `/cur` of
    /// `maildir` and returns every message it finds.
    pub fn list_messages(&self, maildir: Maildir) -> Result<HashSet<Message>, MaildirClientError> {
        let mut coroutine = MaildirMessagesList::new(maildir);
        let mut arg: Option<MaildirMessagesListArg> = None;

        loop {
            match coroutine.resume(arg.take()) {
                MaildirMessagesListResult::Ok(messages) => return Ok(messages),
                MaildirMessagesListResult::WantsDirRead(paths) => {
                    arg = Some(MaildirMessagesListArg::DirRead(read_dirs(paths)?));
                }
                MaildirMessagesListResult::WantsFileRead(paths) => {
                    arg = Some(MaildirMessagesListArg::FileRead(read_files(paths)?));
                }
                MaildirMessagesListResult::Err(err) => return Err(err.into()),
            }
        }
    }

    /// Runs [`MaildirMessageStore`]: writes `contents` to `tmp` then
    /// atomically renames it under `subdir` of `maildir` with the
    /// given `flags`. Returns the generated message ID and final path.
    pub fn store(
        &self,
        maildir: Maildir,
        subdir: MaildirSubdir,
        flags: Flags,
        contents: Vec<u8>,
    ) -> Result<(String, PathBuf), MaildirClientError> {
        let mut coroutine = MaildirMessageStore::new(maildir, subdir, flags, contents);
        let mut arg: Option<MaildirMessageStoreArg> = None;

        loop {
            match coroutine.resume(arg.take()) {
                MaildirMessageStoreResult::Ok { id, path } => return Ok((id, path)),
                MaildirMessageStoreResult::WantsFileCreate(files) => {
                    for (path, contents) in files {
                        trace!("write {path} ({} bytes)", contents.len());
                        fs::write(&path, &contents)?;
                    }
                    arg = Some(MaildirMessageStoreArg::FileCreate);
                }
                MaildirMessageStoreResult::WantsRename(pairs) => {
                    rename_paths(pairs)?;
                    arg = Some(MaildirMessageStoreArg::Rename);
                }
                MaildirMessageStoreResult::Err(err) => return Err(err.into()),
            }
        }
    }

    /// Runs [`MaildirMessageCopy`]: copies message `id` from `source`
    /// into `target`. If `target_subdir` is [`None`] the message is
    /// placed in the same subdir as in the source Maildir.
    pub fn copy(
        &self,
        id: impl ToString,
        source: Maildir,
        target: Maildir,
        target_subdir: Option<MaildirSubdir>,
    ) -> Result<(), MaildirClientError> {
        let mut coroutine = MaildirMessageCopy::new(id, source, target, target_subdir);
        let mut arg: Option<MaildirMessageCopyArg> = None;

        loop {
            match coroutine.resume(arg.take()) {
                MaildirMessageCopyResult::Ok => return Ok(()),
                MaildirMessageCopyResult::WantsDirRead(paths) => {
                    let entries = read_dirs(paths)?;
                    arg = Some(MaildirMessageCopyArg::DirRead(entries));
                }
                MaildirMessageCopyResult::WantsCopy(pairs) => {
                    for (from, to) in pairs {
                        trace!("copy {from} -> {to}");
                        fs::copy(&from, &to)?;
                    }
                    arg = Some(MaildirMessageCopyArg::Copy);
                }
                MaildirMessageCopyResult::Err(err) => return Err(err.into()),
            }
        }
    }

    /// Runs [`MaildirMessageMove`]: moves message `id` from `source`
    /// into `target`. If `target_subdir` is [`None`] the message is
    /// placed in the same subdir as in the source Maildir.
    pub fn r#move(
        &self,
        id: impl ToString,
        source: Maildir,
        target: Maildir,
        target_subdir: Option<MaildirSubdir>,
    ) -> Result<(), MaildirClientError> {
        let mut coroutine = MaildirMessageMove::new(id, source, target, target_subdir);
        let mut arg: Option<MaildirMessageMoveArg> = None;

        loop {
            match coroutine.resume(arg.take()) {
                MaildirMessageMoveResult::Ok => return Ok(()),
                MaildirMessageMoveResult::WantsDirRead(paths) => {
                    let entries = read_dirs(paths)?;
                    arg = Some(MaildirMessageMoveArg::DirRead(entries));
                }
                MaildirMessageMoveResult::WantsRename(pairs) => {
                    rename_paths(pairs)?;
                    arg = Some(MaildirMessageMoveArg::Rename);
                }
                MaildirMessageMoveResult::Err(err) => return Err(err.into()),
            }
        }
    }
}
