---
cairn: log
change: list-skips-plain-files
landed: 2026-11-12
---

# List skips plain files

src/maildir/list.rs probes each child it would descend into with `WantsDirExists` alongside the cur/new/tmp markers, and reads only the ones that are directories. Before, every non-dot child of an fs-layout store was read as a directory, so one `dovecot-uidlist` or `dovecot-keywords` file (the second written by io-maildir itself when `dovecot_keywords` is on) failed the whole listing with `Not a directory (os error 20)`. A Himalaya live test against a local store found it on 2026-11-12. New capability: listing (cairn/spec/listing.md).
