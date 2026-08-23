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

An entry in `new/` carries no info suffix and therefore no flags. A flag write naming it whose resulting flag set is non-empty SHALL rename it to `cur/` under the same unique id, with that flag set as its info suffix. The id SHALL NOT change, so an identifier a caller stored stays valid across the move.

A write whose resulting flag set is empty SHALL leave the entry in `new/`: it breaks no invariant, and moving it would mark an unread message as no longer new as a side effect of a write that changed nothing.

#### Scenario: A message read as seen

- GIVEN a message in `new/`
- WHEN the `S` flag is added to it
- THEN it is renamed to `cur/<id>:2,S`, `new/` no longer holds it, and it is still found under the same id

#### Scenario: A flag removed from an unread message

- GIVEN the same message, still in `new/`
- WHEN a flag it does not carry is removed
- THEN nothing is renamed and the message stays in `new/`

### Requirement: A flag write never touches an entry in tmp/

An entry in `tmp/` is another process's delivery in flight. A flag write naming it SHALL complete without renaming anything, rather than claiming a half-written file.
