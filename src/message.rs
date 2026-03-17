use std::{
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
};

use mail_parser::MessageParser;
use thiserror::Error;

use crate::maildir::{MaildirError, MaildirSubdir};

#[cfg(unix)]
pub static INFORMATIONAL_SUFFIX_SEPARATOR: char = ':';
#[cfg(windows)]
pub static INFORMATIONAL_SUFFIX_SEPARATOR: char = ';';

#[derive(Debug, Error)]
pub enum MessageError {
    #[error("Invalid parent for Maildir message at {0}")]
    InvalidParent(PathBuf),
    #[error(transparent)]
    Maidir(#[from] MaildirError),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Message {
    pub(crate) path: PathBuf,
    pub(crate) contents: Vec<u8>,
}

impl Message {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn id(&self) -> Option<&str> {
        let file_name = self.path.file_name()?;
        let file_name = file_name.to_str()?;

        let id = match file_name.rsplit_once(INFORMATIONAL_SUFFIX_SEPARATOR) {
            Some((id, _)) => id,
            None => file_name,
        };

        Some(id)
    }

    pub fn contents(&self) -> &[u8] {
        &self.contents
    }

    pub fn subdir(&self) -> Result<MaildirSubdir, MessageError> {
        Ok(MaildirSubdir::try_from(self.path.to_owned())?)
    }

    pub fn parsed(&self) -> Option<mail_parser::Message<'_>> {
        MessageParser::new().parse(&self.contents)
    }

    pub fn headers(&self) -> Option<mail_parser::Message<'_>> {
        MessageParser::new()
            .with_minimal_headers()
            .parse(&self.contents)
    }
}

impl From<Message> for Vec<u8> {
    fn from(msg: Message) -> Self {
        msg.contents
    }
}

impl From<(PathBuf, Vec<u8>)> for Message {
    fn from((path, contents): (PathBuf, Vec<u8>)) -> Self {
        Self { path, contents }
    }
}

impl Hash for Message {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.path.hash(state);
    }
}
