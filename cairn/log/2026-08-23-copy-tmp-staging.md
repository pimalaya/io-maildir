---
cairn: log
change: copy-tmp-staging
landed: 2026-08-23
---

# Stage a copied entry in tmp/

`MaildirEntryCopy` now yields its copy into `target/tmp/<id>` and follows it with a rename to the final name, where before it yielded the final name as the copy destination and completed on the copy reply. The state machine grew one step, `AwaitCopy` carrying the two paths and a new `AwaitRename` behind it, mirroring the `AwaitCreateTmp` and `AwaitRename` pair `MaildirEntryStore` has always had. Nothing else moved: `build_target_path` is unchanged, the error type gained no variant, and the client already serviced `WantsRename`.

The rename is unconditional. A caller asking for `MaildirSubdir::Tmp` renames the staged file onto itself, which POSIX defines as a successful no-op and which store already does in the same situation. The alternative, a branch comparing the two paths, would have bought nothing and diverged from the reference implementation.

The delivery capability moved: *Delivery is staged in tmp/* now covers every entry entering a Maildir rather than only one written from a caller's bytes, and carries the copy and the interrupted-copy scenarios.

On testing, honestly: the crash itself is not reproduced. Killing a process inside `fs::copy` is not something a Rust test can do to itself, and a test that faked it would assert nothing real. What the unit test asserts instead is the property that closes the window, that the coroutine never names the final path as a copy destination, which is a pure statement about the yields and needs no filesystem. The integration test adds the other half at the filesystem level, that the target's `tmp/` is empty once the copy returns, which is what a stage-but-never-rename regression would break. The reporter's `strace` fault injection remains the only way to observe the original failure, and it is now unable to produce it.
