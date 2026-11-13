---
cairn: tasks
change: store-separator
---

# Tasks

- [x] Add `MaildirStore::separator`, `Maildir::separator`, `MaildirEntry::separator` and `Maildir::entry`, unused at first, and the `WantsCounter` request with its answer in `MaildirClient`.
- [x] Write the failing tests: a store on `;` storing, listing, flagging and copying (tests/separator.rs), the same answers minting the same name, a backslash refused off the colon in a folder name and an entry id.
- [x] Write and split every name with the separator of its Maildir; read `MaildirFlags` from a `MaildirEntry`; remove `INFORMATIONAL_SUFFIX_SEPARATOR`.
- [x] Ask the driver for the counter in the store, copy, move and keyword file coroutines; remove the atomic from the core.
- [x] Decide the reserved characters from the separator in the entry id check, the folder check and the hostname escape; remove every `cfg` on a target from the core.
- [x] Rename `MaildirEntriesLocate` to `MaildirEntryLocateMany` and `locate_entries` to `locate_many`.
- [x] Fold the delta into the flags, delivery and entry-lookup specs, write the log entry, add the changelog lines.
- [x] Run `cargo fmt`, the test suite, clippy and the build without default features.
