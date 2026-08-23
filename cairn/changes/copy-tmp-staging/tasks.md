---
cairn: tasks
change: copy-tmp-staging
---

- [ ] Carry both paths through the copy state: `AwaitCopy { tmp_path, final_path }` plus a new `AwaitRename`, mirroring store's `AwaitCreateTmp`
- [ ] Yield `WantsCopy` with the staging path `target.tmp().join(&id)` instead of the final name
- [ ] Yield `WantsRename` from the staging path to the final name on the copy reply, with a NOTE that a `Tmp` target renames onto itself as a POSIX no-op, as in store
- [ ] Complete on the rename reply, moving the `debug!("copied entry")` off the copy arm so it still marks completion
- [ ] Extend `Display for State` with the rename step
- [ ] Say in the module header that the copy is staged through `tmp/`, like `MaildirEntryStore`
- [ ] Amend `cur_copy_mints_fresh_id_and_preserves_flags`: the copy destination is under `tmp/`, and the fresh-id, no-`,U=999` and flags-preserved assertions move onto the rename destination
- [ ] Assert in the copy section of tests/integration.rs that the target's `tmp/` is empty once the copy returns
- [ ] Add the CHANGELOG entry under `[Unreleased]`, linking himalaya/#738
- [ ] Run `nix develop --command cargo fmt` and the test suite
