---
cairn: change
id: store-separator
status: landed
created: 2026-11-13
---

# The separator belongs to the store, the counter to the driver

One finding of the family audit of 2026-11-13 (MAILDIR-05), two points left open by change name-guards, and the name of the batch lookup of change entry-kinds.

## Why

- The character opening the info section was a static defined under `cfg(unix)` and `cfg(windows)` alone. On any other target the core did not compile, although it is `no_std` and meant to run anywhere. And the separator is a property of a store, not of the machine the code was built on: a Maildir kept with `;`, or with `!` as some tools write on filesystems refusing the colon, read from a Unix build had every flag ignored and every flag write refused, with no option to say otherwise.
- The names minted for deliveries took their counter from a process-wide atomic inside the I/O-free core: state the driver cannot see, and an instruction some targets lack.
- Change name-guards left two points for Windows: a backslash in a folder name passes the folder check and is a path separator there, and the hostname escape writes a backslash. Both hung on `cfg!(windows)`, as the refusal of `\` and `:` in an entry id did.
- The batch lookup was `MaildirEntriesLocate` in the module `entry::locate_many`: the module and the type did not agree.

## What

- `MaildirStore` gains `separator`, the colon by default. `Maildir` and `MaildirEntry` carry it: a Maildir loaded or listed from a store takes the store's, `Maildir::entry` hands it to the entries, and every name is written and split with it. `INFORMATIONAL_SUFFIX_SEPARATOR` is gone. `MaildirFlags` is read from a `MaildirEntry` rather than a path, and `MaildirFullEntry` holds one, so no reading can pick another separator than its entry's.
- `MaildirClient::new`, which is std, sets `;` on Windows, so an application there changes nothing.
- The counter is asked of the driver: `WantsCounter`, answered by `Counter`, between the pid and the hostname. The same answers now give the same name. `MaildirClient` answers from a process-wide atomic of its own, where process state belongs.
- The rules that hung on `cfg!(windows)` hang on the store: a store whose separator is not the colon sits on a filesystem that reserves the colon and takes the backslash as a path separator. There an entry id holding `\` or `:` is refused, a folder name holding `\` is refused, and a character the hostname cannot keep is written `_`, the octal escape needing a backslash.
- The core holds no `cfg` on a target and no static. The two that remain in the crate are in the std client and in the `PathBuf` conversions the `client` feature gates.
- The type takes the name of its module, as `MaildirEntryLocate` does of `entry::locate`: `MaildirEntryLocateMany`, with `MaildirEntryLocateManyError` and `MaildirClient::locate_many`. The crate lays its coroutines out as one concept folder and one file per verb, and `entry::locate` is taken.

## What a driver has to change

Answer `WantsCounter` with `MaildirReply::Counter(n)`, `n` given to no other such request in the process: an incremented integer does. An application whose store is not on the colon sets `store.separator` once.

## Out of scope

A store whose separator is the colon on a filesystem folding case still resolves `New` onto `new`. No target other than the host's is installed in the development shell, so the core was built without its default features for the host alone.
