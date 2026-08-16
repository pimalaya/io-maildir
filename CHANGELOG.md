# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

[unreleased]: https://github.com/pimalaya/io-maildir/compare/v0.2.1..HEAD
[0.2.1]: https://github.com/pimalaya/io-maildir/compare/v0.2.0..v0.2.1
[0.2.0]: https://github.com/pimalaya/io-maildir/compare/v0.1.0..v0.2.0
[0.1.0]: https://github.com/pimalaya/io-maildir/compare/root..v0.1.0
