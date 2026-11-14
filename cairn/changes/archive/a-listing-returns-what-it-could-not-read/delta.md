---
cairn: delta
change: a-listing-returns-what-it-could-not-read
---

# Delta

## ADDED Requirements

None.

## MODIFIED Requirements

### Requirement: One unreadable folder does not stop the listing

(listing) `MaildirClient::list_maildirs` SHALL list the rest of the store past a folder, a folder entry or a folder link it cannot read, and SHALL return each in its `MaildirListing::unreadable` as a `MaildirUnreadableFolder { path, error }`. A folder gone when read and a link to a folder the walk already reached SHALL be no failure. `MaildirClient::read_readable_entries` SHALL likewise read the rest past an entry it cannot read, and SHALL return each in its `MaildirEntriesRead::unreadable` as a `MaildirUnreadableEntry { entry, error }`, in the order the entries were given, where `read_entries` fails naming the entry's path. Every filesystem failure the client reports SHALL name the path it failed on.

#### Scenario: An unreadable message among three

- GIVEN a Maildir holding three messages, the file of one unreadable to the user
- WHEN `read_readable_entries` reads the three
- THEN its `entries` hold the two others, and its `unreadable` holds the third's entry and the permission error naming its path

## REMOVED Requirements

None.
