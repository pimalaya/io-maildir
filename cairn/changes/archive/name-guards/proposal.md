---
cairn: change
id: name-guards
status: landed
created: 2026-11-13
---

# Validate folder names, keywords and the hostname

Three findings of the family audit of 2026-11-13 (MAILDIR-01, MAILDIR-08, MAILDIR-10), all of one kind: a string the caller or the host supplies reaches a filesystem name or a file format as received.

## Why

- In the fs layout a folder sits beside the `cur`, `new` and `tmp` of its parent, so a folder named after one of them resolves onto the parent's own messages. `create_maildir("new")` nested a Maildir inside the root's `new/`, the listing hid it, and `delete_maildir("new")` removed every unread message of the root. Only delete and rename checked the name at all, and only for the root and `..`: `MaildirCreate` and `load_maildir` took `..` too.
- A keyword went to the `dovecot-keywords` file and to the keyword header unchecked. `x<LF>0 y` added a line renaming slot 0 for the whole folder; one holding CR LF added header lines to the stored message, or ended its header block.
- The hostname went into the minted name as received. One holding `/` added a path segment under `tmp/`, one holding `:` gave an entry whose id is cut at the colon and which every flag write refuses.

## What

- `MaildirStore::names_folder` is the one layout-aware check: a segment at least, none of them `.` or `..`, nor `cur`, `new` or `tmp` in the fs layout. `MaildirStore::names_maildir` accepts the root as well. `MaildirDelete` and `MaildirRename` use the first, `MaildirCreate` and `MaildirClient::load_maildir` the second, each refusing with `NotAFolder` before any filesystem call. `MaildirStore::resolve` stays infallible, and Maildir++ keeps the three names, which resolve to `.cur`, `.new` and `.tmp`.
- One keyword check (not empty, not padded with whitespace, no control character) stands before both sinks: `allocate_keyword_slot` gives no slot to a new keyword failing it, and the header injection of `MaildirClient::store` drops it along with a keyword holding the header separator. Dropped means a warning and the write going on, as a full slot table already does. A name the file already holds keeps its slot whatever it holds, so a foreign file is never renumbered. `serialize_dovecot_keywords` skips a name holding a line feed, for a table built by hand.
- `mint_id` writes the `/`, `:` and NUL of the hostname as `\057`, `\072` and `\000`, the first two as the Maildir naming rules ask.

## Out of scope

Names are compared as written: on a case-insensitive filesystem `New` still resolves onto `new`, and refusing it everywhere would take a plausible folder name away from every other system. On Windows `\` is a path separator, which neither the folder check nor the backslash of the hostname escape accounts for; both belong with the decision on the info separator as a store option.
