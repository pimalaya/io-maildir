---
cairn: spec
capability: delivery
status: current
---

# Entry delivery

How an entry enters a Maildir: the unique name it is given, and the steps a reader may observe while it gets there.

### Requirement: A unique name minted per delivery

Every entry entering a Maildir SHALL be given a name minted from the current time, a per-process counter, the process id and the hostname. An entry delivered from another Maildir SHALL NOT reuse the source basename: a name carries folder-specific metadata baked into it by other tools, such as mbsync's `,U=<uid>` infix, which is valid only in the folder that wrote it and would corrupt the destination's sync state or silently overwrite a same-named entry there.

#### Scenario: A copy of an mbsync entry

- GIVEN a source entry whose name carries a `,U=<uid>` infix
- WHEN it is copied into another Maildir
- THEN the delivered entry carries a freshly minted name with no `,U=` infix, and the source flags

### Requirement: Delivery is staged in tmp/

Every entry entering a Maildir, whether written from a caller's bytes or copied from another entry, SHALL be written under `tmp/` and renamed into `cur/` or `new/`. A reader enumerating the destination SHALL never observe the entry under its final name until every byte of it is there, so a process that dies mid-delivery leaves at worst a stray file in `tmp/`, which is not a message.

#### Scenario: A stored message

- GIVEN a message stored into `cur/`
- WHEN the store completes
- THEN the bytes were written under `tmp/` and renamed into `cur/`, leaving `tmp/` empty

#### Scenario: A copied message

- GIVEN an entry copied into another Maildir
- WHEN the copy completes
- THEN the bytes were copied under the target's `tmp/` and renamed into the target's `cur/`, leaving `tmp/` empty

#### Scenario: A copy interrupted mid-flight

- GIVEN a copy whose process dies between the first byte and the last
- WHEN the destination Maildir is enumerated
- THEN no entry under a final name is holding a truncated message

### Requirement: A move is a single rename

Moving an entry into another Maildir SHALL be one rename from the source path to the freshly minted destination name, so the entry exists at exactly one of the two paths at any instant.

### Requirement: The source of a copy is untouched

Copying SHALL leave the source entry byte for byte as it was, under its own name in its own Maildir, whatever happens to the destination.
