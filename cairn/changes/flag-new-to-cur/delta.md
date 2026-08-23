---
cairn: delta
change: flag-new-to-cur
---

## ADDED Requirements

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
