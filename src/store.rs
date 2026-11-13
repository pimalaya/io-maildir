//! Layout-aware view of a Maildir tree.
//!
//! Wraps the filesystem root with the layout convention used to map a
//! logical [`MaildirPath`] to its on-disk [`MaildirFsPath`]. Plain Maildir is
//! the zero-subfolders degenerate case of fs.

use alloc::string::String;

use crate::{
    maildir::{CUR, NEW, SEPARATOR, TMP},
    path::{MaildirFsPath, MaildirPath},
};

/// Root folder holding one or more Maildirs, plus the layout used to
/// translate logical mailbox names to on-disk paths.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaildirStore {
    /// Filesystem root that holds the tree.
    pub root: MaildirFsPath,
    /// Maildir++ rules: root is itself INBOX, subfolders are dot-prefixed flat
    /// siblings of cur/new/tmp at the root, no nesting. Default off: fs layout
    /// (recursive descent through real nested directories).
    pub maildirpp: bool,
    /// Character between the id of an entry and its `2,<flags>` info
    /// section. The colon by default; `;` or `!` in a store kept on a
    /// filesystem that reserves the colon.
    pub separator: char,
}

impl Default for MaildirStore {
    fn default() -> Self {
        Self {
            root: MaildirFsPath::default(),
            maildirpp: false,
            separator: SEPARATOR,
        }
    }
}

impl MaildirStore {
    /// Resolves a logical mailbox path to its on-disk fs path under
    /// this store's layout.
    ///
    /// In fs layout, `"Foo/Bar"` → `<root>/Foo/Bar`. In Maildir++ layout,
    /// `"Foo/Bar"` → `<root>/.Foo.Bar`. The empty path resolves to the store
    /// root itself.
    pub fn resolve(&self, path: &MaildirPath) -> MaildirFsPath {
        if path.is_empty() {
            return self.root.clone();
        }

        if self.maildirpp {
            let mut segment = String::with_capacity(path.as_str().len() + 1);
            segment.push('.');
            for (i, component) in path.components().enumerate() {
                if i > 0 {
                    segment.push('.');
                }
                segment.push_str(component);
            }
            self.root.join(&segment)
        } else {
            self.root.join(path.as_str())
        }
    }

    /// Returns `true` when `path` names a folder below the root of this
    /// store: one segment at least, none of them `.` or `..`, nor `cur`,
    /// `new` or `tmp` in the fs layout.
    ///
    /// The fs layout keeps a folder beside the three subdirectories of
    /// its parent, so such a name resolves onto the parent's messages.
    /// A store whose separator is not the colon sits on a filesystem
    /// where the backslash separates paths too, so it takes none.
    pub fn names_folder(&self, path: &MaildirPath) -> bool {
        let subdir = |name: &str| !self.maildirpp && matches!(name, CUR | NEW | TMP);
        let reserved = |name: &str| self.separator != SEPARATOR && name.contains('\\');
        let mut names = path.components();
        path.names_folder() && !names.any(|name| subdir(name) || reserved(name))
    }

    /// Returns `true` when `path` names the root of this store or a
    /// folder below it, see [`Self::names_folder`].
    pub fn names_maildir(&self, path: &MaildirPath) -> bool {
        path.components().next().is_none() || self.names_folder(path)
    }

