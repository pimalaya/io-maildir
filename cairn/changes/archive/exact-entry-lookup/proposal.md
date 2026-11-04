---
cairn: change
id: exact-entry-lookup
status: landed
created: 2026-11-04
---

# Resolve exact Maildir entries and preserve their flags

## Why

Two lookup failures were confirmed by source review: logical IDs listed from
suffixed `new/` entries cannot be located, and prefix matching in `cur/` can
select a different message even when the caller supplies a complete listed ID. Delete directly removes the
located path. Flag updates, move, and copy also share this locator. The human
reviewed this proposal and approved implementation before code changes began.

This plan is based on io-maildir 0.3.1 at
`7c7aafc3bba6538cd633023eca5356f0def1134d`. The independent baseline review
reproduced both defects: a truncated overlapping ID selected a different
entry, and a suffixed `new/` entry was not found by its logical ID.

## What

Make the locator accept an exact logical ID or exact complete basename, scan
`new/` as well as `cur/`, and return the actual path, directory, and parsed flags.
Then correct the small downstream assumptions exposed by accepting flagged
`new/` files and complete basenames. Preserve the sans-I/O design and existing
delivery protocol. The implementation checklist and validation record are in `tasks.md`.

## Proposed decisions

1. Compare the supplied input with either the complete candidate basename or
   `MaildirEntry::id()`. Never strip the caller's suffix to make a different
   full filename match. Use the existing platform separator and parsing rules;
   changing suffix grammar is outside this fix.
2. Preserve successful exact probes in this order: `new/<input>`, then
   `tmp/<input>`. On a miss, scan `new/` and `cur/`, considering `new/` first,
   then `cur/`, and lexicographically ordered candidate paths within each.
   This explicitly defines duplicate logical-ID resolution without a new
   ambiguity error or a global duplicate scan. Exact probes retain precedence
   over scanned matches. No `tmp/` scan.
3. Confirm scanned matching candidates are files through `WantsFileExists`,
   as listing already does. Skip dotfiles. A vanished candidate is skipped;
   disappearance after selection retains the existing client I/O error.
   This does not promise atomic lookup plus mutation or symlink containment.
4. Reject empty inputs, dot-prefixed inputs, NUL, and platform path separators
   before any I/O yield, using `NotFound`. `/` is rejected on all platforms;
   `\` is rejected on Windows, where it is a path separator, and remains a
   valid Unix basename character. On Windows also reject drive/stream colon
   syntax. Valid platform informational suffixes remain accepted. Keep
   validation local to entry lookup; do not redesign the path types.
5. Build flag-update destination names from the located entry's logical ID,
   never the raw caller input. Preserve current keyword/unknown-letter policy.
   Plain `new/` no-op behavior and all `tmp/` flag no-ops remain unchanged.
   For flagged `new/`, add/remove/set must operate on its actual flags. A
   changed nonempty result moves to `cur/`; a cleared result stays in `new/`
   under its unsuffixed logical ID. An unchanged result leaves the file alone.
6. Move/copy to `New`, including the default inherited destination, retains
   nonempty source flags using a suffix in `new/`. Empty flags keep the plain
   name. Explicit `Cur` and `Tmp` destination behavior stays unchanged. This
   preserves caller directory selection without silently discarding flags.

## Boundaries

No parser redesign, generalized coroutine framework, public error expansion,
dependency update, mailbox normalization, sync configuration change, or MCP
change. Do not merge helpers just to remove existing duplication. No commits,
forks, pushes, or PR submission are needed to implement this plan.

Automated and downstream CLI verification use synthetic fixtures in temporary
Maildirs. No live mailbox data is needed in the patch or its tests.

## Review and completion

The reviewed behavior choices include duplicate precedence and flagged `new/`
writes. After implementation, fold the delta into
the current spec, add the dated log and changelog entry, and archive this change.
Do not mark it landed merely because the plan is complete.
