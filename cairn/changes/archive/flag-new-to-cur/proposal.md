---
cairn: change
id: flag-new-to-cur
status: landed
created: 2026-08-23
---

# A flagged entry leaves new/ for cur/

## Why

`MaildirFlagsSet`, `MaildirFlagsAdd` and `MaildirFlagsRemove` all short-circuit an entry found in `new/` or `tmp/` to a successful no-op. So a flag write on a message that is still in `new/` silently discards the flag and reports success: `himalaya message read --seen` leaves the message unread on disk, and `himalaya flag add` does nothing at all.

Maildir has no other place to record a flag: the flags of an entry are the letters of its info suffix, and an entry in `new/` carries no info suffix by construction. Flagging one therefore has to move it. That is the transition himalaya/#637 asks for, and the same transition every other Maildir client performs when it takes a message out of `new/`.

The issue describes the older behaviour, appending `:2,S` while leaving the file in `new/`. What ships today drops the flag instead, which is worse: the write is lost rather than misplaced.

## What

A flag write whose resulting flag set is non-empty renames the entry from `new/<id>` to `cur/<id>:2,<flags>`. The unique id does not change, so an envelope id a caller stored stays valid across the move.

A write whose result is empty leaves the entry where it is. The invariant being restored is that a name carrying an info suffix lives in `cur/`; a write producing no flags does not break it, and moving on `flag remove -f seen` would silently mark an unread message as no longer new, which is a side effect nobody asked for. This also means `remove` needs no change at all: locate reports no flags for an entry in `new/`, so the result of removing there is empty by construction.

An entry in `tmp/` stays untouched whatever the flags are. It is another process's delivery in flight, and claiming it would steal a half-written message. The existing no-op stays, with a comment saying why, so it is not mistaken later for the same oversight this change fixes.

Out of scope: turning the `tmp/` case into an error rather than a silent success, which is defensible but a separate behaviour change with its own callers to check; and `MaildirEntryLocate` reporting no flags for `new/` and `tmp/`, which is correct for a conforming Maildir and which this change removes himalaya's own source of suffixed `new/` names for.
