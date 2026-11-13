---
cairn: spec
capability: delivery
status: current
---

# Entry delivery

How an entry enters a Maildir: the unique name it is given, and the steps a reader may observe while it gets there.

### Requirement: A unique name minted per delivery

Every entry entering a Maildir SHALL be given a name minted from the current time, the process id, a counter and the hostname, each asked of the driver (`WantsTime`, `WantsPid`, `WantsCounter`, `WantsHostname`): the core keeps no state of its own, so the same answers give the same name. The driver SHALL answer `WantsCounter` with a number it gave to no other request in the process, which is what keeps two deliveries of one clock tick apart. A character of the hostname the name cannot hold (`/`, NUL and the info separator) SHALL be written in octal after a backslash (`\057`, `\000`, `\072` for the colon), so a hostname adds neither a path segment nor an info section to the name; in a store whose separator is not the colon, where the backslash is reserved too, it SHALL be written `_`, as `\` and `:` are. An entry delivered from another Maildir SHALL NOT reuse the source basename: a name carries folder-specific metadata baked into it by other tools, such as mbsync's `,U=<uid>` infix, which is valid only in the folder that wrote it and would corrupt the destination's sync state or silently overwrite a same-named entry there. A copy SHALL return the id it minted, so the caller can name the delivered entry.

#### Scenario: A copy of an mbsync entry

- GIVEN a source entry whose name carries a `,U=<uid>` infix
- WHEN it is copied into another Maildir
- THEN the delivered entry carries a freshly minted name with no `,U=` infix, and the source flags

#### Scenario: A hostname holding a slash and a colon

- GIVEN the hostname `mail/host:25`
- WHEN an entry is delivered
- THEN its name ends `.mail\057host\07225`

#### Scenario: The same answers twice

- GIVEN a driver answering time `1` and `2` nanoseconds, pid `3`, counter `4` and hostname `host`
- WHEN two entries are stored
- THEN both are named `1.#4M2P3.host`

### Requirement: Delivery is staged in tmp/

Every entry entering a Maildir, whether written from a caller's bytes or copied from another entry, SHALL be written under `tmp/` and renamed into `cur/` or `new/`. A reader enumerating the destination SHALL never observe the entry under its final name until every byte of it is there, so a process that dies mid-delivery leaves at worst a stray file in `tmp/`, which is not a message.

#### Scenario: A stored message

- GIVEN a message stored into `cur/`
- WHEN the store completes
- THEN the bytes were written under `tmp/` and renamed into `cur/`, leaving `tmp/` empty

#### Scenario: A copied message

- GIVEN an entry copied into another Maildir
- WHEN the copy completes
- THEN the bytes were copied under the target's `tmp/` and renamed into the target's `cur/`, leaving `tmp/` empty

#### Scenario: A copy interrupted mid-flight

- GIVEN a copy whose process dies between the first byte and the last
- WHEN the destination Maildir is enumerated
- THEN no entry under a final name is holding a truncated message

### Requirement: A delivery reported done is flushed to stable storage

A store or a copy SHALL ask the driver to flush the entry under `tmp/` (`WantsFileSync`) before the rename, and to flush the directory it was renamed into (`WantsDirSync`) after it. A move, being a rename alone, SHALL ask for the source and the target directories to be flushed after it. A delivery reported done then survives a power cut, where staging in `tmp/` alone covers a dying process. `MaildirClient` SHALL answer both requests with a sync of the file or directory; where the platform cannot flush a directory, or a read-only file, the request SHALL be acknowledged as it is. Flag writes and folder operations ask for no flush.

Flushing is the default and costs two flushes per delivery, which a bulk import or a first synchronisation of tens of thousands of messages pays in full. With `MaildirClient::skip_sync` set, the client SHALL acknowledge both requests without flushing, the coroutines still yielding them: deliveries then run at the speed of the page cache and stay safe against a dying process, and a power cut may lose or truncate the entries delivered shortly before it. The option suits a caller that can deliver again what was lost, and no other.

#### Scenario: A stored message

- GIVEN a message stored into `new/`
- WHEN the store runs
- THEN the driver is asked, in order, to write `tmp/<id>`, flush it, rename it to `new/<id>` and flush `new/`

#### Scenario: A bulk import with skip_sync

- GIVEN a client whose `skip_sync` is set
- WHEN a message is stored
- THEN the same four requests are made and the two flushes are acknowledged without touching the disk

### Requirement: A move is a single rename

Moving an entry into another Maildir SHALL be one rename from the source path to the freshly minted destination name, so the entry exists at exactly one of the two paths at any instant.

### Requirement: The source of a copy is untouched

Copying SHALL leave the source entry byte for byte as it was, under its own name in its own Maildir, whatever happens to the destination.

### Requirement: A copy and a move return the id they minted

`MaildirEntryCopy`, `MaildirEntryMove`, `MaildirClient::copy` and `MaildirClient::r#move` SHALL complete with the id minted for the delivered entry, so a caller reports the entry by the name it has afterwards.

### Requirement: The store root is never deleted nor renamed

`MaildirDelete` and `MaildirRename` SHALL refuse a name that resolves to the store root or outside it (empty, `.`, `..` or holding either as a segment), before any filesystem call, with `NotAFolder`. In Maildir++ `.` would resolve to the parent of the root.

#### Scenario: Deleting `.`

- GIVEN a store whose root is a Maildir holding `Work` and a Maildir++ `.Sent`
- WHEN `.` is deleted
- THEN the delete is refused and nothing is removed

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

### Requirement: A Maildir++ folder is renamed and deleted with its children

On Maildir++, `MaildirRename` SHALL rename every root entry named `.<from>.*` along with `.<from>`, as the fs layout moves the subtree, and SHALL refuse with `TargetExists`, before any rename, when one of the new names already exists. `MaildirDelete` SHALL remove every `.<name>.*` entry along with `.<name>`.

#### Scenario: Renaming a Maildir++ folder holding a child

- GIVEN a Maildir++ store holding `.Lists.rust`, `.Lists.rust.Kaffee` and `.Lists.rustacean`
- WHEN `Lists/rust` is renamed `Lists/go`
- THEN the store holds `.Lists.go`, `.Lists.go.Kaffee` and `.Lists.rustacean`
