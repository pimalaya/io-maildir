---
cairn: spec
capability: entry-lookup
status: current
---

# Entry lookup

How a Maildir entry identifier resolves to the file used by read and mutation
operations.

### Requirement: Locate an exact logical ID or complete basename

Lookup SHALL accept equality with an entry's parsed logical ID or complete
basename, never an arbitrary prefix. It SHALL resolve suffixed entries in both
`new/` and `cur/`, returning the actual path, source subdirectory and flags
parsed from that path. Lookup and reading SHALL NOT rename entries or change
their bytes or flags. Parsing SHALL use the existing platform separator rules.

#### Scenario: Suffixed new entry listed by logical ID

- GIVEN `new/abc,U=5:2,` on Unix, or the platform-equivalent suffix
- WHEN listing's logical ID `abc,U=5` is located
- THEN the selected path is the suffixed new entry and its flags are empty

#### Scenario: Overlapping IDs

- GIVEN `cur/abc:2,S` and `cur/abc123:2,F`
- WHEN `abc` is located
- THEN only the entry with logical ID `abc` is selected
- AND if that entry is absent, lookup returns NotFound

### Requirement: Lookup priority and candidate eligibility

Lookup SHALL first accept an exact file in `new/`, then an exact file in
`tmp/`. On probe misses it SHALL inspect `new/`, then `cur/`, choosing the
lexicographically first matching confirmed file within that directory. This
priority SHALL also resolve duplicate logical IDs. It SHALL NOT scan `tmp/`.
Scanned dotfiles and non-files SHALL be ignored. Empty, dot-prefixed,
NUL-containing or platform path-separated input SHALL return NotFound before
I/O. `/` is rejected on every platform; `\` is rejected on Windows and remains
a valid Unix basename character. Windows drive/stream colon syntax SHALL
likewise be rejected.
