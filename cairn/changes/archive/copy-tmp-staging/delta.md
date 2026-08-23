---
cairn: delta
change: copy-tmp-staging
---

## MODIFIED Requirements

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
