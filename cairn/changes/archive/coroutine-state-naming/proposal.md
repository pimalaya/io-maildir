---
cairn: change
id: coroutine-state-naming
status: landed
created: 2026-08-23
---

# Coroutine states are named after the action, not the wait

## Why

Every `State` enum in the crate named its variants after what the driver was waiting for: `AwaitTime`, `AwaitCopy`, `AwaitRename`, `AwaitProbe`, `AwaitScan`, `AwaitCreateTmp`. io-imap, the reference for coroutines across the Pimalaya libraries, names them after the action in flight instead: `Send`, `Read`, `Idle`, `FetchBaseline`, `EnableQresync`, with `Start` for the entry point. The `Display` impls carried the same posture, reading "await rename reply" where io-imap reads "send move".

A state is a place the coroutine is at, doing something. Naming it for the driver's posture describes the caller rather than the machine, and it diverges from every sibling library for no reason anyone chose.

## What

Rename the variants of all fifteen `State` enums to present-tense actions, and reword the `Display` phrases to the verb plus its object. Nothing else moves: the enums are private, no public item is touched, and no behaviour changes.

The convention is now written down as naming-013 in the organisation guidelines, so this does not have to be rediscovered per repository.