    /// Reverse of [`Self::resolve`]: given an on-disk fs path inside
    /// this store, returns the logical mailbox name. Returns `None` if
    /// `fs` is outside the store root.
    pub fn relative(&self, fs: &MaildirFsPath) -> Option<MaildirPath> {
        let rel = fs.strip_prefix(&self.root)?;
        if rel.is_empty() {
            return Some(MaildirPath::default());
        }

        if self.maildirpp {
            let stripped = rel.strip_prefix('.').unwrap_or(rel);
            let mut out = String::with_capacity(stripped.len());
            for (i, segment) in stripped.split('.').enumerate() {
                if i > 0 {
                    out.push('/');
                }
                out.push_str(segment);
            }
            Some(MaildirPath::from(out))
        } else {
            Some(MaildirPath::from(rel))
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        path::{MaildirFsPath, MaildirPath},
        store::MaildirStore,
    };

    #[test]
    fn resolve_fs_empty_is_root() {
        let store = MaildirStore {
            root: MaildirFsPath::from("/tmp/m"),
            maildirpp: false,
            separator: ':',
        };
        assert_eq!(store.resolve(&MaildirPath::default()).as_str(), "/tmp/m");
    }

    #[test]
    fn resolve_fs_nested() {
        let store = MaildirStore {
            root: MaildirFsPath::from("/tmp/m"),
            maildirpp: false,
            separator: ':',
        };
        assert_eq!(
            store.resolve(&MaildirPath::from("Foo/Bar")).as_str(),
            "/tmp/m/Foo/Bar"
        );
    }

    #[test]
    fn resolve_maildirpp_flat_dotted() {
        let store = MaildirStore {
            root: MaildirFsPath::from("/tmp/m"),
            maildirpp: true,
            separator: ':',
        };
        assert_eq!(
            store.resolve(&MaildirPath::from("Foo/Bar")).as_str(),
            "/tmp/m/.Foo.Bar"
        );
    }

    #[test]
    fn resolve_maildirpp_empty_is_root() {
        let store = MaildirStore {
            root: MaildirFsPath::from("/tmp/m"),
            maildirpp: true,
            separator: ':',
        };
        assert_eq!(store.resolve(&MaildirPath::default()).as_str(), "/tmp/m");
    }

    #[test]
    fn only_the_fs_layout_reserves_the_subdirectory_names() {
        let mut store = MaildirStore {
            root: MaildirFsPath::from("/tmp/m"),
            maildirpp: false,
            separator: ':',
        };

        for path in ["cur", "new", "tmp", "Foo/cur", "new/Foo", "", "..", "Foo/."] {
            assert!(!store.names_folder(&MaildirPath::from(path)), "{path}");
        }
        for path in ["Foo", "Foo/Bar", "New", "current", "Foo/tmp2"] {
            assert!(store.names_folder(&MaildirPath::from(path)), "{path}");
        }

        store.maildirpp = true;

        for path in ["cur", "new", "tmp", "Foo/cur", "new/Foo"] {
            assert!(store.names_folder(&MaildirPath::from(path)), "{path}");
        }
        for path in ["", "..", "Foo/."] {
            assert!(!store.names_folder(&MaildirPath::from(path)), "{path}");
        }
    }

    #[test]
    fn a_store_off_the_colon_takes_no_backslash_in_a_folder_name() {
        let mut store = MaildirStore::default();

        for path in ["Foo\\cur", "..\\Foo", "Foo/Bar\\Baz"] {
            assert!(store.names_folder(&MaildirPath::from(path)), "{path}");
        }

        store.separator = ';';

        for path in ["Foo\\cur", "..\\Foo", "Foo/Bar\\Baz"] {
            assert!(!store.names_folder(&MaildirPath::from(path)), "{path}");
        }
        assert!(store.names_folder(&MaildirPath::from("Foo/Bar")));
    }

    #[test]
    fn the_root_and_the_folders_below_it_are_maildirs() {
        for maildirpp in [false, true] {
            let store = MaildirStore {
                maildirpp,
                ..Default::default()
            };

            for path in ["", "/", "Foo"] {
                assert!(store.names_maildir(&MaildirPath::from(path)), "{path}");
            }
            for path in [".", "..", "Foo/.."] {
                assert!(!store.names_maildir(&MaildirPath::from(path)), "{path}");
            }
        }
    }

    #[test]
    fn relative_fs_round_trip() {
        let store = MaildirStore {
            root: MaildirFsPath::from("/tmp/m"),
            maildirpp: false,
            separator: ':',
        };
        let logical = MaildirPath::from("Foo/Bar");
        let fs = store.resolve(&logical);
        assert_eq!(store.relative(&fs), Some(logical));
    }

    #[test]
    fn relative_maildirpp_round_trip() {
        let store = MaildirStore {
            root: MaildirFsPath::from("/tmp/m"),
            maildirpp: true,
            separator: ':',
        };
        let logical = MaildirPath::from("Foo/Bar");
        let fs = store.resolve(&logical);
        assert_eq!(store.relative(&fs), Some(logical));
    }

    #[test]
    fn relative_outside_store_is_none() {
        let store = MaildirStore {
            root: MaildirFsPath::from("/tmp/m"),
            maildirpp: false,
            separator: ':',
        };
        assert_eq!(store.relative(&MaildirFsPath::from("/other/path")), None);
    }
}
