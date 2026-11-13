---
cairn: delta
change: store-separator
---

# Delta

## ADDED Requirements

Capability: flags.

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

## MODIFIED Requirements

Capability: delivery.

### Requirement: A unique name minted per delivery

Every entry entering a Maildir SHALL be given a name minted from the current time, the process id, a counter and the hostname, each asked of the driver (`WantsTime`, `WantsPid`, `WantsCounter`, `WantsHostname`): the core keeps no state of its own, so the same answers give the same name. The driver SHALL answer `WantsCounter` with a number it gave to no other request in the process, which is what keeps two deliveries of one clock tick apart. A character of the hostname the name cannot hold (`/`, NUL and the info separator) SHALL be written in octal after a backslash (`\057`, `\000`, `\072` for the colon), so a hostname adds neither a path segment nor an info section to the name; in a store whose separator is not the colon, where the backslash is reserved too, it SHALL be written `_`, as `\` and `:` are. An entry delivered from another Maildir SHALL NOT reuse the source basename: a name carries folder-specific metadata baked into it by other tools, such as mbsync's `,U=<uid>` infix, which is valid only in the folder that wrote it and would corrupt the destination's sync state or silently overwrite a same-named entry there. A copy SHALL return the id it minted, so the caller can name the delivered entry.

#### Scenario: A copy of an mbsync entry

- GIVEN a source entry whose name carries a `,U=<uid>` infix
- WHEN it is copied into another Maildir
- THEN the delivered entry carries a freshly minted name with no `,U=` infix, and the source flags

#### Scenario: A hostname holding a slash and a colon

- GIVEN the hostname `mail/host:25`
- WHEN an entry is delivered
- THEN its name ends `.mail\057host\07225`

#### Scenario: The same answers twice

- GIVEN a driver answering time `1` and `2` nanoseconds, pid `3`, counter `4` and hostname `host`
- WHEN two entries are stored
- THEN both are named `1.#4M2P3.host`

Capability: entry-lookup.

### Requirement: Lookup priority and candidate eligibility

Lookup SHALL first accept an exact file in `new/`, then an exact file in `tmp/`, then an exact file in `cur/`, all three probed in one request, so a complete basename costs no directory read. On a miss it SHALL scan `new/`, then `cur/`, choosing the lexicographically first regular file within each, which also settles duplicate logical IDs. It SHALL NOT scan `tmp/`, and SHALL skip dotfiles and non-files.

Empty, dot-prefixed, NUL-containing or path-separated input SHALL return NotFound before any I/O: `/` in every store, `\` and the drive or stream colon too in a store whose info separator is not the colon, `\` staying a valid basename character on the colon.

#### Scenario: A complete basename in cur/

- GIVEN `cur/abc:2,S`
- WHEN `abc:2,S` is located, then `abc`
- THEN the first costs one request and the second two, a probe then a read of `new/` and `cur/`

### Requirement: Many identifiers resolve from one read

`MaildirEntryLocateMany` and `MaildirClient::locate_many` SHALL resolve any number of identifiers from one read of `new/`, `tmp/` and `cur/`, each to the entry the single lookup selects, `tmp/` matching by complete basename only. An identifier naming no entry SHALL be left out of the result.

#### Scenario: Four identifiers

- GIVEN a Maildir and four identifiers, one of them naming no entry
- WHEN they are located together
- THEN three entries are returned from one request
