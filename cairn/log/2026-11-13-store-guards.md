---
cairn: log
change: store-guards
landed: 2026-11-13
---

# Store guards

Fixes from the Himalaya Maildir test campaign of 2026-11-13 (report MD).

- `MaildirDelete` and `MaildirRename` refuse a name resolving to the store root or outside it (`NotAFolder`, through the new `MaildirPath::names_folder`): deleting `.` used to empty the whole store before the final `rmdir` failed, and in Maildir++ `.` resolved to the root's parent.
- Only a `2,` info section is read as flags; the flag coroutines refuse an entry with any other info section (`ForeignInfo`) instead of rewriting `:1,experimental` as `:2,<letters>`.
- `MaildirClient::copy` and `MaildirClient::r#move` rename dovecot keyword letters through the target's `dovecot-keywords` when `dovecot_keywords` is set; `MaildirEntryMove` and `r#move` return the minted id (breaking).
- `list_maildirs` skips an unreadable folder and a symlink back to a reached folder with a warning; `read_readable_entries` skips an unreadable entry; every client I/O error names its path.

Capabilities moved: delivery, flags, listing.
