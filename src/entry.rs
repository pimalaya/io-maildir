//! Maildir entries: the [`MaildirEntry`] lightweight handle and the
//! [`MaildirFullEntry`] body-carrying entry.
//!
//! The I/O-free coroutines for the delivery protocol and the entry
//! lifecycle (store, get, list, locate, copy, move) live in the
//! submodules next to this file, alongside the RFC 5322 header helpers
//! under [`headers`].

pub mod copy;
pub mod get;
pub mod headers;
pub mod list;
pub mod locate;
pub mod locate_many;
pub mod r#move;
pub mod store;

use core::{
    fmt::Write as _,
    hash::{Hash, Hasher},
};

use alloc::{string::String, vec::Vec};

use crate::{flag::MaildirFlags, maildir::SEPARATOR, path::MaildirFsPath};

/// How many times the subdirectories are read while a name they list
/// turns out gone, before a listing or a lookup gives up.
pub(crate) const MAX_DIR_READS: u8 = 3;

/// Returns `true` when no file name of a Maildir whose info separator
/// is `separator` can hold `c`: NUL and `/` anywhere, and `\` and `:`
/// too off the colon, such a Maildir sitting on a filesystem that
/// reserves them.
pub(crate) fn is_reserved(c: char, separator: char) -> bool {
    matches!(c, '\0' | '/') || (separator != SEPARATOR && matches!(c, '\\' | ':'))
}

/// Mints a Maildir unique name following the delivery convention:
/// `{secs}.#{counter:x}M{nanos}P{pid}.{hostname}`.
///
/// A character of the hostname the name cannot hold, the separator
/// included, is written in octal (`\057` for `/`, `\072` for `:`), or
/// `_` off the colon, where the backslash is reserved too.
///
/// Used by the store, copy and move coroutines so a relocated entry
/// never reuses the source basename, which may carry foreign,
/// folder-specific metadata such as mbsync's `,U=<uid>` infix.
pub(crate) fn mint_id(
    secs: u64,
    nanos: u32,
    pid: u32,
    counter: u32,
    hostname: &str,
    separator: char,
) -> String {
    let mut id = format!("{secs}.#{counter:x}M{nanos}P{pid}.");

    for c in hostname.chars() {
        if c != separator && !is_reserved(c, separator) {
            id.push(c);
        } else if separator == SEPARATOR {
            let _ = write!(id, "\\{:03o}", c as u32);
        } else {
            id.push('_');
        }
    }

    id
}

/// A Maildir entry: on-disk path, body bytes and flags.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct MaildirFullEntry {
    pub(crate) entry: MaildirEntry,
    pub(crate) contents: Vec<u8>,
    pub(crate) flags: MaildirFlags,
}

impl MaildirFullEntry {
    /// Returns the on-disk path of the entry file.
    pub fn path(&self) -> &MaildirFsPath {
        &self.entry.path
    }

    /// Returns the flags of the entry.
    ///
    /// An entry the client read carries its custom keywords resolved
    /// through the client's own `dovecot_keywords` and
    /// `keywords_header`; one built by hand carries the filename
    /// letters alone.
    pub fn flags(&self) -> &MaildirFlags {
        &self.flags
    }

    /// Returns the entry id (filename before the info-section
    /// separator).
    pub fn id(&self) -> Option<&str> {
        self.entry.id()
    }

    /// Returns the raw body bytes.
    pub fn contents(&self) -> &[u8] {
        &self.contents
    }

    /// Parses the full message, headers and body, with mail-parser.
    #[cfg(feature = "parser")]
    pub fn parsed(&self) -> Option<mail_parser::Message<'_>> {
        mail_parser::MessageParser::new().parse(&self.contents)
    }

    /// Parses only the minimal headers with mail-parser.
    #[cfg(feature = "parser")]
    pub fn headers(&self) -> Option<mail_parser::Message<'_>> {
        mail_parser::MessageParser::new()
            .with_minimal_headers()
            .parse(&self.contents)
    }
}

impl From<MaildirFullEntry> for Vec<u8> {
    fn from(msg: MaildirFullEntry) -> Self {
        msg.contents
    }
}

impl From<(MaildirEntry, Vec<u8>)> for MaildirFullEntry {
    fn from((entry, contents): (MaildirEntry, Vec<u8>)) -> Self {
        let flags = entry.flags();
        Self {
            entry,
            contents,
            flags,
        }
    }
}

impl Hash for MaildirFullEntry {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.entry.path.hash(state);
    }
}

/// Lightweight handle to a Maildir entry file (path only, no body).
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct MaildirEntry {
    pub(crate) path: MaildirFsPath,
    /// Character between the id and the info section of the file name.
    pub separator: char,
}

impl MaildirEntry {
    /// Wraps `path` as an entry handle, its info separator the colon.
    ///
    /// [`Maildir::entry`](crate::maildir::Maildir::entry) gives the
    /// handle of an entry in a Maildir with another one.
    pub fn from_path(path: impl Into<MaildirFsPath>) -> Self {
        Self {
            path: path.into(),
            separator: SEPARATOR,
        }
    }

    /// Returns the on-disk path of the entry file.
    pub fn path(&self) -> &MaildirFsPath {
        &self.path
    }

    /// Returns the entry id (filename before the info-section
    /// separator).
    pub fn id(&self) -> Option<&str> {
        let file_name = self.path.file_name()?;

        Some(match file_name.rsplit_once(self.separator) {
            Some((id, _)) => id,
            None => file_name,
        })
    }

    /// Parses the flags encoded in the filename.
    pub fn flags(&self) -> MaildirFlags {
        MaildirFlags::from(self)
    }
}

impl From<MaildirFsPath> for MaildirEntry {
    fn from(path: MaildirFsPath) -> Self {
        Self::from_path(path)
    }
}

impl From<MaildirEntry> for MaildirFsPath {
    fn from(entry: MaildirEntry) -> Self {
        entry.path
    }
}
