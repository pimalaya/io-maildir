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

### Requirement: A flag write on an entry in cur/ is a rename in place

Adding, removing or replacing the flags of an entry in `cur/` SHALL rename it under the same unique name with a rewritten info suffix. The rewrite SHALL preserve the dovecot slot letters standing for custom keywords, and any letter the crate does not name, so a write never erases what it did not address.

#### Scenario: Adding a flag to an entry carrying a keyword

- GIVEN an entry in `cur/` whose info suffix carries a dovecot slot letter
- WHEN a standard flag is added
- THEN the entry keeps its unique name and its slot letter, and gains the flag

### Requirement: Gaining a flag moves an entry out of new/

Flag writes SHALL accept entries in `new/` with or without informational
suffixes and operate on their actual parsed flags. Destination names SHALL use
the located logical ID, including when the caller supplied a full basename. An
unchanged resulting flag set SHALL leave a new entry untouched. A changed
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

### Requirement: A flag write never touches an entry in tmp/

An entry in `tmp/` is another process's delivery in flight. A flag write naming it SHALL complete without renaming anything, rather than claiming a half-written file.
