---
cairn: delta
change: entry-kinds
---

# Delta

## ADDED Requirements

Capability: entry-lookup.

### Requirement: A directory read carries the kind of each entry

The answer to `WantsDirRead` SHALL give, for each entry, whether it is a regular file, a directory or anything else, a symlink counting as what it points to. Listing and lookup SHALL keep regular files on that answer alone, with no request per entry afterwards: a listing is one request whatever the size of the Maildir. `MaildirClient` SHALL take the kind from the directory entry where the platform gives it, and stat a symlink only.

#### Scenario: Listing a Maildir

- GIVEN a Maildir holding entries in `new/` and `cur/`, a directory and a dotfile in `cur/`
- WHEN it is listed
- THEN the regular files of `new/` and `cur/` are returned from one request

### Requirement: A name gone while read is looked for again

A driver that reads the kind of an entry after its name SHALL report an entry gone in between as `Missing`. Another program renaming an entry to change its flags does exactly that, so the entry exists under a name the read did not see. A listing SHALL then read the subdirectories again, up to three reads in all, and SHALL fail with `Unstable` when the last still holds such a name, rather than return a listing short of an entry that exists. A lookup whose only matches are gone SHALL read again likewise, and return NotFound after the last read.

#### Scenario: An entry flagged by another program during the listing

- GIVEN a read reporting `cur/abc:2,` as gone, and the next one listing `cur/abc:2,S`
- WHEN the Maildir is listed
- THEN the listing holds `cur/abc:2,S`, from two requests

### Requirement: Many identifiers resolve from one read

`MaildirEntriesLocate` and `MaildirClient::locate_entries` SHALL resolve any number of identifiers from one read of `new/`, `tmp/` and `cur/`, each to the entry the single lookup selects, `tmp/` matching by complete basename only. An identifier naming no entry SHALL be left out of the result.

#### Scenario: Four identifiers

- GIVEN a Maildir and four identifiers, one of them naming no entry
- WHEN they are located together
- THEN three entries are returned from one request

## MODIFIED Requirements

Capability: entry-lookup.

### Requirement: Lookup priority and candidate eligibility

Lookup SHALL first accept an exact file in `new/`, then an exact file in `tmp/`, then an exact file in `cur/`, all three probed in one request, so a complete basename costs no directory read. On a miss it SHALL scan `new/`, then `cur/`, choosing the lexicographically first regular file within each, which also settles duplicate logical IDs. It SHALL NOT scan `tmp/`, and SHALL skip dotfiles and non-files.

Empty, dot-prefixed, NUL-containing or path-separated input SHALL return NotFound before any I/O: `/` on every platform, `\` and the drive or stream colon on Windows only, `\` staying a valid Unix basename character.

#### Scenario: A complete basename in cur/

- GIVEN `cur/abc:2,S`
- WHEN `abc:2,S` is located, then `abc`
- THEN the first costs one request and the second two, a probe then a read of `new/` and `cur/`

Capability: delivery.

### Requirement: A delivery reported done is flushed to stable storage

A store or a copy SHALL ask the driver to flush the entry under `tmp/` (`WantsFileSync`) before the rename, and to flush the directory it was renamed into (`WantsDirSync`) after it. A move, being a rename alone, SHALL ask for the source and the target directories to be flushed after it. A delivery reported done then survives a power cut, where staging in `tmp/` alone covers a dying process. `MaildirClient` SHALL answer both requests with a sync of the file or directory; where the platform cannot flush a directory, or a read-only file, the request SHALL be acknowledged as it is. Flag writes and folder operations ask for no flush.

Flushing is the default and costs two flushes per delivery, which a bulk import or a first synchronisation of tens of thousands of messages pays in full. With `MaildirClient::skip_sync` set, the client SHALL acknowledge both requests without flushing, the coroutines still yielding them: deliveries then run at the speed of the page cache and stay safe against a dying process, and a power cut may lose or truncate the entries delivered shortly before it. The option suits a caller that can deliver again what was lost, and no other.

#### Scenario: A stored message

- GIVEN a message stored into `new/`
- WHEN the store runs
- THEN the driver is asked, in order, to write `tmp/<id>`, flush it, rename it to `new/<id>` and flush `new/`

#### Scenario: A bulk import with skip_sync

- GIVEN a client whose `skip_sync` is set
- WHEN a message is stored
- THEN the same four requests are made and the two flushes are acknowledged without touching the disk
