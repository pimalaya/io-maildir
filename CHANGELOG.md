# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Added `MaildirCreate` sequential state machine coroutine that creates
  `root`, `cur`, `new` and `tmp` directories in order using `FsDirCreate`.

- Added `MaildirDelete` coroutine wrapping `FsDirRemove`.

- Added `MaildirRename` coroutine wrapping `FsRename`.

- Added `MaildirList` coroutine that lists valid Maildir entries using `FsDirRead`.

- Added `MaildirMessageLocate` coroutine to find a message file by ID across
  `cur`, `new` and `tmp` subdirectories.

- Added `MaildirMessageStore` coroutine to write a message to a Maildir subdir.

- Added `MaildirMessageGet` coroutine to read a message by ID.

- Added `MaildirMessagesList` coroutine to list all messages in a Maildir.

- Added `MaildirMessageCopy` coroutine to copy a message between Maildirs.

- Added `MaildirMessageMove` coroutine to move a message between Maildirs.

- Added `MaildirFlagsAdd`, `MaildirFlagsRemove`, `MaildirFlagsSet` coroutines.

- Added `std_store_message` example.

- Added integration test covering the full Maildir workflow.

### Changed

- Updated all coroutines to use `FsInput` / `FsOutput` split from io-fs `0.0.2`.

- Renamed coroutine modules to snake\_case matching struct names.

- Bumped MSRV to `1.87`.

- Bumped edition to `2024`.

- Bumped io-fs dependency to `0.0.2`.

## [0.0.1] - 2025-07-30

### Added

- Initiated basic coroutines.

[unreleased]: https://github.com/pimalaya/io-maildir/compare/v0.0.1...HEAD
[0.0.1]: https://github.com/pimalaya/io-maildir/compare/root..v0.0.1
