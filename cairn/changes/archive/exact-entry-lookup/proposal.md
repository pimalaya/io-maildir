---
cairn: change
id: exact-entry-lookup
status: landed
created: 2026-11-04
---

# Resolve exact Maildir entries

## Why

`list_entries` returns logical ids from both `new/` and `cur/`, but the locator probed exact paths in `new/` and `tmp/` only, then matched a prefix in `cur/`. So the id `abc,U=5` listed for `new/abc,U=5:2,` could not be found, and `abc` could select `cur/abc123:2,F`. Read, delete, move, copy and the flag writes all share that locator, so the wrong message could be deleted.

## What

- The locator accepts an exact logical id or complete basename, never a prefix: exact probes of `new/` then `tmp/`, then a scan of `new/` then `cur/`, the lexicographically first confirmed file winning within each. No `tmp/` scan; dotfiles and non-files are skipped.
- Empty, dot-prefixed, NUL-containing and path-separated input is refused with `NotFound` before any I/O.
- The flag writes build the destination from the located id and read the entry's actual flags, so a complete basename never doubles the suffix and a suffixed entry another client left in `new/` is handled: a changed non-empty set moves it to `cur/`, a cleared one renames it to its bare id.

## Out of scope

Move and copy into `new/` keep dropping the flags, as cairn/spec/flags.md states; changing that is a separate decision. Lookup and the mutation after it are not atomic, and symlink containment is unchanged.
