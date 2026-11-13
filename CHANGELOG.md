# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- `MaildirClient::copy` and the `MaildirEntryCopy` coroutine return the id minted for the copy instead of `()`.

- `MaildirClient::r#move` and the `MaildirEntryMove` coroutine return the id minted for the moved entry instead of `()`.

- `MaildirClient::list_maildirs` skips a folder it cannot read, and a symlink to a folder the walk already reached, with a warning instead of failing or looping.

- `MaildirCreate` and `MaildirClient::load_maildir` refuse a name holding a `.` or `..` segment, or a `cur`, `new` or `tmp` one in the fs layout, instead of resolving it.

- `allocate_keyword_slot` returns `None` for a new keyword that is empty, padded with whitespace or holding a control character.

- `MaildirEntryStore`, `MaildirEntryCopy`, `MaildirEntryMove` and `MaildirDovecotStore` yield `WantsFileSync` and `WantsDirSync`.

  A driver other than `MaildirClient` has to answer both, with `MaildirReply::FileSync` and `MaildirReply::DirSync`.

- `MaildirDovecotStore` reads the `dovecot-keywords` file, then writes it under `tmp/` and renames it into place.

- `keyword_slot` and `allocate_keyword_slot` match a keyword to a name without regard to ASCII case.

- `MaildirReply::DirRead` gives each entry of a directory with its `MaildirFileKind`, instead of the entry paths alone.

  A driver other than `MaildirClient` has to report the kind of what it lists.

- `MaildirEntryList` lists from one request, the stat of every entry gone, and `MaildirEntryLocate` no longer confirms its candidates with a second probe.

- `MaildirEntryLocate` probes `cur/<id>` along with `new/<id>` and `tmp/<id>`, so a complete basename costs one request and an exact name wins over a scanned one.

### Added

- `MaildirClient::skip_sync`, acknowledging the sync requests without flushing.

  Off by default. On, a bulk import no longer pays two flushes per delivery, and a power cut may lose the entries delivered shortly before it.

- `MaildirFileKind`, the kind of a directory entry: file, directory, other or gone while read.

- I/O-free `MaildirEntriesLocate` coroutine, locating many entries from one read of the Maildir.

- `MaildirClient::locate_entries`, running it.

- `MaildirEntryListError::Unstable`, failing a listing whose every read held a name gone meanwhile.

- `MaildirYield::WantsFileSync` and `MaildirReply::FileSync`, asking the driver to flush files to stable storage.

- `MaildirYield::WantsDirSync` and `MaildirReply::DirSync`, asking the driver to flush directories to stable storage.

- `MaildirDovecotStoreError::Malformed`, refusing to rewrite a `dovecot-keywords` file that does not parse.

- `rewrite_dovecot_keywords`, replacing the slots `a..z` of a `dovecot-keywords` content and keeping its other lines.

- `MaildirStore::names_folder`, true for a path naming a folder below the store root under the store's layout.

- `MaildirStore::names_maildir`, true for a path naming the store root or a folder below it.

- `MaildirCreateError::NotAFolder`, refusing a name that resolves outside the store or onto a Maildir subdirectory.

- `MaildirClientError::NotAFolder`, the same refusal from `load_maildir`.

- `MaildirRenameError::TargetExists`, refusing a Maildir++ rename whose child would land on an existing folder.

- `MaildirPath::names_folder`, true for a path naming a folder below the store root.

- `MaildirClient::read_readable_entries`, reading a batch of entries while skipping, with a warning, the ones that cannot be read.

### Fixed

- Fixed a listing silently leaving out an entry renamed while it ran.

  The kind of an entry comes with the directory read, so no stat pass can miss a name another program changed in between; a name still reported gone has the directories read again.

- Fixed a store, copy or move reported done before it reached stable storage.

  The entry is flushed under `tmp/` before the rename and its directory after, so a power cut no longer leaves an empty or missing message behind a success.

- Fixed a `dovecot-keywords` rewrite dropping the lines it does not read.

  Lines naming an index of 26 or more are kept in place, and a file that is not UTF-8 or holds a line that is no slot is refused rather than replaced.

