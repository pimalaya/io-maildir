//! Standard blocking Maildir client driving any coroutine against [`std::fs`].

use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::{String, ToString},
    vec::Vec,
};

use std::{
    fs, io,
    path::{Path, PathBuf},
    process,
    sync::atomic::{AtomicU32, Ordering},
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

use gethostname::gethostname;
use log::{trace, warn};
use thiserror::Error;

use crate::{
    coroutine::*,
    dovecot::{
        load::*,
        store::*,
        utils::{allocate_keyword_slot, keyword_slot},
    },
    entry::{
        MaildirEntry, MaildirFullEntry,
        copy::*,
        get::*,
        headers::{inject_header, strip_headers},
        list::*,
        locate::*,
        locate_many::*,
        r#move::*,
        store::*,
    },
    flag::{
        KeywordHeader, MaildirFlags, add::*, info_letters, is_valid_keyword, remove::*, set::*,
    },
    maildir::{
        CUR, Maildir, MaildirSubdir, NEW, SEPARATOR, TMP, create::*, delete::*, list::*, rename::*,
    },
    path::{MaildirFsPath, MaildirPath},
    store::MaildirStore,
};

/// The count of [`MaildirYield::WantsCounter`] requests answered by this
/// process, every client included, so no two get the same number.
static COUNTER: AtomicU32 = AtomicU32::new(0);

/// Errors returned by the [`MaildirClient`] helpers.
#[derive(Debug, Error)]
pub enum MaildirClientError {
    /// The resolved path exists but is not a directory.
    #[error("path {0} is not a directory")]
    NotDir(MaildirFsPath),
    /// One of the cur/new/tmp subdirectories is missing.
    #[error("missing {0}/ subdirectory at Maildir {1}")]
    MissingSubdir(&'static str, MaildirFsPath),
    /// The name resolves outside the store or onto a subdirectory.
    #[error("Maildir load failed: `{0}` names neither the store root nor a folder below it")]
    NotAFolder(MaildirPath),
    /// The dovecot-keywords load coroutine failed.
    #[error(transparent)]
    MaildirDovecotLoad(#[from] MaildirDovecotLoadError),
    /// The dovecot-keywords store coroutine failed.
    #[error(transparent)]
    MaildirDovecotStore(#[from] MaildirDovecotStoreError),
    /// The flags-add coroutine failed.
    #[error(transparent)]
    FlagsAdd(#[from] MaildirFlagsAddError),
    /// The flags-remove coroutine failed.
    #[error(transparent)]
    FlagsRemove(#[from] MaildirFlagsRemoveError),
    /// The flags-set coroutine failed.
    #[error(transparent)]
    FlagsSet(#[from] MaildirFlagsSetError),
    /// The Maildir-create coroutine failed.
    #[error(transparent)]
    MaildirCreate(#[from] MaildirCreateError),
    /// The Maildir-delete coroutine failed.
    #[error(transparent)]
    MaildirDelete(#[from] MaildirDeleteError),
    /// The Maildir-list coroutine failed.
    #[error(transparent)]
    MaildirList(#[from] MaildirListError),
    /// The Maildir-rename coroutine failed.
    #[error(transparent)]
    MaildirRename(#[from] MaildirRenameError),
    /// The entry-copy coroutine failed.
    #[error(transparent)]
    EntryCopy(#[from] MaildirEntryCopyError),
    /// The entry-get coroutine failed.
    #[error(transparent)]
    EntryGet(#[from] MaildirEntryGetError),
    /// The entry-locate coroutine failed.
    #[error(transparent)]
    EntryLocate(#[from] MaildirEntryLocateError),
    /// The entry-locate-many coroutine failed.
    #[error(transparent)]
    EntryLocateMany(#[from] MaildirEntryLocateManyError),
    /// The entry-list coroutine failed.
    #[error(transparent)]
    EntryList(#[from] MaildirEntryListError),
    /// The entry-move coroutine failed.
    #[error(transparent)]
    EntryMove(#[from] MaildirEntryMoveError),
    /// The entry-store coroutine failed.
    #[error(transparent)]
    EntryStore(#[from] MaildirEntryStoreError),
    /// A filesystem operation failed.
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// Std-blocking Maildir client. Wraps a [`MaildirStore`] (filesystem
/// root + layout) and drives any [`MaildirCoroutine`] against
/// [`std::fs`].
#[derive(Debug)]
pub struct MaildirClient {
    /// Filesystem root + layout (fs / Maildir++).
    pub store: MaildirStore,
    /// Resolve and persist custom keywords via the `dovecot-keywords` sidecar.
    pub dovecot_keywords: bool,
    /// Header used to ferry custom keywords inline with the body.
    pub keywords_header: Option<KeywordHeader>,
    /// Header names to strip from message bytes on read.
    pub strip_headers: Vec<String>,
    /// Acknowledge the sync requests without flushing anything.
    ///
    /// Off by default: a delivery reported done is on stable storage, at
    /// the cost of two flushes each. On, a bulk import runs at the speed
    /// of the page cache, and a power cut may lose or truncate the
    /// entries delivered shortly before it.
    pub skip_sync: bool,
}

impl MaildirClient {
    /// Builds a client rooted at `root` in fs layout without filesystem
    /// checks. Flip `client.store.maildirpp = true` for Maildir++.
    ///
    /// The info separator of the store is the colon, and `;` on Windows,
    /// whose filesystems reserve the colon.
    pub fn new(root: impl Into<MaildirFsPath>) -> Self {
        Self {
            store: MaildirStore {
                root: root.into(),
                maildirpp: false,
                separator: if cfg!(windows) { ';' } else { SEPARATOR },
            },
            dovecot_keywords: false,
            keywords_header: None,
            strip_headers: Vec::new(),
            skip_sync: false,
        }
    }

    /// Drives any standard-shape coroutine to completion against [`std::fs`].
    pub fn run<C, T, E>(&self, mut coroutine: C) -> Result<T, MaildirClientError>
    where
        C: MaildirCoroutine<Yield = MaildirYield, Return = Result<T, E>>,
        MaildirClientError: From<E>,
    {
        let mut arg: Option<MaildirReply> = None;

        loop {
            match coroutine.resume(arg.take()) {
                MaildirCoroutineState::Complete(Ok(out)) => return Ok(out),
                MaildirCoroutineState::Complete(Err(err)) => return Err(err.into()),
                MaildirCoroutineState::Yielded(wants) => arg = Some(self.reply(wants)?),
            }
        }
    }

    /// Performs the I/O one coroutine step asks for against [`std::fs`].
    ///
    /// A failing filesystem call names the path it failed on, the bare
    /// [`io::Error`] naming none.
    fn reply(&self, wants: MaildirYield) -> Result<MaildirReply, MaildirClientError> {
        let reply = match wants {
            MaildirYield::WantsFileExists(paths) => {
                let mut out = BTreeMap::new();
                for path in paths {
                    let exists = fs::metadata(path.as_str())
                        .map(|m| m.is_file())
                        .unwrap_or(false);
                    trace!("file_exists {path}: {exists}");
                    out.insert(path, exists);
                }
                MaildirReply::FileExists(out)
            }
            MaildirYield::WantsDirExists(paths) => {
                let mut out = BTreeMap::new();
                for path in paths {
                    let exists = fs::metadata(path.as_str())
                        .map(|m| m.is_dir())
                        .unwrap_or(false);
                    trace!("dir_exists {path}: {exists}");
                    out.insert(path, exists);
                }
                MaildirReply::DirExists(out)
            }
            MaildirYield::WantsDirRead(paths) => {
                let mut entries = BTreeMap::new();
                for path in paths {
                    trace!("read_dir {path}");
                    let mut names = BTreeMap::new();
                    match fs::read_dir(path.as_str()) {
                        Ok(iter) => {
                            for entry in iter {
                                let entry = entry.map_err(at(&path))?;
                                let kind = file_kind(&entry);
                                names.insert(MaildirFsPath::from(entry.path()), kind);
                            }
                        }
                        Err(err) if err.kind() == io::ErrorKind::NotFound => {}
                        Err(err) => return Err(at(&path)(err)),
                    }
                    entries.insert(path, names);
                }
                MaildirReply::DirRead(entries)
            }
            MaildirYield::WantsFileRead(paths) => {
                let mut contents = BTreeMap::new();
                for path in paths {
                    trace!("read_file {path}");
                    let bytes = fs::read(path.as_str()).map_err(at(&path))?;
                    contents.insert(path, bytes);
                }
                MaildirReply::FileRead(contents)
            }
            MaildirYield::WantsFileCreate(files) => {
                for (path, contents) in files {
                    trace!("write {path} ({} bytes)", contents.len());
                    if let Some(parent) = Path::new(path.as_str()).parent() {
                        fs::create_dir_all(parent).map_err(at(&path))?;
                    }
                    fs::write(path.as_str(), &contents).map_err(at(&path))?;
                }
                MaildirReply::FileCreate
            }
            MaildirYield::WantsDirCreate(paths) => {
                for path in paths {
                    trace!("create_dir_all {path}");
                    fs::create_dir_all(path.as_str()).map_err(at(&path))?;
                }
                MaildirReply::DirCreate
            }
            MaildirYield::WantsDirRemove(paths) => {
                for path in paths {
                    trace!("remove_dir_all {path}");
                    fs::remove_dir_all(path.as_str()).map_err(at(&path))?;
                }
                MaildirReply::DirRemove
            }
            MaildirYield::WantsRename(pairs) => {
                for (from, to) in pairs {
                    trace!("rename {from} to {to}");
                    fs::rename(from.as_str(), to.as_str()).map_err(at(&from))?;
                }
                MaildirReply::Rename
            }
            MaildirYield::WantsCopy(pairs) => {
                for (from, to) in pairs {
                    trace!("copy {from} to {to}");
                    fs::copy(from.as_str(), to.as_str()).map_err(at(&from))?;
                }
                MaildirReply::Copy
            }
            MaildirYield::WantsFileSync(_) if self.skip_sync => MaildirReply::FileSync,
            MaildirYield::WantsFileSync(paths) => {
                for path in paths {
                    trace!("sync file {path}");
                    sync_file(&path).map_err(at(&path))?;
                }
                MaildirReply::FileSync
            }
            MaildirYield::WantsDirSync(_) if self.skip_sync => MaildirReply::DirSync,
            MaildirYield::WantsDirSync(paths) => {
                for path in paths {
                    trace!("sync dir {path}");
                    sync_dir(&path).map_err(at(&path))?;
                }
                MaildirReply::DirSync
            }
            MaildirYield::WantsTime => {
                let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
                MaildirReply::Time {
                    secs: ts.as_secs(),
                    nanos: ts.subsec_nanos(),
                }
            }
            MaildirYield::WantsPid => MaildirReply::Pid(process::id()),
            MaildirYield::WantsCounter => {
                MaildirReply::Counter(COUNTER.fetch_add(1, Ordering::Relaxed))
            }
            MaildirYield::WantsHostname => {
                let hostname = gethostname().into_string().unwrap_or_default();
                MaildirReply::Hostname(hostname)
            }
        };

        Ok(reply)
    }

    /// Runs [`MaildirDovecotLoad`] for `maildir`.
    pub fn load_dovecot_keywords(
        &self,
        maildir: &Maildir,
    ) -> Result<BTreeMap<char, String>, MaildirClientError> {
        self.run(MaildirDovecotLoad::new(maildir))
    }

    /// Runs [`MaildirDovecotStore`] for `maildir` with the given table.
    pub fn store_dovecot_keywords(
        &self,
        maildir: &Maildir,
        table: &BTreeMap<char, String>,
    ) -> Result<(), MaildirClientError> {
        self.run(MaildirDovecotStore::new(maildir, table))
    }

    /// Opens an existing Maildir named `name`, resolving the logical path
    /// through the store and validating its cur/new/tmp subdirs.
    ///
    /// A name [`MaildirStore::names_maildir`] rejects is refused.
    pub fn load_maildir(
        &self,
        name: impl Into<MaildirPath>,
    ) -> Result<Maildir, MaildirClientError> {
        let name = name.into();

        if !self.store.names_maildir(&name) {
            return Err(MaildirClientError::NotAFolder(name));
        }

        let root = self.store.resolve(&name);

        if !Path::new(root.as_str()).is_dir() {
            return Err(MaildirClientError::NotDir(root));
        }

        for sub in [CUR, NEW, TMP] {
            let path = root.join(sub);

            // NOTE: a folder that cannot be searched fails every stat
            // below it, which reads as a missing subdirectory unless the
            // error goes out as it is.
            match fs::metadata(path.as_str()) {
                Ok(meta) if meta.is_dir() => {}
                Ok(_) => return Err(MaildirClientError::MissingSubdir(sub, root)),
                Err(err) if is_missing(&err) => {
                    return Err(MaildirClientError::MissingSubdir(sub, root));
                }
                Err(err) => return Err(at(&path)(err)),
            }
        }

        let mut maildir = Maildir::from_path(root);
        maildir.separator = self.store.separator;

        Ok(maildir)
    }
}

/// Maildir lifecycle helpers: create, delete, list and rename whole
/// Maildirs by logical mailbox name.
impl MaildirClient {
    /// Runs [`MaildirCreate`] for the logical mailbox `name`.
    pub fn create_maildir(&self, name: impl Into<MaildirPath>) -> Result<(), MaildirClientError> {
        self.run(MaildirCreate::new(&self.store, name.into()))
    }

    /// Runs [`MaildirDelete`] for the logical mailbox `name`.
    pub fn delete_maildir(&self, name: impl Into<MaildirPath>) -> Result<(), MaildirClientError> {
        self.run(MaildirDelete::new(&self.store, name.into()))
    }

    /// Runs [`MaildirList`] under the store root; honours the store's
    /// layout flag.
    ///
    /// A folder that cannot be read is skipped with a warning naming it,
    /// so one unreadable folder does not hide the store. A symlink to a
    /// folder the walk already reached is skipped too, which keeps a link
    /// back to an ancestor from listing the same folders again and again.
    pub fn list_maildirs(&self) -> Result<BTreeSet<Maildir>, MaildirClientError> {
        let mut coroutine = MaildirList::new(&self.store);
        let mut reached = BTreeSet::new();
        let mut arg = None;

        loop {
            match coroutine.resume(arg.take()) {
                MaildirCoroutineState::Complete(Ok(out)) => return Ok(out),
                MaildirCoroutineState::Complete(Err(err)) => return Err(err.into()),
                MaildirCoroutineState::Yielded(MaildirYield::WantsDirRead(paths)) => {
                    let mut entries = BTreeMap::new();
                    for path in paths {
                        let names = read_folder(&path, &mut reached);
                        entries.insert(path, names);
                    }
                    arg = Some(MaildirReply::DirRead(entries));
                }
                MaildirCoroutineState::Yielded(wants) => arg = Some(self.reply(wants)?),
            }
        }
    }

    /// Runs [`MaildirRename`] from `from` to `to`, both logical mailbox
    /// names resolved through the store.
    pub fn rename_maildir(
        &self,
        from: impl Into<MaildirPath>,
        to: impl Into<MaildirPath>,
    ) -> Result<(), MaildirClientError> {
        self.run(MaildirRename::new(&self.store, from.into(), to.into()))
    }
}

/// Flag helpers: add, remove and set flags on an entry, resolving
/// custom keywords through the configured strategy.
impl MaildirClient {
    /// Runs [`MaildirFlagsAdd`] for `id` in `maildir`; resolves keywords
    /// through [`Self::dovecot_keywords`] if set.
    pub fn add_flags(
        &self,
        maildir: Maildir,
        id: impl ToString,
        mut flags: MaildirFlags,
    ) -> Result<(), MaildirClientError> {
        self.allocate_keywords(&maildir, &mut flags)?;
        self.run(MaildirFlagsAdd::new(maildir, id, flags))
    }

    /// Runs [`MaildirFlagsRemove`] for `id` in `maildir`; resolves keywords
    /// through [`Self::dovecot_keywords`] if set.
    ///
    /// A keyword the sidecar does not name is left alone rather than
    /// allocated a slot, nothing carrying it to begin with.
    pub fn remove_flags(
        &self,
        maildir: Maildir,
        id: impl ToString,
        mut flags: MaildirFlags,
    ) -> Result<(), MaildirClientError> {
        self.lookup_keywords(&maildir, &mut flags)?;
        self.run(MaildirFlagsRemove::new(maildir, id, flags))
    }

    /// Runs [`MaildirFlagsSet`] for `id` in `maildir`; resolves keywords
    /// through [`Self::dovecot_keywords`] if set.
    ///
    /// Replaces the whole set: unlike [`Self::add_flags`] and
    /// [`Self::remove_flags`], keywords missing from `flags` are dropped.
    pub fn set_flags(
        &self,
        maildir: Maildir,
        id: impl ToString,
        mut flags: MaildirFlags,
    ) -> Result<(), MaildirClientError> {
        self.allocate_keywords(&maildir, &mut flags)?;
        self.run(MaildirFlagsSet::new(maildir, id, flags))
    }
}

/// Entry helpers: locate, read, list, store, copy and move entries.
impl MaildirClient {
    /// Runs [`MaildirEntryLocate`] for `id` in `maildir`.
    pub fn locate(
        &self,
        maildir: Maildir,
        id: impl ToString,
    ) -> Result<(MaildirFsPath, MaildirSubdir, MaildirFlags), MaildirClientError> {
        let MaildirEntryLocateOutput {
            path,
            subdir,
            flags,
        } = self.run(MaildirEntryLocate::new(maildir, id))?;
        Ok((path, subdir, flags))
    }

    /// Runs [`MaildirEntryLocateMany`] for `ids` in `maildir`: one read
    /// of the Maildir whatever their number, the ids naming no entry
    /// left out.
    pub fn locate_many(
        &self,
        maildir: Maildir,
        ids: impl IntoIterator<Item = impl ToString>,
    ) -> Result<BTreeMap<String, MaildirEntryLocateOutput>, MaildirClientError> {
        self.run(MaildirEntryLocateMany::new(maildir, ids))
    }

    /// Runs [`MaildirEntryGet`] for `id` in `maildir`; resolves the
    /// entry's keywords through [`Self::dovecot_keywords`] and
    /// [`Self::keywords_header`].
    pub fn get(
        &self,
        maildir: Maildir,
        id: impl ToString,
    ) -> Result<MaildirFullEntry, MaildirClientError> {
        let table = self.keyword_table(&maildir);
        let mut entry = self.run(MaildirEntryGet::new(maildir, id))?;
        self.resolve_entry_keywords(&mut entry, &table);
        Ok(entry)
    }

    /// Locates entry `id` in `maildir` and permanently removes its file.
    ///
    /// Unlike [`Self::remove_flags`] (which only rewrites the flag
    /// suffix), this unlinks the message from disk. A missing entry
    /// surfaces as a locate error.
    pub fn delete_entry(
        &self,
        maildir: Maildir,
        id: impl ToString,
    ) -> Result<(), MaildirClientError> {
        let (path, _subdir, _flags) = self.locate(maildir, id)?;
        trace!("remove entry file at {path}");
        fs::remove_file(path.as_str()).map_err(at(&path))?;
        Ok(())
    }

    /// Runs [`MaildirEntryList`] on `maildir`; bodies not loaded (pair with
    /// [`Self::read_entry`] / [`Self::read_entries`] /
    /// [`Self::read_entries_par`]).
    pub fn list_entries(
        &self,
        maildir: Maildir,
    ) -> Result<BTreeSet<MaildirEntry>, MaildirClientError> {
        self.run(MaildirEntryList::new(maildir))
    }

    /// Reads `entry`'s file as a [`MaildirFullEntry`]; applies
    /// [`Self::strip_headers`] when set, and resolves the entry's
    /// keywords through [`Self::dovecot_keywords`] and
    /// [`Self::keywords_header`].
    ///
    /// `maildir` is the Maildir `entry` was listed from, holding the
    /// `dovecot-keywords` sidecar the slot letters are named by.
    pub fn read_entry(
        &self,
        maildir: &Maildir,
        entry: &MaildirEntry,
    ) -> Result<MaildirFullEntry, MaildirClientError> {
        self.read_entry_with(entry, &self.keyword_table(maildir))
    }

    /// Reads every entry sequentially into an unordered set, loading
    /// the dovecot table once for the whole batch.
    pub fn read_entries(
        &self,
        maildir: &Maildir,
        entries: &[MaildirEntry],
    ) -> Result<BTreeSet<MaildirFullEntry>, MaildirClientError> {
        let table = self.keyword_table(maildir);

        entries
            .iter()
            .map(|entry| self.read_entry_with(entry, &table))
            .collect()
    }

    /// Like [`Self::read_entries`], skipping with a warning an entry
    /// that cannot be read rather than failing the whole batch.
    ///
    /// Fits a listing, where one unreadable file should not hide the
    /// rest of the Maildir.
    pub fn read_readable_entries(
        &self,
        maildir: &Maildir,
        entries: &[MaildirEntry],
    ) -> BTreeSet<MaildirFullEntry> {
        let table = self.keyword_table(maildir);

        entries
            .iter()
            .filter_map(|entry| match self.read_entry_with(entry, &table) {
                Ok(entry) => Some(entry),
                Err(err) => {
                    warn!("skip entry: {err}");
                    None
                }
            })
            .collect()
    }

    /// Parallel variant of [`Self::read_entries`] using
    /// [`thread::available_parallelism`].
    pub fn read_entries_par(
        &self,
        maildir: &Maildir,
        entries: &[MaildirEntry],
    ) -> Result<BTreeSet<MaildirFullEntry>, MaildirClientError> {
        let table = self.keyword_table(maildir);

        if entries.len() <= 1 {
            return entries
                .iter()
                .map(|entry| self.read_entry_with(entry, &table))
                .collect();
        }

        let n_threads = thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(8)
            .min(entries.len());

        let chunk_size = entries.len().div_ceil(n_threads);

        thread::scope(
            |s| -> Result<BTreeSet<MaildirFullEntry>, MaildirClientError> {
                let mut handles = Vec::with_capacity(n_threads);

                for chunk in entries.chunks(chunk_size) {
                    let this = self;
                    let table = &table;
                    handles.push(s.spawn(
                        move || -> Result<Vec<MaildirFullEntry>, MaildirClientError> {
                            chunk
                                .iter()
                                .map(|entry| this.read_entry_with(entry, table))
                                .collect()
                        },
                    ));
                }

                let mut out = BTreeSet::new();

                for handle in handles {
                    for msg in handle.join().expect("maildir worker thread panicked")? {
                        out.insert(msg);
                    }
                }

                Ok(out)
            },
        )
    }

    /// Runs [`MaildirEntryStore`] under `subdir` of `maildir`; honours
    /// [`Self::keywords_header`] and [`Self::dovecot_keywords`] for keyword
    /// serialisation.
    pub fn store(
        &self,
        maildir: Maildir,
        subdir: MaildirSubdir,
        mut flags: MaildirFlags,
        mut contents: Vec<u8>,
    ) -> Result<(String, MaildirFsPath), MaildirClientError> {
        let keywords = flags.drain_keywords();

        if let Some(header) = self.keywords_header {
            let separator = header.separator();
            // NOTE: such a keyword would split into several corrupted
            // ones, or into header lines, so dropping beats corrupting.
            let safe: Vec<&String> = keywords
                .iter()
                .filter(|k| {
                    if is_valid_keyword(k) && !k.contains(separator) {
                        true
                    } else {
                        log::warn!(
                            "keyword {k:?} would not read back from the {} header, dropping it",
                            header.header_name()
                        );
                        false
                    }
                })
                .collect();

            if !safe.is_empty() {
                let sep = match separator {
                    ' ' => " ",
                    _ => ", ",
                };
                let value = safe
                    .iter()
                    .map(|k| k.as_str())
                    .collect::<Vec<_>>()
                    .join(sep);
                contents = inject_header(&contents, header.header_name(), &value);
            }
        }

        if self.dovecot_keywords && !keywords.is_empty() {
            let mut table = self.load_dovecot_keywords(&maildir)?;
            let original_len = table.len();
            for keyword in &keywords {
                match allocate_keyword_slot(&mut table, keyword) {
                    Some(letter) => {
                        flags.extend_letters([letter]);
                    }
                    None => {
                        log::warn!(
                            "no dovecot-keywords slot for keyword {keyword:?} at {}, dropping it",
                            maildir.path()
                        );
                    }
                }
            }
            if table.len() != original_len {
                self.store_dovecot_keywords(&maildir, &table)?;
            }
        }

        let MaildirEntryStoreOutput { id, path } =
            self.run(MaildirEntryStore::new(maildir, subdir, flags, contents))?;

        Ok((id, path))
    }

    /// Runs [`MaildirEntryCopy`] from `source` to `target`, returning
    /// the id the copy got.
    ///
    /// With [`Self::dovecot_keywords`] set, the copy's keyword letters
    /// are renamed through the target's own `dovecot-keywords` file, see
    /// [`Self::remap_keywords`].
    pub fn copy(
        &self,
        id: impl ToString,
        source: Maildir,
        target: Maildir,
        target_subdir: Option<MaildirSubdir>,
    ) -> Result<String, MaildirClientError> {
        let id = id.to_string();
        let keywords = self.entry_keywords(&source, &target, target_subdir.as_ref(), &id)?;
        let coroutine = MaildirEntryCopy::new(id, source, target.clone(), target_subdir);
        let id = self.run(coroutine)?;
        self.remap_keywords(&target, &id, keywords)?;
        Ok(id)
    }

    /// Runs [`MaildirEntryMove`] from `source` to `target`, returning
    /// the id the moved entry got.
    ///
    /// Keyword letters are renamed as [`Self::copy`] renames them.
    pub fn r#move(
        &self,
        id: impl ToString,
        source: Maildir,
        target: Maildir,
        target_subdir: Option<MaildirSubdir>,
    ) -> Result<String, MaildirClientError> {
        let id = id.to_string();
        let keywords = self.entry_keywords(&source, &target, target_subdir.as_ref(), &id)?;
        let coroutine = MaildirEntryMove::new(id, source, target.clone(), target_subdir);
        let id = self.run(coroutine)?;
        self.remap_keywords(&target, &id, keywords)?;
        Ok(id)
    }

    /// The keywords the dovecot letters of entry `id` in `source` stand
    /// for, `None` when nothing needs renaming: [`Self::dovecot_keywords`]
    /// unset, `source` and `target` one Maildir, no letter on the entry,
    /// or a delivery outside `cur/`, which carries none.
    ///
    /// Their slots are allocated in `target` on the way, so a keyword
    /// file that cannot be rewritten fails the delivery before it lands.
    fn entry_keywords(
        &self,
        source: &Maildir,
        target: &Maildir,
        target_subdir: Option<&MaildirSubdir>,
        id: &str,
    ) -> Result<Option<Vec<String>>, MaildirClientError> {
        if !self.dovecot_keywords || source.path() == target.path() {
            return Ok(None);
        }

        let (path, subdir, _) = self.locate(source.clone(), id)?;
        let entry = source.entry(path);
        let lands_in_cur = *target_subdir.unwrap_or(&subdir) == MaildirSubdir::Cur;
        if !lands_in_cur || !has_keyword_letters(&entry) {
            return Ok(None);
        }

        let table = self.keyword_table(source);
        let mut flags = MaildirFlags::with_dovecot(&entry, &table);
        let keywords = flags.drain_keywords();

        flags.extend_keywords(keywords.iter().cloned());
        self.allocate_keywords(target, &mut flags)?;

        Ok(Some(keywords))
    }

    /// Rewrites the keyword letters of entry `id` in `target` so they
    /// name `keywords` in the target's `dovecot-keywords` file,
    /// allocating slots there as a store does.
    ///
    /// A letter is an index into its own Maildir's table, so a letter
    /// carried verbatim would name another keyword, or none, in the
    /// target. A letter the source table did not name is dropped.
    fn remap_keywords(
        &self,
        target: &Maildir,
        id: &str,
        keywords: Option<Vec<String>>,
    ) -> Result<(), MaildirClientError> {
        let Some(keywords) = keywords else {
            return Ok(());
        };

        let (path, _, _) = self.locate(target.clone(), id)?;
        let entry = target.entry(path);
        if !has_keyword_letters(&entry) {
            return Ok(());
        }

        let mut flags: MaildirFlags = entry.flags().iter().cloned().collect();
        flags.extend_keywords(keywords);
        self.set_flags(target.clone(), id, flags)
    }

    /// Reads `entry`'s file, applying [`Self::strip_headers`] when set,
    /// and resolves its keywords against an already-loaded `table`.
    fn read_entry_with(
        &self,
        entry: &MaildirEntry,
        table: &BTreeMap<char, String>,
    ) -> Result<MaildirFullEntry, MaildirClientError> {
        let path = entry.path();
        trace!("read entry at {path}");

        let contents = fs::read(path.as_str()).map_err(at(path))?;
        let contents = if self.strip_headers.is_empty() {
            contents
        } else {
            let names: Vec<&str> = self.strip_headers.iter().map(String::as_str).collect();
            strip_headers(&contents, &names)
        };

        let mut entry = MaildirFullEntry::from((entry.clone(), contents));
        self.resolve_entry_keywords(&mut entry, table);

        Ok(entry)
    }

    /// Replaces `entry`'s filename flags with the same set resolved
    /// through `table` and [`Self::keywords_header`].
    fn resolve_entry_keywords(&self, entry: &mut MaildirFullEntry, table: &BTreeMap<char, String>) {
        entry.flags =
            MaildirFlags::with_keywords(&entry.entry, &entry.contents, table, self.keywords_header);
    }

    /// Dovecot slot table of `maildir`, empty when
    /// [`Self::dovecot_keywords`] is unset or the sidecar cannot be
    /// read.
    ///
    /// Unlike the store path, which fails rather than allocate slots
    /// against a table it could not load, a read degrades to unresolved
    /// keywords: a sidecar is optional, and a mailbox stays readable
    /// whatever state its own is in.
    fn keyword_table(&self, maildir: &Maildir) -> BTreeMap<char, String> {
        if !self.dovecot_keywords {
            return BTreeMap::new();
        }

        self.load_dovecot_keywords(maildir).unwrap_or_else(|err| {
            warn!(
                "could not load dovecot keywords at {}: {err}",
                maildir.path()
            );
            BTreeMap::new()
        })
    }

    /// Drains every keyword out of `flags`, replacing each with the
    /// dovecot slot letter the sidecar already names it by when
    /// [`Self::dovecot_keywords`] is set; drops it otherwise.
    ///
    /// Allocates nothing, unlike [`Self::allocate_keywords`]: a keyword
    /// the table does not name is on no message of this Maildir either,
    /// so a remove naming it has nothing to remove, and minting a slot
    /// would grow the sidecar towards its twenty-six letter ceiling one
    /// spurious remove at a time.
    fn lookup_keywords(
        &self,
        maildir: &Maildir,
        flags: &mut MaildirFlags,
    ) -> Result<(), MaildirClientError> {
        let keywords = flags.drain_keywords();
        if !self.dovecot_keywords || keywords.is_empty() {
            return Ok(());
        }

        let table = self.load_dovecot_keywords(maildir)?;
        let letters = keywords
            .iter()
            .filter_map(|keyword| keyword_slot(&table, keyword));

        flags.extend_letters(letters);

        Ok(())
    }

    /// Drains every keyword out of `flags`, allocating dovecot slots when
    /// [`Self::dovecot_keywords`] is set; drops them otherwise.
    fn allocate_keywords(
        &self,
        maildir: &Maildir,
        flags: &mut MaildirFlags,
    ) -> Result<(), MaildirClientError> {
        let keywords = flags.drain_keywords();
        if !self.dovecot_keywords || keywords.is_empty() {
            return Ok(());
        }

        let mut table = self.load_dovecot_keywords(maildir)?;
        let original_len = table.len();

        for keyword in &keywords {
            match allocate_keyword_slot(&mut table, keyword) {
                Some(letter) => {
                    flags.extend_letters([letter]);
                }
                None => {
                    log::warn!(
                        "no dovecot-keywords slot for keyword {keyword:?} at {}, dropping it",
                        maildir.path()
                    );
                }
            }
        }

        if table.len() != original_len {
            self.store_dovecot_keywords(maildir, &table)?;
        }

        Ok(())
    }
}

/// Wraps an [`io::Error`] so its message names the path it failed on,
/// its kind kept.
fn at(path: &MaildirFsPath) -> impl FnOnce(io::Error) -> MaildirClientError + '_ {
    move |err| io::Error::new(err.kind(), format!("{path}: {err}")).into()
}

/// Flushes the file at `path` to stable storage.
#[cfg(unix)]
fn sync_file(path: &MaildirFsPath) -> io::Result<()> {
    fs::File::open(path.as_str())?.sync_all()
}

/// Flushes the file at `path` to stable storage.
///
/// Flushing needs write access outside Unix, so a read-only file is
/// left as it is.
#[cfg(not(unix))]
fn sync_file(path: &MaildirFsPath) -> io::Result<()> {
    match fs::OpenOptions::new().write(true).open(path.as_str()) {
        Ok(file) => file.sync_all(),
        Err(err) if err.kind() == io::ErrorKind::PermissionDenied => Ok(()),
        Err(err) => Err(err),
    }
}

/// Flushes the directory at `path` to stable storage.
#[cfg(unix)]
fn sync_dir(path: &MaildirFsPath) -> io::Result<()> {
    fs::File::open(path.as_str())?.sync_all()
}

/// Does nothing: a directory cannot be flushed outside Unix.
#[cfg(not(unix))]
fn sync_dir(_path: &MaildirFsPath) -> io::Result<()> {
    Ok(())
}

/// Whether `err` says the path is not there, as opposed to unreachable.
fn is_missing(err: &io::Error) -> bool {
    matches!(
        err.kind(),
        io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
    )
}

/// The kind of a directory entry, read from the directory itself where
/// the platform gives it: only a symlink costs a stat, of its target.
fn file_kind(entry: &fs::DirEntry) -> MaildirFileKind {
    let file_type = match entry.file_type() {
        Ok(file_type) => file_type,
        Err(err) if is_missing(&err) => return MaildirFileKind::Missing,
        Err(_) => return MaildirFileKind::Other,
    };

    if file_type.is_file() {
        return MaildirFileKind::File;
    }

    if file_type.is_dir() {
        return MaildirFileKind::Dir;
    }

    if !file_type.is_symlink() {
        return MaildirFileKind::Other;
    }

    let path = entry.path();
    match fs::metadata(&path) {
        Ok(meta) if meta.is_file() => MaildirFileKind::File,
        Ok(meta) if meta.is_dir() => MaildirFileKind::Dir,
        Err(_) if fs::symlink_metadata(&path).is_err() => MaildirFileKind::Missing,
        _ => MaildirFileKind::Other,
    }
}

/// The children of the folder `path` for the listing walk, `reached`
/// holding the canonical folders walked so far.
///
/// An unreadable folder or child is skipped with a warning, and so is a
/// symlink to a folder already reached.
fn read_folder(
    path: &MaildirFsPath,
    reached: &mut BTreeSet<PathBuf>,
) -> BTreeMap<MaildirFsPath, MaildirFileKind> {
    trace!("read_dir {path}");
    let mut names = BTreeMap::new();

    if let Ok(canonical) = fs::canonicalize(path.as_str()) {
        reached.insert(canonical);
    }

    let iter = match fs::read_dir(path.as_str()) {
        Ok(iter) => iter,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return names,
        Err(err) => {
            warn!("skip folder {path}: {err}");
            return names;
        }
    };

    for entry in iter {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                warn!("skip an entry of folder {path}: {err}");
                continue;
            }
        };
        let child = entry.path();

        let is_link = entry.file_type().is_ok_and(|t| t.is_symlink());
        if is_link && fs::metadata(&child).is_ok_and(|m| m.is_dir()) {
            match fs::canonicalize(&child) {
                Ok(target) if reached.insert(target.clone()) => {}
                Ok(target) => {
                    let child = child.display();
                    let target = target.display();
                    warn!("skip folder {child}: it links to {target}, already listed");
                    continue;
                }
                Err(err) => {
                    warn!("skip folder {}: {err}", child.display());
                    continue;
                }
            }
        }

        names.insert(MaildirFsPath::from(child), file_kind(&entry));
    }

    names
}

/// Returns `true` when `entry` carries a dovecot keyword letter, a
/// lowercase one in its `2,` info section.
fn has_keyword_letters(entry: &MaildirEntry) -> bool {
    let name = entry.path().file_name();
    name.and_then(|name| info_letters(name, entry.separator))
        .is_some_and(|letters| letters.chars().any(|c| c.is_ascii_lowercase()))
}
