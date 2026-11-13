---
cairn: tasks
change: entry-kinds
---

# Tasks

- [x] Add `MaildirClient::skip_sync`, with a test driving a coroutine that asks for both syncs of a missing path.
- [x] Add `MaildirFileKind` and carry it in `MaildirReply::DirRead`; report it from `MaildirClient` without a stat per entry.
- [x] Rewrite the listing and lookup tests to count requests and to feed kinds, and see them fail on the stat pass and the second probe.
- [x] List from the kinds alone in `MaildirEntryList`, reading again on `Missing` and failing with `Unstable` after three reads.
- [x] Probe `cur/<id>` in `MaildirEntryLocate`, match on the kinds, read again when the only matches are gone.
- [x] Add `MaildirEntriesLocate` and `MaildirClient::locate_entries` on the matching the single lookup uses, with a test resolving each id both ways.
- [x] Adapt the flag, copy, move and folder coroutines and their tests to the new answer.
- [x] Fold the delta into the entry-lookup and delivery specs, write the log entry, add the changelog lines.
- [x] Run `cargo fmt`, the test suite, clippy and the build without default features.
