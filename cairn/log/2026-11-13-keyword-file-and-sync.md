---
cairn: log
change: keyword-file-and-sync
landed: 2026-11-13
---

# Keyword file and sync

Three findings of the family audit of 2026-11-13 (MAILDIR-07, MAILDIR-02, MAILDIR-09).

- `MaildirDovecotStore` reads the `dovecot-keywords` file before writing it: the new `rewrite_dovecot_keywords` replaces its slots `a..z` by the table and keeps the lines of a higher index, which the rewrite from the parsed table dropped. A file that is not UTF-8 or holds a line that is no slot is refused with the new `MaildirDovecotStoreError::Malformed`, where it parsed as empty and was replaced, handing slot 0 to the next keyword. The content is staged under `tmp/` with a minted name, flushed and renamed, instead of truncating the file in place. `keyword_slot` compares names without ASCII case.
- `MaildirClient::copy` and `r#move` allocate the target's slots before the delivery. The first version of the refusal showed why: a move into a folder with a refused file went through, then failed, the source gone.
- The vocabulary gains `WantsFileSync` and `WantsDirSync` (`FileSync`, `DirSync`). `MaildirEntryStore` and `MaildirEntryCopy` flush the entry under `tmp/` before the rename and the destination directory after it, `MaildirEntryMove` its two directories after the rename, `MaildirDovecotStore` its tmp file and the Maildir root. `MaildirClient` answers with `sync_all`; outside Unix a directory and a read-only file are acknowledged without a flush. Breaking for any other driver, which has two requests to answer.
- `MaildirFlagsSet` was checked against the spec line saying a replace preserves the slot letters. The code is what is meant: `set_flags` documents the drop and the keyword renaming of a copy is built on it. cairn/spec/flags.md now says so, and a test pins it.

Left open: no option turns the flushes off in `MaildirClient`, flag writes are not flushed, and the load and store of the keyword file take no lock.

Verification: `cargo test --all-features`, `cargo clippy --all-features --all-targets -- -D warnings`, `cargo build --no-default-features`. The code for targets other than Unix was not compiled.

Capabilities moved: delivery ("A delivery reported done is flushed to stable storage" added), flags ("The keyword file is rewritten whole or not at all" and "Keyword names are matched without case" added, "A flag write on an entry in cur/ is a rename in place" modified).