- Fixed a `dovecot-keywords` file truncated in place on every rewrite: it is replaced by a rename.

- Fixed two keywords differing by case taking two `dovecot-keywords` slots.

- Fixed `cur`, `new` and `tmp` accepted as folder names in the fs layout.

  Such a name resolved onto the parent's own message directory, so deleting `new` removed the unread messages of the root. Create, load, delete and rename refuse it (`NotAFolder`); Maildir++ keeps the three names.

- Fixed a keyword holding a line break or another control character written to the `dovecot-keywords` file or the keyword header.

  It could rename a slot of the folder or add header lines to the stored message. It is dropped with a warning.

- Fixed a hostname holding `/`, `:` or NUL written as received in a minted entry name: they are written `\057`, `\072` and `\000`.

- Fixed a Maildir++ rename or delete leaving the `.<name>.*` children behind under a parent that no longer exists: they are renamed or removed with it, as the fs layout moves the subtree.

- Fixed `load_maildir` reporting a folder it may not search as missing its `cur/` subdirectory: the permission error goes out naming the path.

- Fixed `MaildirDelete` on `.` emptying the whole store: deleting or renaming a name that resolves to the store root or outside it is refused (`NotAFolder`). In Maildir++ `.` resolved to the root's parent.

- Fixed a copy or move with `dovecot_keywords` set carrying the keyword letters verbatim, so a keyword took the meaning of whatever the target's `dovecot-keywords` file numbered the same. The letters are now renamed through the target's file.

- Fixed an experimental `:1,` info section read as flag letters and rewritten as `:2,` by a flag write: only `2,` carries flags, and a flag write on any other info section is refused (`ForeignInfo`).

- Fixed filesystem errors of the client not naming the path they failed on.

- Fixed listing a store failing with `Not a directory` on any plain file beside its folders, such as `dovecot-uidlist` or the `dovecot-keywords` file io-maildir writes itself.

- Fixed the info section letters written out of ASCII order.

  A flagged and seen entry was renamed `:2,SF` instead of `:2,FS`, which the Maildir spec forbids and some readers compare literally.

- Fixed an entry id selecting another message by prefix.

  The locator matched a prefix in `cur/`, so `abc` could read, flag, move or delete `cur/abc123:2,F`, and it never scanned `new/`, so the id listed for a suffixed entry there was not found. It now accepts an exact id or complete file name only, and refuses an empty, dot-prefixed or path-like one before touching the disk.

- Fixed a flag write given a complete file name doubling its info suffix: the new name is built from the located id and the entry's actual flags.

## [0.3.1] - 2026-08-23

### Fixed

