---
cairn: delta
change: keyword-file-and-sync
---

# Delta

## ADDED Requirements

Capability: delivery.

### Requirement: A delivery reported done is flushed to stable storage

A store or a copy SHALL ask the driver to flush the entry under `tmp/` (`WantsFileSync`) before the rename, and to flush the directory it was renamed into (`WantsDirSync`) after it. A move, being a rename alone, SHALL ask for the source and the target directories to be flushed after it. A delivery reported done then survives a power cut, where staging in `tmp/` alone covers a dying process. `MaildirClient` SHALL answer both requests with a sync of the file or directory; where the platform cannot flush a directory, or a read-only file, the request SHALL be acknowledged as it is. Flag writes and folder operations ask for no flush.

#### Scenario: A stored message

- GIVEN a message stored into `new/`
- WHEN the store runs
- THEN the driver is asked, in order, to write `tmp/<id>`, flush it, rename it to `new/<id>` and flush `new/`

Capability: flags.

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

## MODIFIED Requirements

Capability: flags.

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
