//! Maildir flags: the [`MaildirFlags`] set, the individual
//! [`MaildirFlag`] letters and keywords, and the [`KeywordHeader`]
//! carrying custom keywords inline with a message body.
//!
//! The I/O-free coroutines rewriting the `:2,<flags>` suffix on entry
//! filenames live in the submodules next to this file: [`add`],
//! [`remove`] and [`set`].

pub mod add;
pub mod remove;
pub mod set;

use core::{
    fmt::{self, Write as _},
    str::FromStr,
};

use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::String,
    vec::Vec,
};

use log::trace;

use crate::{
    entry::{INFORMATIONAL_SUFFIX_SEPARATOR, headers::extract_keywords_header},
    path::MaildirFsPath,
};

/// A set of Maildir flags plus opaque info-section letters.
#[derive(Clone, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct MaildirFlags {
    /// Named flags and custom keywords.
    flags: BTreeSet<MaildirFlag>,
    /// Resolved dovecot `a..z` slot letters with no named-variant
    /// counterpart, appended verbatim by [`fmt::Display`].
    extra_letters: BTreeSet<char>,
}

/// Returns the `<letters>` part of an entry filename's
/// `<id>:2,<letters>` info section, if any.
///
/// Splits at the marker, not at the last comma, so that dovecot's
/// `,S=<size>,W=<vsize>` extensions in the unique part are not read as
/// flag letters.
fn info_letters(file_name: &str) -> Option<&str> {
    let (_, info) = file_name.rsplit_once(INFORMATIONAL_SUFFIX_SEPARATOR)?;
    let (_, letters) = info.split_once(',')?;
    Some(letters)
}

impl From<&MaildirFsPath> for MaildirFlags {
    fn from(path: &MaildirFsPath) -> Self {
        let Some(file_name) = path.file_name() else {
            return Default::default();
        };

        let Some(letters) = info_letters(file_name) else {
            return Default::default();
        };

        // NOTE: unnamed letters are kept verbatim, otherwise a flag op
        // would rewrite the name without the dovecot keyword slots.
        let mut flags = BTreeSet::new();
        let mut extra_letters = BTreeSet::new();
        for c in letters.chars() {
            match MaildirFlag::from_char(c) {
                Some(flag) => {
                    flags.insert(flag);
                }
                None => {
                    extra_letters.insert(c);
                }
            }
        }

        MaildirFlags {
            flags,
            extra_letters,
        }
    }
}

impl fmt::Display for MaildirFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // NOTE: BTreeSet iterates in sorted order so the on-disk
        // representation is deterministic.
        for flag in &self.flags {
            write!(f, "{flag}")?;
        }
        for letter in &self.extra_letters {
            f.write_char(*letter)?;
        }
        Ok(())
    }
}

impl MaildirFlags {
    /// Returns `true` when no flag, keyword or extra letter is set.
    pub fn is_empty(&self) -> bool {
        self.flags.is_empty() && self.extra_letters.is_empty()
    }

    /// Returns the total count of flags, keywords and extra letters.
    pub fn len(&self) -> usize {
        self.flags.len() + self.extra_letters.len()
    }

    /// Returns `true` when `flag` is present in the set.
    pub fn contains(&self, flag: &MaildirFlag) -> bool {
        self.flags.contains(flag)
    }

    /// Merges every flag, keyword and extra letter of `flags` in.
    pub fn extend(&mut self, flags: MaildirFlags) {
        self.flags.extend(flags.flags);
        self.extra_letters.extend(flags.extra_letters);
    }

    /// Removes from this set every flag and letter present in `flags`.
    pub fn difference(&mut self, flags: &MaildirFlags) {
        self.flags = self.flags.difference(&flags.flags).cloned().collect();
        self.extra_letters = self
            .extra_letters
            .difference(&flags.extra_letters)
            .copied()
            .collect();
    }

    /// Iterates over the named flags and keywords, sorted.
    pub fn iter(&self) -> impl Iterator<Item = &MaildirFlag> {
        self.flags.iter()
    }

    /// Inserts `flag`, returning `true` when it was not already set.
    pub fn insert(&mut self, flag: MaildirFlag) -> bool {
        self.flags.insert(flag)
    }