- Fixed `copy` leaving a truncated entry behind when the process died mid-copy.

  The bytes went straight to the name the destination is enumerated under, so the window between the file appearing there and its last byte arriving exposed an entry that every Maildir reader lists as an ordinary message, empty and unparsable, and that copying again does not replace. The copy now lands in the target `tmp/` and is renamed into place, as store already did, so an interrupted copy leaves at worst a stray file in `tmp/` ([himalaya#738]).

- Fixed `add_flags` and `set_flags` silently discarding the flags of an entry in `new/`.

  Both reported success and renamed nothing, so a write meant to mark a message seen left it unread on disk, and a Maildir name in `new/` carries no info suffix to hold a flag anyway. An entry in `new/` gaining a non-empty flag set now moves to `cur/<id>:2,<flags>` under the same id, so an identifier a caller stored stays valid. A write leaving it with no flag keeps it in `new/`, and `remove_flags` is unchanged for that reason. An entry in `tmp/` is still never touched, being another process's delivery in flight ([himalaya#637]).

## [0.3.0] - 2026-08-16

### Added

- Added `MaildirFullEntry::flags`, the flags of a read entry. An entry the client read carries its custom keywords resolved; one built from a path and its bytes carries the filename letters alone.

- Added `MaildirFlags::with_keywords`, `with_dovecot` plus the keywords a `KeywordHeader` carries in the message bytes. The two conventions compose: a message can name one keyword by a dovecot slot letter and another in its header, and the flag set carries both.

- Added `keyword_slot`, the non-allocating counterpart of `allocate_keyword_slot`: the letter a `dovecot-keywords` table already names a keyword by, if any.

### Changed

- `MaildirClient::read_entry`, `read_entries` and `read_entries_par` now take the `Maildir` the entries were listed from, and resolve each entry's custom keywords through `dovecot_keywords` and `keywords_header`, as `store` already did on the way out. `get` resolves them too. The `dovecot-keywords` table is loaded once per call and only when the option is on, so the default path costs no extra syscall.

  A sidecar that is absent or unreadable yields no keywords and a warning rather than failing the read: it is optional, and a mailbox stays readable whatever state its own is in. The store path still fails instead, since allocating slots against a table it could not load would corrupt the mapping.

### Fixed

- Fixed `remove_flags` growing the `dovecot-keywords` sidecar. Removing a keyword the table did not name allocated a slot for it and wrote the table back, although nothing carried it, so a sync repeatedly clearing an unset keyword leaked an entry per call and walked the folder towards its twenty-six slot ceiling. The remove path now resolves keywords against the existing table without allocating, through the new `keyword_slot`; only add and set allocate ([#4]).

- Fixed flag operations dropping the custom keywords they never named. The entry locate behind `add_flags`, `remove_flags` and `set_flags` discarded the dovecot slot letters before renaming, so any flag write erased them; the info section is now split at its `:2,` marker rather than at the last comma, which also keeps a `,S=<size>,W=<vsize>` extension in a unique part from reading as flags. A keyword holding the active header separator is dropped on store rather than written and read back as several corrupted ones ([#3]).

## [0.2.1] - 2026-08-07

### Added

- Added `MaildirClient::delete_entry`, which locates a message by id and permanently unlinks its file from disk (as opposed to `remove_flags`, which only rewrites the flag suffix). A missing entry surfaces as a locate error.

## [0.2.0] - 2026-07-16

### Changed

- Renamed `FsPath` to `MaildirFsPath` and the dovecot coroutines `DovecotLoad` / `DovecotStore` (with their error companions) to `MaildirDovecotLoad` / `MaildirDovecotStore`, so every public item carries the crate domain prefix.

- Flattened the per-module `types` submodules into their parents: `entry::types::*`, `flag::types::*` and `maildir::types::*` are now reached as `entry::*`, `flag::*` and `maildir::*`.

- Reworked library logging to the Pimalaya canon: dropped the per-resume state traces and their message prefixes, logging instead at coroutine completion.

- Realigned the README, the lib.rs header, CONTRIBUTING.md, Cargo.toml and added a docs/ folder to follow the Pimalaya documentation and naming guidelines, and documented every remaining public item.

### Fixed

- `MaildirEntryCopy` and `MaildirEntryMove` now mint a fresh unique name and preserve flags ([#1]).

  Both previously built the destination filename from the raw source basename (`{id}:2,`), which reused the source unique name verbatim (carrying folder-specific metadata baked in by other tools, notably mbsync's `,U=<uid>` infix valid only in the source folder, into the destination where it corrupts sync state and can silently overwrite a same-named entry) and dropped the source flags. Copy and move now follow the same delivery convention as `MaildirEntryStore` (a shared `mint_id` from time / pid / hostname) and carry the source flags into the target `:2,<flags>` suffix.

## [0.1.0] - 2026-06-05

### Added

- Added the `MaildirCoroutine` trait mirroring `core::ops::Coroutine`.

  Composed of `Yield` and `Return` associated types plus a two-variant `MaildirCoroutineState<Y, R>` (`Yielded(Y)` / `Complete(R)`). Every coroutine picks the shared `MaildirYield` enum (filesystem `Wants*` requests plus the three delivery inputs `WantsTime` / `WantsPid` / `WantsHostname`) and is fed back via the matching `MaildirReply` enum.

- Added the `maildir_try!` macro: coroutine equivalent of `?`.

  Advances one inner resume step, re-yields intermediate `Yielded(y)` (via `Into`), and short-circuits on `Complete(Err(_))`.

- Added I/O-free `MaildirCreate` coroutine.

  Creates `root`, `cur`, `new`, `tmp` in lexicographic order.

- Added I/O-free `MaildirDelete` coroutine.

  Recursively removes a Maildir.

- Added I/O-free `MaildirRename` coroutine.

  Renames a Maildir within its parent directory.

- Added I/O-free `MaildirList` coroutine.

  Walks every valid Maildir under a root; `MaildirList::new(root).maildirpp(true)` switches to the Maildir++ flat-dotted-siblings layout.

- Added I/O-free `MaildirEntryStore` coroutine.

  Follows the Maildir delivery protocol: writes to `/tmp` first, then atomically renames into `/cur` or `/new`, producing IDs of the shape `secs.#counter.M<nanos>P<pid>.<host>`.

- Added I/O-free `MaildirEntryGet` coroutine.

  Reads a single entry by ID and validates it against the on-disk filename.

- Added I/O-free `MaildirEntryList` coroutine.

  Scans both `/new` and `/cur` and returns every confirmed entry.

- Added I/O-free `MaildirEntryCopy` and `MaildirEntryMove` coroutines.

  Propagate an entry across Maildirs.

- Added I/O-free `MaildirEntryLocate` coroutine.

  Finds an entry file by ID across `cur`, `new` and `tmp`.

- Added I/O-free `MaildirFlagsAdd`, `MaildirFlagsRemove` and `MaildirFlagsSet` coroutines.

  Each rewrites the `:2,<flags>` suffix on the entry filename in place; custom keywords round-trip via optional `X-Keywords` / `X-Label` headers gated by per-client `keywords_header` / `strip_headers` switches.

- Added I/O-free `DovecotLoad` and `DovecotStore` coroutines.

  Read / write the `dovecot-keywords` slot table mapping `a..z` letters to user-defined keyword strings, gated by the per-client `dovecot_keywords` switch.

- Added the `FsPath` / `MaildirPath` split with `MaildirStore` as the translator.

  `FsPath` is the literal `/`-separated filesystem path; `MaildirPath` is the logical mailbox hierarchy. `MaildirStore { root: FsPath, maildirpp: bool }` resolves logical names to fs paths (`Foo/Bar` → `<root>/Foo/Bar` in fs layout, `<root>/.Foo.Bar` in Maildir++).

- Added the `client` cargo feature (default) enabling `MaildirClient`.

  Standard, blocking client backed by `std::fs` that drives any standard-Yield coroutine to completion, with high-level helpers (`create_maildir`, `delete_maildir`, `rename_maildir`, `load_maildir`) taking logical mailbox names.

- Added the `parser` cargo feature (default).

  Pulls in `mail-parser` to expose `MaildirFullEntry`, an entry paired with its parsed headers.

- Added the `serde` cargo feature (default).

  Forwards `serde` support to `mail-parser` so parsed entries can be serialized.

- Added a `# Example` `rust,no_run` block at the top of every coroutine module.

  Drives the coroutine through `MaildirClient::run` so the snippet stays self-contained and `cargo test --doc` compiles it.

[#1]: https://github.com/pimalaya/io-maildir/issues/1
[#3]: https://github.com/pimalaya/io-maildir/issues/3
[#4]: https://github.com/pimalaya/io-maildir/issues/4
[himalaya#637]: https://github.com/pimalaya/himalaya/issues/637
[himalaya#738]: https://github.com/pimalaya/himalaya/issues/738

[unreleased]: https://github.com/pimalaya/io-maildir/compare/v0.3.1..HEAD
[0.3.1]: https://github.com/pimalaya/io-maildir/compare/v0.3.0..v0.3.1
[0.3.0]: https://github.com/pimalaya/io-maildir/compare/v0.2.1..v0.3.0
[0.2.1]: https://github.com/pimalaya/io-maildir/compare/v0.2.0..v0.2.1
[0.2.0]: https://github.com/pimalaya/io-maildir/compare/v0.1.0..v0.2.0
[0.1.0]: https://github.com/pimalaya/io-maildir/compare/root..v0.1.0
