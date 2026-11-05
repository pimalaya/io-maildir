---
cairn: tasks
change: exact-entry-lookup
---

# Tasks

- [x] Reproduce both failures: a suffixed `new/` entry not found by its listed id, an overlapping `cur/` id selecting another entry.
- [x] Match exact logical ids or complete basenames, with the probe and scan order of the proposal.
- [x] Confirm scanned candidates are files, skip dotfiles, refuse invalid input before any I/O.
- [x] Build flag-write destinations from the located id and its actual flags.
- [x] Coroutine and client tests for exact ids, complete basenames, overlapping ids, refused prefixes, flagged `new/` entries and read non-mutation.
- [x] Fold the delta into the entry-lookup and flags specs, write the log entry, add the changelog lines.
