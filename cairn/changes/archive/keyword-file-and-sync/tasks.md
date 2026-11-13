---
cairn: tasks
change: keyword-file-and-sync
---

# Tasks

- [x] Add `WantsFileSync`, `WantsDirSync`, `FileSync` and `DirSync` to the vocabulary and answer them in `MaildirClient`.
- [x] Make the flow tests of `MaildirEntryStore`, `MaildirEntryCopy` and `MaildirEntryMove` expect the flushes, see them fail, then yield the requests.
- [x] Reproduce the keyword file findings with failing tests (tests/keyword_file.rs and the `MaildirDovecotStore` unit tests): lines beyond slot 25 dropped, an unparsable file replaced, a direct write to the file, two slots for one name in two cases.
- [x] Add `rewrite_dovecot_keywords` and `MaildirDovecotStoreError::Malformed`; read, stage under `tmp/`, flush and rename in `MaildirDovecotStore`; compare names without case in `keyword_slot`.
- [x] Allocate the target's slots before a copy or a move, after a failing test showed a refused file letting the move through.
- [x] Check `MaildirFlagsSet` against the spec, the client documentation and the keyword renaming of a copy; pin the replace with a test and correct the spec.
- [x] Fold the delta into the delivery and flags specs, write the log entry, add the changelog lines.
- [x] Run `cargo fmt`, the test suite, clippy and the build without default features.
