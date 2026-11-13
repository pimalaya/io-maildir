---
cairn: log
change: name-guards
landed: 2026-11-13
---

# Name guards

Three findings of the family audit of 2026-11-13 (MAILDIR-01, MAILDIR-08, MAILDIR-10).

- In the fs layout `cur`, `new` and `tmp` were accepted as folder names, resolving onto the parent's own message directories: `create_maildir("new")` nested a Maildir in the root's `new/` and `delete_maildir("new")` removed its unread messages. The new `MaildirStore::names_folder` refuses these segments in the fs layout along with `.` and `..`, and `MaildirStore::names_maildir` accepts the root besides. `MaildirDelete` and `MaildirRename` call the first, `MaildirCreate` and `MaildirClient::load_maildir`, which checked nothing, the second, through the new `MaildirCreateError::NotAFolder` and `MaildirClientError::NotAFolder`. Maildir++ keeps the three names.
- A keyword went unchecked to the `dovecot-keywords` file and to the keyword header, so `x<LF>0 y` renamed slot 0 and one holding CR LF added header lines. `allocate_keyword_slot` gives no slot to a new keyword that is empty, padded with whitespace or holding a control character, and `MaildirClient::store` keeps it out of the header: it is dropped with a warning, as a keyword finding the table full is. `serialize_dovecot_keywords` skips a name holding a line feed.
- `mint_id` writes the `/`, `:` and NUL of the hostname as `\057`, `\072` and `\000`.

Left open: names are compared as written, so `New` resolves onto `new` on a case-insensitive filesystem, and on Windows the backslash is a path separator that neither the folder check nor the hostname escape accounts for.

Verification: `cargo test --all-features`, `cargo clippy --all-features --all-targets -- -D warnings`, `cargo build --no-default-features`. Windows was not exercised.

Capabilities moved: delivery ("A name resolving onto a Maildir subdirectory is refused" added, "A unique name minted per delivery" modified), flags ("A keyword that would not read back is not written" added).
