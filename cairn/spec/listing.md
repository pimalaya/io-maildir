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
