---
cairn: tasks
change: exact-entry-lookup
---

# Implementation checklist

- [x] Review intent and lookup precedence before implementation.
- [x] Reproduce both failures against the baseline: suffixed `new/` lookup
      returns NotFound and overlapping `cur/` IDs select the wrong entry.
- [x] Match exact logical IDs or complete basenames using the existing parser.
      Preserve exact new/tmp probes, then scan new/cur with deterministic order.
- [x] Confirm scanned candidates are files, skip dotfiles and vanished entries,
      and reject invalid basename inputs before yielding I/O requests.
- [x] Return actual paths, source subdirectories and parsed flags.
- [x] Build flag-write destinations from the located logical ID. Handle flagged
      new entries, cleared flags and unchanged flags without duplicating suffixes.
- [x] Preserve nonempty flags when move/copy targets new, retaining fresh IDs,
      single-rename moves, tmp-staged copies and untouched copy sources.
- [x] Add coroutine and client regressions for exact IDs, complete basenames,
      overlapping IDs, invalid prefixes, flag preservation and read non-mutation.
      Check failed mutations leave source and destination snapshots unchanged.
- [x] Verify the patched library through Himalaya 2.2.1 on synthetic Maildirs.
- [x] Update the entry-lookup, flags and delivery specs, write the dated Cairn
      log, update the Unreleased changelog, and archive this change.
- [x] Format and validate the patch; prepare a PR description with AI disclosure.

# Validation record

Default and all-features tests passed: 106 unit tests, 8 entry-lookup client
regressions, the existing integration suites and 15 doctests. Core library
tests passed with `--no-default-features --lib`. Builds passed with no features
and with client, parser and serde individually. Clippy passed with
`--all-targets --all-features -- -D warnings`; formatting, documentation,
missing-docs, broken-doc-link, Cairn and whitespace checks passed.

Full `cargo test --no-default-features` fails in pre-existing integration
files that import the feature-gated client module. Baseline verification
confirmed that failure predates this patch. Cargo-deny and Windows runtime
validation were unavailable. Local checks used the installed Cargo toolchain
under the user's explicit override because Nix was unavailable.

Downstream synthetic CLI checks passed for list-to-read logical IDs, full
basenames, overlapping IDs, rejected prefixes, unchanged read snapshots,
flag updates, copy, move and exact deletion. Himalaya was built with the local
patched io-maildir; its temporary dependency override was restored afterward.
