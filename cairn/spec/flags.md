---
cairn: spec
capability: flags
status: current
---

# Entry flags

How the flags of an entry are carried in its filename, and what a flag write does to the entry on disk.

### Requirement: Flags are carried in the filename info suffix

Flags SHALL be carried as letters following the `:2,` marker of the entry filename, in sorted order, so the same flag set always renders as the same name. The info section SHALL be split at that marker rather than at the last comma, so a unique part carrying a `,S=<size>,W=<vsize>` extension is not read as flags.

#### Scenario: A size extension in the unique part

- GIVEN an entry named with a `,S=<size>,W=<vsize>` extension before its `:2,` marker
- WHEN its flags are read
- THEN only the letters after the marker are read as flags

### Requirement: The info separator belongs to the store

The character opening the info section SHALL be the `separator` of the `MaildirStore`, the colon unless set otherwise, and every `Maildir` and `MaildirEntry` coming from a store SHALL carry it. Names SHALL be written and split with that character alone, whatever the platform the crate was built for: the core holds no target-specific code, so it builds for any target, and a store kept with `;` or `!` on a filesystem reserving the colon is read the same everywhere. A handle built by hand (`Maildir::from_path`, `MaildirEntry::from_path`) SHALL take the colon. `MaildirClient::new` SHALL set `;` on Windows and the colon elsewhere.

