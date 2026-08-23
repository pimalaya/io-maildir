---
cairn: tasks
change: flag-new-to-cur
---

- [x] Keep the `Maildir` on `MaildirFlagsSet` and `MaildirFlagsAdd`, cloning it into the inner locate, so the destination directory is reachable at rename time
- [x] Replace the parent-preserving `rename_with_flags` in both with a `cur/`-anchored builder, which serves the `cur/` and `new/` branches alike
- [x] Rewrite the subdir match in both: `tmp/` is a no-op, `new/` with no resulting flag is a no-op, `cur/` and flagged `new/` rename, each arm carrying a NOTE for why
- [x] Leave `MaildirFlagsRemove` alone: removing from `new/` always resolves to an empty flag set
- [x] Restate the "No-op on /new and /tmp" line on the set and add type docs
- [x] Replace `new_subdir_returns_noop_ok` in both with `new_subdir_renames_into_cur`, and add `new_subdir_without_flags_returns_noop_ok` and `tmp_subdir_returns_noop_ok`
- [x] Add tests/flag_fixes.rs: store into `new/`, add `Seen`, assert `new/` is empty, `cur/<id>:2,S` exists, locate reports `cur/` with `Seen`, and the id is unchanged
- [x] Extend the flag line of docs/design.md, which describes the module as the info-suffix rewrite alone
- [x] Add the CHANGELOG entry under `[Unreleased]`, linking himalaya/#637
- [x] Run `nix develop --command cargo fmt` and the test suite