    /// Like [`From<&MaildirFsPath>`] but resolves lowercase `a..z` letters
    /// through a dovecot-keywords table.
    pub fn with_dovecot(path: &MaildirFsPath, table: &BTreeMap<char, String>) -> Self {
        let Some(file_name) = path.file_name() else {
            return Default::default();
        };

        let Some(letters) = info_letters(file_name) else {
            return Default::default();
        };

        let mut flags = BTreeSet::new();
        for c in letters.chars() {
            if let Some(flag) = MaildirFlag::from_char(c) {
                flags.insert(flag);
            } else if c.is_ascii_lowercase() {
                if let Some(name) = table.get(&c) {
                    flags.insert(MaildirFlag::Keyword(name.clone()));
                }
            }
        }
        MaildirFlags {
            flags,
            extra_letters: BTreeSet::new(),
        }
    }

    /// Like [`Self::with_dovecot`], additionally reading the keywords
    /// `contents` carries in `header`.
    ///
    /// The two conventions compose rather than exclude each other: a
    /// message can name one keyword by a dovecot slot letter and
    /// another in its header, and the flag set carries both. This is
    /// the read counterpart of the serialisation the client performs on
    /// store, which is why the client resolves an entry's flags through
    /// it rather than leaving each caller to spell the composition out.
    pub fn with_keywords(
        path: &MaildirFsPath,
        contents: &[u8],
        table: &BTreeMap<char, String>,
        header: Option<KeywordHeader>,
    ) -> Self {
        let mut flags = Self::with_dovecot(path, table);

        if let Some(header) = header {
            flags.extend_keywords(extract_keywords_header(contents, header));
        }

        flags
    }

    /// Adds raw keyword strings as [`MaildirFlag::Keyword`] entries.
    pub fn extend_keywords<I, S>(&mut self, keywords: I)
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        for k in keywords {
            self.flags.insert(MaildirFlag::Keyword(k.into()));
        }
    }

    /// Appends raw info-section letters written verbatim by
    /// [`fmt::Display`] (typically resolved dovecot slot letters).
    pub fn extend_letters<I>(&mut self, letters: I)
    where
        I: IntoIterator<Item = char>,
    {
        self.extra_letters.extend(letters);
    }

    /// Drains every [`MaildirFlag::Keyword`] variant out, returning
    /// the keyword strings in lexicographic order.
    pub fn drain_keywords(&mut self) -> Vec<String> {
        let keywords: BTreeSet<MaildirFlag> = self
            .flags
            .iter()
            .filter(|f| matches!(f, MaildirFlag::Keyword(_)))
            .cloned()
            .collect();

        for f in &keywords {
            self.flags.remove(f);
        }

        keywords
            .into_iter()
            .filter_map(|f| match f {
                MaildirFlag::Keyword(s) => Some(s),
                _ => None,
            })
            .collect()
    }
}

impl FromIterator<MaildirFlag> for MaildirFlags {
    fn from_iter<I: IntoIterator<Item = MaildirFlag>>(iter: I) -> Self {
        MaildirFlags {
            flags: iter.into_iter().collect(),
            extra_letters: BTreeSet::new(),
        }
    }
}

/// A single Maildir flag: a standard IANA letter or a custom keyword.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MaildirFlag {
    /// The message has been forwarded (`P`).
    Passed,
    /// The message has been replied to (`R`).
    Replied,
    /// The message has been read (`S`).
    Seen,
    /// The message is marked for deletion (`T`).
    Trashed,
    /// The message is a draft (`D`).
    Draft,
    /// The message is flagged for later attention (`F`).
    Flagged,
    /// Custom keyword with no info-section letter; serialised via
    /// dovecot-keywords or a configured header.
    Keyword(String),
}

impl MaildirFlag {
    /// Maps an info-section letter to its named flag, or [`None`] for
    /// any other character.
    pub fn from_char(c: char) -> Option<MaildirFlag> {
        match c {
            'P' => Some(MaildirFlag::Passed),
            'R' => Some(MaildirFlag::Replied),
            'S' => Some(MaildirFlag::Seen),
            'T' => Some(MaildirFlag::Trashed),
            'D' => Some(MaildirFlag::Draft),
            'F' => Some(MaildirFlag::Flagged),
            c => {
                trace!("invalid maildir flag {c:?}, ignoring");
                None
            }
        }
    }

    /// Builds a [`MaildirFlag::Keyword`] from `s`.
    pub fn keyword(s: impl Into<String>) -> Self {
        Self::Keyword(s.into())
    }

    /// Returns the keyword string when this is a
    /// [`MaildirFlag::Keyword`], else [`None`].
    pub fn as_keyword(&self) -> Option<&str> {
        match self {
            Self::Keyword(s) => Some(s.as_str()),
            _ => None,
        }
    }
}

