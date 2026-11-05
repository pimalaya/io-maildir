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

Lookup SHALL first accept an exact file in `new/`, then an exact file in `tmp/`. On a miss it SHALL scan `new/`, then `cur/`, choosing the lexicographically first confirmed file within each, which also settles duplicate logical IDs. It SHALL NOT scan `tmp/`, and SHALL skip dotfiles and non-files.

Empty, dot-prefixed, NUL-containing or path-separated input SHALL return NotFound before any I/O: `/` on every platform, `\` and the drive or stream colon on Windows only, `\` staying a valid Unix basename character.
