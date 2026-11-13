---
cairn: log
change: store-separator
landed: 2026-11-13
---

# Store separator

One finding of the family audit of 2026-11-13 (MAILDIR-05), the two Windows points change name-guards left open, and the name of the batch lookup.

- The info separator is `MaildirStore::separator`, the colon by default, carried by `Maildir` and `MaildirEntry` (through the new `Maildir::entry`) to every name written or split. `INFORMATIONAL_SUFFIX_SEPARATOR`, a static defined for Unix and Windows alone, is removed, so the core compiles on a target that is neither, and a store kept with `;` or `!` is read with its flags on any platform. `MaildirFlags` is read from a `MaildirEntry` (`From<&MaildirEntry>`, `with_dovecot`, `with_keywords`) and `MaildirFullEntry` built from one, both breaking. `MaildirClient::new` sets `;` on Windows.
- The counter of a minted name is asked of the driver (`WantsCounter`, `Counter`) instead of read from a process-wide atomic in the core. A test stores twice on the same answers and gets `1.#4M2P3.host` twice, where the atomic gave two names. `MaildirClient` answers from its own atomic. Breaking for any other driver, which has one request to answer.
- What `cfg!(windows)` decided the store's separator decides: off the colon, an entry id holding `\` or `:` is refused, a folder name holding `\` is refused by `MaildirStore::names_folder`, and the hostname is escaped with `_` rather than an octal sequence behind a backslash. No `cfg` on a target and no static is left in the core.
- `MaildirEntriesLocate` becomes `MaildirEntryLocateMany` and `MaildirClient::locate_entries` becomes `locate_many`, so the type agrees with its module `entry::locate_many` as every other coroutine of the crate agrees with its file. Change entry-kinds and its log entry name it by the old name.
- `MaildirFlagsRemove` builds its `cur/` target from the Maildir as add and set do, where it rebuilt it from the path of the entry.

Verification: `cargo test --all-features`, `cargo clippy --all-features --all-targets -- -D warnings`, `cargo build --no-default-features`, `cargo clippy --no-default-features -- -D warnings`. The development shell installs the host target alone, so no build for wasm32 or another target was made; the search for `cfg` on a target and for statics in the core returns nothing outside the std client and the `PathBuf` conversions of the `client` feature.

Capabilities moved: flags ("The info separator belongs to the store" added), delivery ("A unique name minted per delivery" modified), entry-lookup ("Lookup priority and candidate eligibility" and "Many identifiers resolve from one read" modified).
