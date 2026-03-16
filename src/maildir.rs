//! Module dedicated to the Vdir maildir.

use std::{
    ffi::{OsStr, OsString},
    fmt,
    hash::{Hash, Hasher},
    io,
    path::{Path, PathBuf},
    str::FromStr,
};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum MaildirError {
    #[error("Path at {0} is not a valid Maildir subdir")]
    InvalidSubdirPath(PathBuf),
    #[error("Path at {0} is not a valid Maildir")]
    InvalidMaildirPath(PathBuf),
    #[error("Missing subdir /{0} at Maildir {1}")]
    MissingSubdir(&'static str, PathBuf),
    #[error("Invalid Maildir subdir {0:?}: expected cur, new or tmp")]
    InvalidSubdir(OsString),
    #[error("Missing parent directory for {1}")]
    InvalidParent(#[source] io::Error, PathBuf),
    #[error("Invalid parent directory for {0}")]
    InvalidParentName(PathBuf),
}

pub const CUR: &str = "cur";
pub const NEW: &str = "new";
pub const TMP: &str = "tmp";

#[derive(Clone, Debug)]
pub enum MaildirSubdir {
    Cur,
    New,
    Tmp,
}

impl FromStr for MaildirSubdir {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            CUR => Ok(Self::Cur),
            NEW => Ok(Self::New),
            TMP => Ok(Self::Tmp),
            s => Err(format!("invalid maildir subdir {s}")),
        }
    }
}

impl fmt::Display for MaildirSubdir {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cur => write!(f, "{CUR}"),
            Self::New => write!(f, "{NEW}"),
            Self::Tmp => write!(f, "{TMP}"),
        }
    }
}

impl TryFrom<PathBuf> for MaildirSubdir {
    type Error = MaildirError;

    fn try_from(mut path: PathBuf) -> Result<Self, Self::Error> {
        // if path is a file, take the parent
        if path.is_file() {
            path = if let Some(parent_path) = path.parent() {
                parent_path.to_owned()
            } else {
                match path.canonicalize() {
                    Ok(path) => path,
                    Err(err) => return Err(MaildirError::InvalidParent(err, path)),
                }
            };
        };

        // at this point path should be a dir
        if !path.is_dir() {
            return Err(MaildirError::InvalidSubdirPath(path));
        }

        let Some(subdir_name) = path.file_name() else {
            return Err(MaildirError::InvalidParentName(path));
        };

        match subdir_name {
            name if name == CUR => Ok(Self::Cur),
            name if name == NEW => Ok(Self::New),
            name if name == TMP => Ok(Self::Tmp),
            name => Err(MaildirError::InvalidSubdir(name.to_os_string())),
        }
    }
}

impl TryFrom<&OsStr> for MaildirSubdir {
    type Error = MaildirError;

    fn try_from(value: &OsStr) -> Result<Self, Self::Error> {
        match value {
            value if value == CUR => Ok(Self::Cur),
            value if value == NEW => Ok(Self::New),
            value if value == TMP => Ok(Self::Tmp),
            value => Err(MaildirError::InvalidSubdir(value.to_os_string())),
        }
    }
}

/// The Vdir maildir.
///
/// Represents a directory that contains only files (items). A
/// maildir may have [metadata], as defined in the vdirsyncer
/// standard.
///
/// See [`crate::item::Item`].
///
/// [metadata]: https://vdirsyncer.pimutils.org/en/stable/vdir.html#metadata
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Maildir {
    root: PathBuf,
    cur: PathBuf,
    new: PathBuf,
    tmp: PathBuf,
}

impl Maildir {
    pub fn subdir(&self, subdir: &MaildirSubdir) -> &Path {
        match subdir {
            MaildirSubdir::Cur => self.cur(),
            MaildirSubdir::New => self.new(),
            MaildirSubdir::Tmp => self.tmp(),
        }
    }

    pub fn cur(&self) -> &Path {
        &self.cur
    }

    pub fn new(&self) -> &Path {
        &self.new
    }

    pub fn tmp(&self) -> &Path {
        &self.tmp
    }
}

impl Hash for Maildir {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.root.hash(state);
    }
}

impl AsRef<Path> for Maildir {
    fn as_ref(&self) -> &Path {
        self.root.as_ref()
    }
}

impl TryFrom<PathBuf> for Maildir {
    type Error = MaildirError;

    fn try_from(root: PathBuf) -> Result<Self, Self::Error> {
        if !root.is_dir() {
            return Err(MaildirError::InvalidMaildirPath(root));
        }

        let cur = root.join(CUR);
        if !cur.is_dir() {
            return Err(MaildirError::MissingSubdir(CUR, root));
        }

        let new = root.join(NEW);
        if !new.is_dir() {
            return Err(MaildirError::MissingSubdir(NEW, root));
        }

        let tmp = root.join(TMP);
        if !tmp.is_dir() {
            return Err(MaildirError::MissingSubdir(TMP, root));
        }

        Ok(Maildir {
            root,
            cur,
            new,
            tmp,
        })
    }
}

impl TryFrom<&Path> for Maildir {
    type Error = MaildirError;

    fn try_from(root: &Path) -> Result<Self, Self::Error> {
        root.to_path_buf().try_into()
    }
}
