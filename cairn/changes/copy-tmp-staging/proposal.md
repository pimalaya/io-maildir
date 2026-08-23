---
cairn: change
id: copy-tmp-staging
status: active
created: 2026-08-23
---

# Stage a copied entry in tmp/

## Why

Copying an entry writes it straight to its final name. `MaildirEntryCopy` yields `WantsCopy` with the destination the reader will enumerate, and the client services that with `fs::copy`, which creates the file at zero bytes and then streams into it. For the whole duration of the copy the destination is a complete, protocol-valid Maildir entry holding a truncated message.

Any death of the process in that window leaves it there: himalaya/#738 reports a 0-byte entry in `cur/` that `envelope list` shows as an ordinary message with blank columns, that `account check` calls healthy, that a second copy does not clean up, and that an independent Maildir reader enumerates too. Only reading it fails, as an RFC 5322 parse error. No mail is lost, since the source is conserved, but the folder now holds a message that is not one.

Store already does this correctly, and the README already claims the property for the crate as a whole ("write to the temporary directory, then atomically rename into place"). Copy is the one write path that never got it.

## What

Copy stages into `target/tmp/<id>` and renames to the final name, exactly as `MaildirEntryStore` does. A crash then leaves a stray file in `tmp/`, which readers ignore by convention and the usual 36-hour tmp sweep reclaims, rather than a phantom message in `cur/`.

The rename is unconditional, including when the caller asked for `MaildirSubdir::Tmp` and the staging path equals the final path. Store already renames onto itself in that case, and POSIX specifies `rename(2)` with identical operands as a successful no-op. A branch to avoid it would buy nothing and diverge from the reference implementation.

The crash itself is not reproducible from a test, and a test pretending to reproduce it would assert nothing. What is testable, and what actually closes the window, is that the coroutine never names the final path as a copy destination. That is a pure I/O-free assertion on the yields, and it is the regression test.

Out of scope: sweeping stale files out of `tmp/`, which is a maintenance concern with its own design questions; unifying the `build_target_path` helper duplicated between copy and move, which is unrelated to the bug; and the identical shape in io-vdir (`item/copy.rs` yields the final item path), which belongs to that repository.
