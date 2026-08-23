---
cairn: log
change: flag-new-to-cur
landed: 2026-08-23
---

# A flagged entry leaves new/ for cur/

`MaildirFlagsAdd` and `MaildirFlagsSet` no longer treat an entry in `new/` as nothing to do. Both keep the `Maildir` they were built with, cloning it into their inner locate, and build the destination on `maildir.cur()` through `cur_path_with_flags`, which replaces the parent-preserving `rename_with_flags` they used before. One helper now serves both branches, since an entry already in `cur/` resolves to the same path either way.

The subdir match reads as the two rules it encodes: `tmp/` completes without renaming, `new/` with nothing to gain completes without renaming, and `cur/` or a flagged `new/` renames. The type docs carry the why, so no inline comment does.

`MaildirFlagsRemove` is untouched, and that is the point of the emptiness rule rather than an omission: locate reports no flags for an entry in `new/`, so removing there always resolves to an empty set and always falls in the no-op arm. Had the rule been "any flag write moves the entry", `flag remove` on an unread message would have marked it no longer new as a side effect of changing nothing.

The flags capability gained *Gaining a flag moves an entry out of new/*, carrying both halves of the rule.

Tests: each of the two coroutines now covers its four outcomes, the `cur/` rename included, which was untested at unit level before and which is what guards the helper swap. tests/flag_fixes.rs covers the pair end to end on a real tree, asserting that the id survives the move, since a changed id would invalidate every envelope identifier a caller stored.

The end to end test had to be corrected rather than extended. Its "FLAGS SET (no-op for /new)" section asserted the old behaviour as though it were the specification, citing the Maildir spec for it, so it failed the moment the fix landed. It now asserts the move. Worth noting because the bug was not only in the code: it was written down as intended in a test, which is how it survived this long.

For himalaya this is the fix for message read --seen on a Maildir account: the flag now lands on disk, and the message leaves `new/` as every other Maildir client expects.
