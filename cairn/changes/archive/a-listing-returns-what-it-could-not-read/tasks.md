---
cairn: tasks
change: a-listing-returns-what-it-could-not-read
---

# Tasks

- [x] `MaildirEntriesRead`, `MaildirUnreadableEntry`; `read_readable_entries` returning them
- [x] `MaildirListing`, `MaildirUnreadableFolder`; `read_folder` collecting what it cannot read; `list_maildirs` returning them
- [x] Logs to debug, the link guard kept a warning
- [x] Tests: an entry whose file is unreadable among two; a folder whose mode forbids reading; a folder removed while walked, not reported
- [x] cairn/spec/listing.md: fold the delta; CHANGELOG; log; status landed
- [ ] The release, left to the owner
