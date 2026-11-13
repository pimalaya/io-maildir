---
cairn: spec
capability: entry-lookup
status: current
---

# Entry lookup

How a Maildir entry identifier resolves to the file that read and mutation operations act on.

### Requirement: Locate an exact logical ID or complete basename

Lookup SHALL accept an entry's parsed logical ID or its complete basename, never an arbitrary prefix. It SHALL resolve suffixed entries in both `new/` and `cur/`, returning the actual path, source subdirectory and the flags parsed from that path. Lookup and reading SHALL NOT rename an entry or change its bytes or flags.

#### Scenario: Suffixed new entry listed by logical ID

- GIVEN `new/abc,U=5:2,` on Unix, or the platform-equivalent suffix
- WHEN the logical ID `abc,U=5` that listing returns is located
- THEN the selected path is the suffixed new entry and its flags are empty

#### Scenario: Overlapping IDs

- GIVEN `cur/abc:2,S` and `cur/abc123:2,F`
- WHEN `abc` is located
- THEN only the entry with logical ID `abc` is selected
- AND if that entry is absent, lookup returns NotFound

### Requirement: Lookup priority and candidate eligibility

Lookup SHALL first accept an exact file in `new/`, then an exact file in `tmp/`, then an exact file in `cur/`, all three probed in one request, so a complete basename costs no directory read. On a miss it SHALL scan `new/`, then `cur/`, choosing the lexicographically first regular file within each, which also settles duplicate logical IDs. It SHALL NOT scan `tmp/`, and SHALL skip dotfiles and non-files.

Empty, dot-prefixed, NUL-containing or path-separated input SHALL return NotFound before any I/O: `/` on every platform, `\` and the drive or stream colon on Windows only, `\` staying a valid Unix basename character.

#### Scenario: A complete basename in cur/

- GIVEN `cur/abc:2,S`
- WHEN `abc:2,S` is located, then `abc`
- THEN the first costs one request and the second two, a probe then a read of `new/` and `cur/`

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
