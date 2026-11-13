---
cairn: log
change: entry-kinds
landed: 2026-11-13
---

# Entry kinds

Two findings of the family audit of 2026-11-13 (MAILDIR-03, MAILDIR-04) and a follow-up to the flushes of change keyword-file-and-sync.

- `MaildirReply::DirRead` gives each entry with a `MaildirFileKind` (`File`, `Dir`, `Other`, `Missing`). `MaildirClient` reads the kind off the directory entry and stats a symlink only. Breaking for any other driver, which has a kind to report per entry.
- `MaildirEntryList` lists from that one answer. The stat of every name that followed the read is gone, and with it the window in which an entry renamed by another program failed its stat under the old name and was left out, unseen under the new one. A name a driver reports `Missing` has the directories read again, three reads at most, then the listing fails with the new `MaildirEntryListError::Unstable` rather than come back short.
- `MaildirEntryLocate` probes `cur/<id>` beside `new/<id>` and `tmp/<id>` and matches the read on the kinds, so a complete basename costs one request and a logical id two, where both cost three. It reads again when its only matches are gone. An exact name in `cur/` now wins over a logical match in `new/`.
- `MaildirEntriesLocate` and `MaildirClient::locate_entries` resolve any number of ids from one read of `new/`, `tmp/` and `cur/`, on the matching the single lookup uses. A test resolves a set of ids both ways and compares.
- `MaildirClient::skip_sync`, off by default, acknowledges the sync requests without flushing, for a bulk import that can deliver again what a power cut takes.

The tests count requests, not time: no measurement was made. `MaildirList`, the folder walker, takes the new answer and still probes each child.

Verification: `cargo test --all-features`, `cargo clippy --all-features --all-targets -- -D warnings`, `cargo build --no-default-features`.

Capabilities moved: entry-lookup ("A directory read carries the kind of each entry", "A name gone while read is looked for again" and "Many identifiers resolve from one read" added, "Lookup priority and candidate eligibility" modified), delivery ("A delivery reported done is flushed to stable storage" modified).
