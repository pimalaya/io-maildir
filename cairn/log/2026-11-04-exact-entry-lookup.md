---
cairn: log
change: exact-entry-lookup
date: 2026-11-04
---

# Exact entry lookup and flag preservation

Lookup now accepts an exact logical ID or complete basename, scans `new/` and
`cur/` with deterministic priority, confirms candidates are files, skips
dotfiles, and rejects invalid platform path input before filesystem access.
Flag writes use the located logical ID and actual flags, including suffixed
entries in `new/`. Move and copy retain nonempty flags when the destination is
`new/`.

Capabilities updated: `entry-lookup` added, `flags` modified, and `delivery`
modified.

Validation: the full default and all-features test suites, feature builds,
clippy, formatting, documentation, missing-docs and broken-link checks passed.
Full no-default-features integration tests remain blocked by pre-existing
ungated client integration targets; no-default library tests passed. `cargo
deny` was unavailable, and Windows runtime validation was not available.
