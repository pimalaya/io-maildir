---
cairn: log
change: exact-entry-lookup
landed: 2026-11-05
---

# Exact entry lookup

src/entry/locate.rs matches an exact logical id or complete basename instead of a prefix: exact probes of `new/` then `tmp/`, then a scan of `new/` then `cur/` whose candidates are confirmed as files through `WantsFileExists`, invalid input refused before any I/O. The flag writes in src/flag/ build their destination from the located id and the entry's actual flags, so a complete basename no longer doubles the suffix and a suffixed entry in `new/` is handled.

The contribution also kept the flags of an entry moved or copied into `new/`. The maintainer dropped that part before landing: cairn/spec/flags.md states that what io-maildir writes into `new/` carries no flags, so move and copy keep dropping them there.

Verification: `cargo test --all-features` (unit, the new tests/entry_lookup.rs, the existing integration suites and doctests), `cargo clippy --all-targets --all-features -- -D warnings`, `cargo fmt --check`. Windows was not exercised.

Capabilities moved: **entry-lookup** (added), **flags** (gaining a flag moves an entry out of new/: modified).
