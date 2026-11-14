---
cairn: log
change: a-listing-returns-what-it-could-not-read
landed: 2026-11-14
---

# A listing returns what it could not read

`MaildirClient::read_readable_entries` and `MaildirClient::list_maildirs` return what they could not read next to what they did, instead of dropping it with a warning, so a caller can tell its user which message or folder a listing misses (pimalaya/himalaya, cairn/changes/a-result-carries-its-errors).

- `read_readable_entries` returns `MaildirEntriesRead { entries, unreadable }`, each unreadable entry a `MaildirUnreadableEntry { entry, error }` in the order given, its error naming the entry's path (breaking).
- `list_maildirs` returns `MaildirListing { maildirs, unreadable }`, each folder, folder entry or link the walk could not read a `MaildirUnreadableFolder { path, error }`, its `io::Error` naming the path, kind kept (breaking).
- A folder gone when read stays out silently; a link to a folder already reached stays a warning. The other skip logs moved to debug.
- `read_entries` and `read_entries_par` still fail on the first unreadable entry; the keywords `store` and `copy` drop stay warnings.
- Tests: an unreadable entry among three and a folder whose mode forbids reading (tests/store_guards.rs, skipped when run as root), a folder gone when read (unit test in src/client.rs).

Capabilities moved: listing.
