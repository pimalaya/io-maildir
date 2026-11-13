---
cairn: change
id: keyword-file-and-sync
status: landed
created: 2026-11-13
---

# Rewrite the keyword file safely, flush deliveries, settle what a replace drops

Three findings of the family audit of 2026-11-13 (MAILDIR-07, MAILDIR-02, MAILDIR-09).

## Why

- The `dovecot-keywords` file was rewritten from the table parsed out of it, and the parser keeps slots 0 to 25 alone. A line naming a higher index, and any line it could not read, was gone after the first new keyword. A file that was not UTF-8 parsed as empty, so the new keyword took slot 0 and `a` changed meaning on every message of the folder. The write truncated the file in place. Names were compared with case, so `$Forwarded` and `$forwarded` took two slots.
- Nothing was ever flushed. A store, copy or move returned once `write` and `rename` had, so after a power cut the entry could sit under its final name empty, or not at all, behind a success the caller may have acted on by removing the source. The delivery spec promised safety against a dying process only, and no decision on durability was recorded.
- cairn/spec/flags.md said a replace preserves the dovecot slot letters; `MaildirFlagsSet` builds the name from the given set alone.

## What

- `MaildirDovecotStore` reads the file first. `rewrite_dovecot_keywords` replaces its slots `a..z` by the table and keeps every line of a higher index where it is; a file that is not UTF-8 or holds a line that is no `<index> <name>` is refused with `Malformed`, since the slots it stands for are unknown. The new content goes under the Maildir's `tmp/` with a minted name, is flushed, and is renamed over the file. Reading stays lenient. `keyword_slot` compares without ASCII case. The signature of `MaildirDovecotStore::new` does not change.
- `MaildirClient::copy` and `r#move` allocate the target's slots before running the delivery. With a refusal in the picture, allocating after it, as they did, would land the entry, or move it away, and then fail.
- The vocabulary gains `WantsFileSync` and `WantsDirSync`, each over a set of paths as the other requests are, acknowledged by `FileSync` and `DirSync`. Store and copy yield the first between the tmp write and the rename and the second, for the destination directory, after the rename; a move yields the second for its source and target directories. `MaildirClient` answers with `sync_all`.
- The replace is what is meant, so the spec moves, not the code. `MaildirClient::set_flags` has documented since 2026-08-15 that keywords missing from the set are dropped, and the keyword renaming of a copy (2026-11-13) is built on a set that drops the letters it does not name: carrying them over would leave the source's letters in the target under another meaning.

## What a driver has to add

A driver other than `MaildirClient` answers two more requests: `WantsFileSync(paths)` by flushing each file then `MaildirReply::FileSync`, and `WantsDirSync(paths)` by flushing each directory then `MaildirReply::DirSync`. One that cannot or will not flush acknowledges them as they are. `MaildirDovecotStore` also asks for requests it did not use before, all existing: `WantsFileExists`, `WantsFileRead`, `WantsTime`, `WantsPid`, `WantsHostname` and `WantsRename`.

## Out of scope

Flag writes and folder operations are not flushed: a flag lost to a power cut is not a lost message, and a flush per flag is a cost on every bulk update. Flushing cannot be turned off in `MaildirClient`; whether it should be, for a bulk import, is left to a later decision. The load and the store of the keyword file are still two steps with no lock between them, so a slot another program allocates in between is overwritten.
