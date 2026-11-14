---
cairn: tasks
change: a-listing-returns-what-it-could-not-read
---

# Tasks

- [ ] `MaildirEntriesRead`, `MaildirUnreadableEntry`; `read_readable_entries` returning them
- [ ] `MaildirListing`, `MaildirUnreadableFolder`; `read_folder` collecting what it cannot read; `list_maildirs` returning them
- [ ] Logs to debug, the link guard kept a warning
- [ ] Tests: an entry whose file is unreadable among two; a folder whose mode forbids reading; a folder removed while walked, not reported
- [ ] cairn/spec/listing.md: fold the delta; CHANGELOG; the release left to the owner; log; status landed
