//! Layout-aware view of a Maildir tree.
//!
//! Wraps the filesystem root with the layout convention used to map a
//! logical [`MaildirPath`] to its on-disk [`MaildirFsPath`]. Plain Maildir is
//! the zero-subfolders degenerate case of fs.

use alloc::string::String;

use crate::{
    maildir::{CUR, NEW, TMP},
    path::{MaildirFsPath, MaildirPath},
};

/// Root folder holding one or more Maildirs, plus the layout used to
/// translate logical mailbox names to on-disk paths.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MaildirStore {
    /// Filesystem root that holds the tree.
    pub root: MaildirFsPath,
    /// Maildir++ rules: root is itself INBOX, subfolders are dot-prefixed flat
    /// siblings of cur/new/tmp at the root, no nesting. Default off: fs layout
    /// (recursive descent through real nested directories).
    pub maildirpp: bool,
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
    pub fn names_folder(&self, path: &MaildirPath) -> bool {
        let reserved = |name: &str| !self.maildirpp && matches!(name, CUR | NEW | TMP);
        path.names_folder() && !path.components().any(reserved)
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
        };
        assert_eq!(store.resolve(&MaildirPath::default()).as_str(), "/tmp/m");
    }

    #[test]
    fn resolve_fs_nested() {
        let store = MaildirStore {
            root: MaildirFsPath::from("/tmp/m"),
            maildirpp: false,
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
        };
        assert_eq!(store.resolve(&MaildirPath::default()).as_str(), "/tmp/m");
    }

    #[test]
    fn only_the_fs_layout_reserves_the_subdirectory_names() {
        let mut store = MaildirStore {
            root: MaildirFsPath::from("/tmp/m"),
            maildirpp: false,
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
    fn the_root_and_the_folders_below_it_are_maildirs() {
        for maildirpp in [false, true] {
            let root = MaildirFsPath::from("/tmp/m");
            let store = MaildirStore { root, maildirpp };

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
        };
        assert_eq!(store.relative(&MaildirFsPath::from("/other/path")), None);
    }
}