impl fmt::Display for MaildirFlag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Passed => write!(f, "P"),
            Self::Replied => write!(f, "R"),
            Self::Seen => write!(f, "S"),
            Self::Trashed => write!(f, "T"),
            Self::Draft => write!(f, "D"),
            Self::Flagged => write!(f, "F"),
            // NOTE: Keyword has no letter encoding; serialised via the
            // dovecot-keywords file or a header instead.
            Self::Keyword(_) => Ok(()),
        }
    }
}

/// Header used to carry custom keywords inline with the message body.
///
/// `XKeywords` follows the OfflineIMAP / mbsync convention (comma-
/// separated); `XLabel` follows mutt / notmuch (space-separated).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeywordHeader {
    /// The `X-Keywords` header, comma-separated.
    XKeywords,
    /// The `X-Label` header, space-separated.
    XLabel,
}

impl KeywordHeader {
    /// Returns the on-the-wire header name.
    pub fn header_name(&self) -> &'static str {
        match self {
            Self::XKeywords => "X-Keywords",
            Self::XLabel => "X-Label",
        }
    }

    /// Returns the character separating keywords in the header value.
    pub fn separator(&self) -> char {
        match self {
            Self::XKeywords => ',',
            Self::XLabel => ' ',
        }
    }
}

impl FromStr for KeywordHeader {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "x-keywords" | "xkeywords" | "x_keywords" => Ok(Self::XKeywords),
            "x-label" | "xlabel" | "x_label" => Ok(Self::XLabel),
            _ => Err("expected x-keywords or x-label"),
        }
    }
}

impl fmt::Display for KeywordHeader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.header_name())
    }
}

#[cfg(test)]
mod tests {
    use alloc::{string::ToString, vec::Vec};

    use crate::{flag::*, path::MaildirFsPath};

    fn path(name: &str) -> MaildirFsPath {
        MaildirFsPath::new(name)
    }

    fn keywords(flags: &MaildirFlags) -> Vec<&str> {
        flags.iter().filter_map(MaildirFlag::as_keyword).collect()
    }

    #[test]
    fn standard_letters_alone_without_a_table_or_a_header() {
        let flags = MaildirFlags::with_keywords(
            &path("/m/cur/1614632942.M1P2.host:2,FRS"),
            b"X-Keywords: NonJunk\r\n\r\nbody",
            &BTreeMap::new(),
            None,
        );

        // NOTE: written in variant order, not in filename order.
        assert_eq!(flags.to_string(), "RSF");
        assert!(keywords(&flags).is_empty());
    }

    #[test]
    fn unknown_letters_are_dropped_without_a_table() {
        let flags = MaildirFlags::with_keywords(
            &path("/m/cur/1614632942.M1P2.host:2,Sab"),
            b"",
            &BTreeMap::new(),
            None,
        );

        assert_eq!(flags.to_string(), "S");
    }

    #[test]
    fn slot_letters_resolve_through_the_table() {
        let table = BTreeMap::from([('a', "NonJunk".to_string()), ('b', "Later".to_string())]);
        let flags = MaildirFlags::with_keywords(
            &path("/m/cur/1614632942.M1P2.host:2,Sab"),
            b"",
            &table,
            None,
        );

        assert_eq!(keywords(&flags), ["Later", "NonJunk"]);
    }

    #[test]
    fn header_keywords_join_the_letters() {
        let flags = MaildirFlags::with_keywords(
            &path("/m/cur/1614632942.M1P2.host:2,S"),
            b"X-Keywords: NonJunk, Work\r\n\r\nbody",
            &BTreeMap::new(),
            Some(KeywordHeader::XKeywords),
        );

        assert!(flags.iter().any(|flag| *flag == MaildirFlag::Seen));
        assert_eq!(keywords(&flags), ["NonJunk", "Work"]);
    }

    #[test]
    fn both_conventions_compose() {
        let table = BTreeMap::from([('a', "NonJunk".to_string())]);
        let flags = MaildirFlags::with_keywords(
            &path("/m/cur/1614632942.M1P2.host:2,a"),
            b"X-Label: work personal\r\n\r\nbody",
            &table,
            Some(KeywordHeader::XLabel),
        );

        assert_eq!(keywords(&flags), ["NonJunk", "personal", "work"]);
    }

    #[test]
    fn a_size_extension_is_neither_a_letter_nor_a_slot() {
        let table = BTreeMap::from([('a', "NonJunk".to_string())]);
        let flags = MaildirFlags::with_keywords(
            &path("/m/new/1614632942.M1P2.host,S=1234,W=1300"),
            b"",
            &table,
            None,
        );

        assert!(flags.is_empty());
    }
}
