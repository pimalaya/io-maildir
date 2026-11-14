---
cairn: change
id: a-listing-returns-what-it-could-not-read
status: landed
created: 2026-11-14
---

# A listing returns what it could not read

## Why

Two client methods keep one unreadable item from hiding the rest, and then drop it with a log line (spec listing, "One unreadable folder does not stop the listing"):

- `MaildirClient::read_readable_entries` skips an entry it cannot read, logging `skip entry: <error>`.
- `MaildirClient::list_maildirs` skips, through `read_folder`, a folder it cannot read, an entry of a folder it cannot read, and a folder link it cannot resolve, logging each.

A caller cannot tell its user that the listing is incomplete, nor which message or folder is missing. Himalaya is making each listing report the items it could not read, next to the ones it did (pimalaya/himalaya, cairn/changes/a-result-carries-its-errors).

## What

- `read_readable_entries` returns `MaildirEntriesRead { entries, unreadable }`. `entries` holds the full entries read, as today. `unreadable` is a `Vec<MaildirUnreadableEntry>`, `MaildirUnreadableEntry { entry: MaildirEntry, error: MaildirClientError }`, in the order the entries were given.
- `list_maildirs` returns `MaildirListing { maildirs, unreadable }`. `maildirs` holds the Maildirs found, as today. `unreadable` is a `Vec<MaildirUnreadableFolder>`, `MaildirUnreadableFolder { path: MaildirFsPath, error: io::Error }`, one per folder, folder entry or link the walk could not read.
- A folder missing when read (`NotFound`, gone since its parent was read) and a link back to a folder the walk already reached are no failures. The first stays out silently as today; the second stays a warning, being the walk's own guard.
- The logs move to debug, since the caller now reports what failed.

## Scope / non-goals

- `read_entries` and `read_entries_par` keep failing on the first unreadable entry, for a caller that wants all or nothing.
- The keywords `store` and `copy` drop when no `dovecot-keywords` slot or header can hold them stay warnings. A later change can return them the same way.

## Decisions

- **Breaking.** Both methods change their return type. This is a 0.x minor release, and the owner decides the version.
