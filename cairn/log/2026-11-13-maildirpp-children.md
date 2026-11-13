---
cairn: log
change: maildirpp-children
landed: 2026-11-13
---

# Maildir++ children

Leftovers of the Himalaya Maildir test campaign of 2026-11-13 (re-verification report VMD, findings N1 to N3).

- On Maildir++, `MaildirRename` and `MaildirDelete` first read the root and act on every `.<name>.*` sibling along with `.<name>`, so a rename or delete no longer leaves the children under a parent that is gone. A rename landing a child on an existing folder is refused with the new `MaildirRenameError::TargetExists`, before any rename.
- `load_maildir` reports a subdirectory it cannot stat for another reason than its absence as the I/O error naming the path, where a folder with mode 000 read as "missing cur/ subdirectory".

Capabilities moved: delivery ("A Maildir++ folder is renamed and deleted with its children" added), listing ("Loading a folder that cannot be searched names the error" added).
