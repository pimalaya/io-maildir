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

### Requirement: A flag write never touches an entry in tmp/

An entry in `tmp/` is another process's delivery in flight. A flag write naming it SHALL complete without renaming anything, rather than claiming a half-written file.
