---
cairn: spec
capability: listing
status: current
---

# Maildir listing

How the Maildirs of a store are found.

### Requirement: Plain files in a store do not stop the listing

Listing SHALL descend only into children that are directories (a symlink to one included), so the plain files a store holds beside its folders, such as dovecot's `dovecot-uidlist`, `dovecot-keywords` and `subscriptions`, are skipped rather than read as directories.

#### Scenario: A folder holding a dovecot-uidlist file

- GIVEN an fs-layout store whose Maildir `Archive` holds a `dovecot-uidlist` file and a nested Maildir `Archive/2026`
- WHEN the store is listed
- THEN `Archive` and `Archive/2026` are returned and no error is raised

### Requirement: One unreadable folder does not stop the listing

`MaildirClient::list_maildirs` SHALL list the rest of the store past a folder, a folder entry or a folder link it cannot read, and SHALL return each in its `MaildirListing::unreadable` as a `MaildirUnreadableFolder { path, error }`. A folder gone when read and a link to a folder the walk already reached SHALL be no failure. `MaildirClient::read_readable_entries` SHALL likewise read the rest past an entry it cannot read, and SHALL return each in its `MaildirEntriesRead::unreadable` as a `MaildirUnreadableEntry { entry, error }`, in the order the entries were given, where `read_entries` fails naming the entry's path. Every filesystem failure the client reports SHALL name the path it failed on.

#### Scenario: An unreadable message among three

- GIVEN a Maildir holding three messages, the file of one unreadable to the user
- WHEN `read_readable_entries` reads the three
- THEN its `entries` hold the two others, and its `unreadable` holds the third's entry and the permission error naming its path

### Requirement: A symlink back into the walk is not followed twice

The listing SHALL skip a symlink whose target is a folder the walk already reached (an ancestor included), with a warning, so a link back to the root or to an ancestor lists no folder twice.

#### Scenario: A link to the root

- GIVEN an fs-layout store holding `Work` and a symlink `Loop` to the root
- WHEN the store is listed
- THEN only `Work` is returned

### Requirement: Loading a folder that cannot be searched names the error

`MaildirClient::load_maildir` SHALL report a `cur`, `new` or `tmp` subdirectory it cannot stat for another reason than its absence (a permission error) as that I/O error naming the path, not as a missing subdirectory.
