# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Added `MaildirCreate` coroutine that creates `root`, `cur`, `new` and `tmp` directories in lexicographic order via a single `WantsDirCreate` request.
- Added `MaildirDelete` coroutine wrapping a recursive `WantsDirRemove`.
- Added `MaildirRename` coroutine wrapping `WantsRename`.
- Added `MaildirList` coroutine that lists valid Maildir entries inside a root directory using `WantsDirRead`.
- Added `MaildirMessageLocate` coroutine to find a message file by ID across `cur`, `new` and `tmp` subdirectories.
- Added `MaildirMessageStore` coroutine following the Maildir delivery protocol: write to `/tmp`, then atomically rename into the target subdir.
- Added `MaildirMessageGet` coroutine to locate a message by ID and read its contents.
- Added `MaildirMessagesList` coroutine to scan both `/new` and `/cur` of a Maildir.
- Added `MaildirMessageCopy` and `MaildirMessageMove` coroutines.
- Added `MaildirFlagsAdd`, `MaildirFlagsRemove`, `MaildirFlagsSet` coroutines (`/new` and `/tmp` messages left unchanged).
- Added `MaildirClient` standard blocking wrapper behind the `client` feature.
- Added `std_store_message` example.
- Added integration test covering the full Maildir workflow.

### Changed

- Dropped the `io-fs` dependency. Each coroutine now emits its own `Wants*` variants directly (`WantsDirCreate`, `WantsFileRead`, `WantsRename`, …) and consumes a local `*Arg` variant fed back by the caller.
- Result variant shapes follow the arity rule throughout: 0 fields → unit, 1 field → tuple, ≥2 fields → struct.
- All low-level logging is now via `trace!`; no more `info!` / `debug!` / `warn!` / `error!` in this crate.
- Renamed coroutine modules to snake\_case matching struct names.
- Set `default = ["client"]` so the std blocking client is enabled out of the box; disable default features to use the coroutines only.
- Bumped MSRV to `1.87`.
- Bumped edition to `2024`.

## [0.0.1] - 2025-07-30

### Added

- Initiated basic coroutines.

[unreleased]: https://github.com/pimalaya/io-maildir/compare/v0.0.1...HEAD
[0.0.1]: https://github.com/pimalaya/io-maildir/compare/root..v0.0.1
