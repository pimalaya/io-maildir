---
cairn: tasks
change: name-guards
---

# Tasks

- [x] Reproduce each finding with a failing test: reserved names through the three lifecycle coroutines and the client (tests/name_guards.rs), a keyword holding a line break through store, add and set, a hostname holding `/`, `:` and NUL through `mint_id`.
- [x] Add `MaildirStore::names_folder` and `MaildirStore::names_maildir`, and call them from `MaildirCreate`, `MaildirDelete`, `MaildirRename` and `MaildirClient::load_maildir`.
- [x] Add `MaildirCreateError::NotAFolder` and `MaildirClientError::NotAFolder`.
- [x] Check a new keyword in `allocate_keyword_slot` and in the header injection of `MaildirClient::store`; skip a name holding a line feed in `serialize_dovecot_keywords`.
- [x] Escape `/`, `:` and NUL of the hostname in `mint_id`.
- [x] Fold the delta into the delivery and flags specs, write the log entry, add the changelog lines.
- [x] Run `cargo fmt`, the test suite, clippy and the build without default features.
