---
cairn: log
change: flags-in-ascii-order
landed: 2026-11-12
---

# Flags in ASCII order

`MaildirFlag` declares its variants in the ASCII order of their letters (D, F, P, R, S, T), so the derived `Ord` the flag set iterates in writes the info section as the Maildir spec and cairn/spec/flags.md require: `:2,FS`, not `:2,SF`. A Himalaya live test against a local store found `:2,RSDF` on disk on 2026-11-12. No capability moved: flags.md already asked for sorted letters.