A store whose separator is not the colon sits on a filesystem that reserves the colon and takes the backslash as a path separator. In such a store an entry id holding `\` or `:` SHALL be refused as NotFound before any I/O, and a folder name holding `\` SHALL be refused with `NotAFolder`.

#### Scenario: A store kept with a semicolon

- GIVEN a store whose separator is `;`
- WHEN a seen message is stored, listed, flagged and copied
- THEN it is named `<id>;2,S` then `<id>;2,FS`, listed under `<id>` with its flags, and its copy is named `<copy>;2,FS`

#### Scenario: A foreign separator

- GIVEN `cur/1.host;2,S` in a store whose separator is the colon
- WHEN it is listed
- THEN its id is `1.host;2,S` and it carries no flag

### Requirement: A flag write on an entry in cur/ is a rename in place

Adding, removing or replacing the flags of an entry in `cur/` SHALL rename it under the same unique name with a rewritten info suffix. An add or a remove SHALL preserve the dovecot slot letters standing for custom keywords, and any letter the crate does not name, so it never erases what it did not address. A replace SHALL write the given set alone: every letter the entry carried and the set does not name is dropped, slot letters included, which is how a replace removes a keyword and how a copy renames its letters.

#### Scenario: Adding a flag to an entry carrying a keyword

- GIVEN an entry in `cur/` whose info suffix carries a dovecot slot letter
- WHEN a standard flag is added
- THEN the entry keeps its unique name and its slot letter, and gains the flag

#### Scenario: Replacing the flags of an entry carrying letters

- GIVEN `cur/abc:2,FXab` on Unix
- WHEN its flags are replaced by seen and the slot letter `c`
- THEN it becomes `cur/abc:2,Sc`

### Requirement: Gaining a flag moves an entry out of new/

An entry io-maildir writes into `new/` carries no info suffix and therefore no flags; one left there with a suffix by another client is read with the flags it carries. A flag write naming an entry in `new/` whose flag set changes and stays non-empty SHALL rename it to `cur/` under the same unique id, with that flag set as its info suffix. The id SHALL NOT change, so an identifier a caller stored stays valid across the move.

A write whose resulting flag set is unchanged SHALL leave the entry in `new/`: moving it would mark an unread message as no longer new as a side effect of a write that changed nothing. A write clearing the flags of a suffixed entry in `new/` SHALL rename it to its bare id there. The destination name SHALL be built from the located id, never from the caller's input, so a complete file name never doubles the suffix.

#### Scenario: A message read as seen

- GIVEN a message in `new/`
- WHEN the `S` flag is added to it
- THEN it is renamed to `cur/<id>:2,S`, `new/` no longer holds it, and it is still found under the same id

#### Scenario: A flag removed from an unread message

- GIVEN the same message, still in `new/`
- WHEN a flag it does not carry is removed
- THEN nothing is renamed and the message stays in `new/`

#### Scenario: Clearing a suffixed entry in new/

- GIVEN `new/abc:2,F` on Unix, left by another client
- WHEN its last flag is removed
- THEN it becomes `new/abc` and is still found as `abc`

#### Scenario: A complete file name used for a flag write

- GIVEN `cur/abc:2,F` on Unix
- WHEN `F` is removed using the complete file name `abc:2,F`
- THEN it becomes `cur/abc:2,`, the suffix written once

### Requirement: A flag write never touches an entry in tmp/

An entry in `tmp/` is another process's delivery in flight. A flag write naming it SHALL complete without renaming anything, rather than claiming a half-written file.

### Requirement: Only a version 2 info section carries flags

Only an info section starting `2,` SHALL be read as flags. Any other info section, such as an experimental `:1,` one, SHALL carry no flag, and a flag write on such an entry SHALL be refused (`ForeignInfo`), the entry left as it is, since rebuilding its name as `<id>:2,<flags>` would lose it.

#### Scenario: An experimental info section

- GIVEN an entry named `1700000008.M8P8.host:1,experimental` and a `dovecot-keywords` file naming `a`
- WHEN its flags are read, then a flag is added to it
- THEN it carries no flag, the write is refused, and the file keeps its name

### Requirement: A copy or move keeps the keywords, not the letters

With `dovecot_keywords` set, a copy or a move across Maildirs SHALL rename the keyword letters of the delivered entry through the target's own `dovecot-keywords` file, allocating slots there as a store does, so every keyword keeps its meaning. A letter the source file does not name SHALL be dropped. Without `dovecot_keywords`, the letters SHALL be carried verbatim.

#### Scenario: Two Maildirs numbering their keywords apart

- GIVEN a source naming `a` `$Important` and `c` `Project X`, a target naming `a` `$Junk`, and an entry `:2,FSac` in the source
- WHEN it is copied to the target
- THEN the copy reads `\Flagged \Seen $Important Project X` and the target file still names `a` `$Junk`

### Requirement: A keyword that would not read back is not written

A keyword SHALL reach the `dovecot-keywords` file or the keyword header only when it reads back as written: not empty, not padded with whitespace, holding no control character and, for the header, not holding the header's separator. Any other keyword SHALL be dropped with a warning and the write SHALL go on without it: it gets no slot and no letter, and the header does not name it. A name the `dovecot-keywords` file already holds SHALL keep its slot whatever it holds, so a file another tool wrote is never renumbered. The serialiser SHALL skip a name holding a line feed.

#### Scenario: A keyword holding a line break

- GIVEN a `dovecot-keywords` file naming `a` `$Junk`
- WHEN a message is stored with the keywords `Work` and `x<LF>0 $NotJunk`
- THEN the file reads `0 $Junk` then `1 Work`, and the message carries `Work` alone

#### Scenario: A keyword holding a header line

- GIVEN a client writing keywords to `X-Keywords`
- WHEN a message is stored with the keywords `Work` and `x<CR><LF>Bcc: eve@example.org`
- THEN the message gains `X-Keywords: Work` and no other line

### Requirement: The keyword file is rewritten whole or not at all

`MaildirDovecotStore` SHALL read the `dovecot-keywords` file before writing it, and replace only its slots `a` to `z` (indices 0 to 25) by the given table, keeping in place every line naming a higher index. A file that is not UTF-8, or holds a non-blank line that is not `<index> <name>`, SHALL be refused (`Malformed`) and left as it is, nothing written: the slots such a line stands for are unknown, so no letter is safe to hand out. Reading stays lenient and skips such a line. The new content SHALL be written under the Maildir's `tmp/` with a minted name, flushed, and renamed over the file, so a reader sees the old file or the new one. A copy or a move SHALL allocate the target's slots before the delivery, so a refused file fails it before anything lands.

#### Scenario: A file holding indices beyond the letters

- GIVEN a `dovecot-keywords` file reading `0 $Junk`, `26 Beyond`, `30 Far`
- WHEN a message is stored with the new keyword `Work`
- THEN the file reads `0 $Junk`, `26 Beyond`, `30 Far`, `1 Work`

#### Scenario: A file holding a stray line

- GIVEN a `dovecot-keywords` file reading `0 $Junk` then `not a slot`, and an entry `:2,a`
- WHEN the entry is read, then a message is stored with the new keyword `Work`
- THEN the entry reads `$Junk`, the store is refused, and neither the file nor the Maildir changed

### Requirement: Keyword names are matched without case

A keyword SHALL be matched to a name the `dovecot-keywords` file holds without regard to ASCII case, as IMAP compares keywords, so `$Forwarded` and `$forwarded` share one slot. The file keeps the spelling it holds.

#### Scenario: Adding a keyword in another case

- GIVEN a `dovecot-keywords` file naming `a` `$Forwarded`
- WHEN `$forwarded` is added to an entry
- THEN the entry gains the letter `a` and the file is not rewritten
