---
cairn: delta
change: name-guards
---

# Delta

## ADDED Requirements

Capability: delivery.

### Requirement: A name resolving onto a Maildir subdirectory is refused

In the fs layout a folder sits beside the `cur`, `new` and `tmp` of its parent, so a name holding one of the three as a segment resolves onto the parent's own messages. `MaildirCreate`, `MaildirDelete`, `MaildirRename` and `MaildirClient::load_maildir` SHALL refuse such a name with `NotAFolder` before any filesystem call, as they SHALL refuse a `.` or `..` segment in either layout. Maildir++ SHALL accept the three names, which resolve to `.cur`, `.new` and `.tmp`. `MaildirCreate` and `load_maildir` SHALL accept a name with no segment, the store root, which delete and rename refuse. Names are compared as written.

#### Scenario: A folder named `new` in the fs layout

- GIVEN an fs-layout store whose root is a Maildir holding an unread message
- WHEN `new` is created, loaded, deleted or renamed, or a folder is renamed to it
- THEN each call is refused and `new/` holds the message alone

#### Scenario: A folder named `new` in Maildir++

- GIVEN a Maildir++ store
- WHEN `new` is created
- THEN `.new` is a Maildir and the root's `new/` is untouched

Capability: flags.

### Requirement: A keyword that would not read back is not written

A keyword SHALL reach the `dovecot-keywords` file or the keyword header only when it reads back as written: not empty, not padded with whitespace, holding no control character and, for the header, not holding the header's separator. Any other keyword SHALL be dropped with a warning and the write SHALL go on without it: it gets no slot and no letter, and the header does not name it. A name the `dovecot-keywords` file already holds SHALL keep its slot whatever it holds, so a file another tool wrote is never renumbered. The serialiser SHALL skip a name holding a line feed.

#### Scenario: A keyword holding a line break

- GIVEN a `dovecot-keywords` file naming `a` `$Junk`
- WHEN a message is stored with the keywords `Work` and `x<LF>0 $NotJunk`
- THEN the file reads `0 $Junk` then `1 Work`, and the message carries `Work` alone

#### Scenario: A keyword holding a header line

- GIVEN a client writing keywords to `X-Keywords`
- WHEN a message is stored with the keywords `Work` and `x<CR><LF>Bcc: eve@example.org`
- THEN the message gains `X-Keywords: Work` and no other line

## MODIFIED Requirements

Capability: delivery.

### Requirement: A unique name minted per delivery

Every entry entering a Maildir SHALL be given a name minted from the current time, a per-process counter, the process id and the hostname. The `/`, `:` and NUL of the hostname SHALL be written `\057`, `\072` and `\000`, so a hostname adds neither a path segment nor an info section to the name. An entry delivered from another Maildir SHALL NOT reuse the source basename: a name carries folder-specific metadata baked into it by other tools, such as mbsync's `,U=<uid>` infix, which is valid only in the folder that wrote it and would corrupt the destination's sync state or silently overwrite a same-named entry there. A copy SHALL return the id it minted, so the caller can name the delivered entry.

#### Scenario: A copy of an mbsync entry

- GIVEN a source entry whose name carries a `,U=<uid>` infix
- WHEN it is copied into another Maildir
- THEN the delivered entry carries a freshly minted name with no `,U=` infix, and the source flags

#### Scenario: A hostname holding a slash and a colon

- GIVEN the hostname `mail/host:25`
- WHEN an entry is delivered
- THEN its name ends `.mail\057host\07225`
