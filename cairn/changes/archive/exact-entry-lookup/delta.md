---
cairn: delta
change: exact-entry-lookup
---

## ADDED Requirements

Capability: entry-lookup.

### Requirement: Locate an exact logical ID or complete basename

Lookup SHALL accept equality with an entry's parsed logical ID or its complete
basename, never an arbitrary prefix. It SHALL resolve suffixed entries in both
`new/` and `cur/`, returning the actual path, source subdirectory, and flags
parsed from that path. Lookup and reading SHALL NOT rename entries or change
their bytes or flags. Parsing SHALL use the existing platform separator rules.

#### Scenario: Suffixed new entry listed by logical ID

- GIVEN `new/abc,U=5:2,F` on Unix, or the platform-equivalent suffix
- WHEN listing's logical ID `abc,U=5` is located
- THEN the selected path is the suffixed new entry and its flags include F

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

### Requirement: Copy and move retain flags in a New destination

Capability: delivery.

Move and copy targeting `New`, explicitly or through the source subdirectory
default, SHALL retain nonempty source flags in an informational suffix. Empty
flags SHALL retain the plain destination basename. Existing fresh-ID, atomic
move, tmp-staged copy, and source-preservation requirements SHALL still apply.

## MODIFIED Requirements

Capability: flags. Replace the existing requirement with the same title.

### Requirement: Gaining a flag moves an entry out of new/

Flag writes SHALL accept entries in `new/` with or without informational
suffixes and operate on their actual parsed flags. Destination names SHALL
use the located logical ID, including when the caller supplied a full basename.
An unchanged resulting flag set SHALL leave a new entry untouched. A changed
nonempty flag set SHALL move it to `cur/` with the same logical ID and resulting
flags. Clearing its flags SHALL leave it in `new/` under the unsuffixed logical
ID. Plain new entries whose resulting flags remain empty SHALL stay untouched.
The existing custom-keyword and unknown-letter policies SHALL be preserved.

#### Scenario: Adding Seen to plain new

- GIVEN `new/abc`
- WHEN Seen is added
- THEN it becomes `cur/abc:2,S` on Unix, with the same logical ID

#### Scenario: Clearing a flagged new entry

- GIVEN `new/abc:2,F` on Unix
- WHEN its final flag is removed or its flag set is replaced with empty
- THEN it becomes `new/abc` and remains discoverable as `abc`

#### Scenario: A complete filename used for flag updates

- GIVEN `cur/abc:2,F` on Unix
- WHEN Seen is added using the complete filename
- THEN it becomes `cur/abc:2,SF`, without a duplicated suffix

## REMOVED Requirements

None.
