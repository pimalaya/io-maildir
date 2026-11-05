---
cairn: delta
change: exact-entry-lookup
---

# Delta

## ADDED Requirements

Capability: entry-lookup.

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

Lookup SHALL first accept an exact file in `new/`, then an exact file in `tmp/`. On a miss it SHALL scan `new/`, then `cur/`, choosing the lexicographically first confirmed file within each, which also settles duplicate logical IDs. It SHALL NOT scan `tmp/`, and SHALL skip dotfiles and non-files.

Empty, dot-prefixed, NUL-containing or path-separated input SHALL return NotFound before any I/O: `/` on every platform, `\` and the drive or stream colon on Windows only, `\` staying a valid Unix basename character.

## MODIFIED Requirements

Capability: flags.

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
