---
cairn: log
change: copy-returns-its-id
landed: 2026-11-12
---

# Copy returns its id

`MaildirEntryCopy` completes with the id it minted for the delivered entry, and `MaildirClient::copy` returns it, so a caller can report the copy by the name it got rather than the source's. Breaking for a caller matching the old `()` output. Capability moved: delivery.
