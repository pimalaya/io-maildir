---
cairn: tasks
change: copy-tmp-staging
---

- [x] Carry both paths through the copy state: `AwaitCopy { tmp_path, final_path }` plus a new `AwaitRename`, mirroring store's `AwaitCreateTmp`
- [x] Yield `WantsCopy` with the staging path `target.tmp().join(&id)` instead of the final name
- [x] Yield `WantsRename` from the staging path to the final name on the copy reply, with a NOTE that a `Tmp` target renames onto itself as a POSIX no-op, as in store
- [x] Complete on the rename reply, moving the `debug!("copied entry")` off the copy arm so it still marks completion
- [x] Extend `Display for State` with the rename step
- [x] Say in the module header that the copy is staged through `tmp/`, like `MaildirEntryStore`
- [x] Amend `cur_copy_mints_fresh_id_and_preserves_flags`: the copy destination is under `tmp/`, and the fresh-id, no-`,U=999` and flags-preserved assertions move onto the rename destination
- [x] Assert in the copy section of tests/integration.rs that the target's `tmp/` is empty once the copy returns
- [x] Add the CHANGELOG entry under `[Unreleased]`, linking himalaya/#738
- [x] Run `nix develop --command cargo fmt` and the test suite
