---
cairn: change
id: entry-kinds
status: landed
created: 2026-11-13
---

# Directory reads carry the entry kind, and flushing gets an option

Two findings of the family audit of 2026-11-13 (MAILDIR-03, MAILDIR-04), and a follow-up to the flushes landed the same day (change keyword-file-and-sync).

## Why

- A listing read `new/` and `cur/`, then asked for a stat of every name to keep the regular files. A program renaming an entry between the two steps, which is how every Maildir client changes a flag, made the old name fail its stat while the new one had never been listed: the listing completed without an entry that exists, and a caller comparing listings reads that as a deletion. The stat pass over a large folder is what kept the window open.
- Every operation addressed by id started with a lookup probing `new/<id>` and `tmp/<id>` only. An entry in `cur/`, the common case, cost a read of both directories and a third request confirming the candidate, even when the caller held the complete basename. Nothing resolved several ids at once, so flagging a thousand messages read the folder a thousand times.
- Since the flushes landed, the std client flushes twice per delivery with no way out, which an import of tens of thousands of messages pays in full.

## What

- `MaildirReply::DirRead` maps each directory to its entries with a `MaildirFileKind` (`File`, `Dir`, `Other`, `Missing`), a symlink counting as what it points to. `MaildirClient` takes the kind from the directory entry and stats a symlink only.
- `MaildirEntryList` keeps the regular files on that answer: one request, no stat pass. `MaildirEntryLocate` drops its second probe the same way.
- A driver that must read the kind after the name reports an entry gone in between as `Missing`. The listing then reads the directories again, three reads at most, and fails with `Unstable` rather than return short; a lookup whose only matches are gone reads again and ends on NotFound.
- `MaildirEntryLocate` probes `cur/<id>` with `new/<id>` and `tmp/<id>`: a complete basename costs one request, a logical id two. An exact name in `cur/` now wins over a logical match in `new/`, which only matters to a Maildir holding two entries under one id.
- `MaildirEntriesLocate`, with `MaildirClient::locate_entries`, resolves any number of ids from one read of `new/`, `tmp/` and `cur/`, by the rules of the single lookup, which both share.
- `MaildirClient::skip_sync`, off by default, acknowledges `WantsFileSync` and `WantsDirSync` without flushing. The coroutines yield them all the same.

## What a driver has to change

A driver other than `MaildirClient` answers `WantsDirRead` with a kind per entry: `File` for a regular file, `Dir` for a directory, `Other` for the rest, a symlink reported as its target. One that learns the kind in a second step reports `Missing` when the entry is gone by then; one that learns both at once never does.

## Out of scope

`MaildirList`, the folder walker, takes the new answer but still probes the `cur`, `new` and `tmp` of each child: it could keep directories by their kind. Reading only the head of a message for an envelope listing needs a bounded read request, left for later. An entry renamed while the directory itself is being read may be missed by the system's own listing, which no request can see.
