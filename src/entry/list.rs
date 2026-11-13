//! I/O-free coroutine listing entries (no body) in a Maildir.
//!
//! # Example
//!
//! ```rust,no_run
//! use io_maildir::{client::MaildirClient, entry::list::MaildirEntryList};
//!
//! let client = MaildirClient::new("/path/to/root");
//! let maildir = client.load_maildir("inbox").unwrap();
//!
//! let coroutine = MaildirEntryList::new(maildir);
//! let entries = client.run(coroutine).unwrap();
//!
//! for entry in &entries {
//!     println!("{}", entry.path());
//! }
//! ```

use core::fmt;

use alloc::collections::BTreeSet;

use log::debug;
use thiserror::Error;

use crate::{
    coroutine::*,
    entry::{MAX_DIR_READS, MaildirEntry},
    maildir::Maildir,
    path::MaildirFsPath,
};

/// Failure causes during a [`MaildirEntryList`] step.
#[derive(Clone, Debug, Error)]
pub enum MaildirEntryListError {
    /// A reply arrived that does not match the awaited step.
    #[error("Maildir messages list failed: unexpected arg {0:?}")]
    UnexpectedArg(Option<MaildirReply>),
    /// Every read listed a name that was gone when its kind was read.
    #[error("Maildir messages list failed: {0} kept changing while it was read")]
    Unstable(MaildirFsPath),
}

/// Lists every entry in a Maildir from one read of `new/` and `cur/`;
/// bodies are not read.
///
/// A name reported [`MaildirFileKind::Missing`] was renamed or removed
/// while the driver read: the subdirectories are read again rather than
/// the entry left out, and the listing fails if that keeps happening.
#[derive(Debug)]
pub struct MaildirEntryList {
    maildir: Maildir,
    state: State,
}

impl MaildirEntryList {
    /// Builds a coroutine listing every entry in the Maildir.
    pub fn new(maildir: Maildir) -> Self {
        Self {
            maildir,
            state: State::Start,
        }
    }

    /// Asks for the `reads`th read of the two subdirectories.
    fn read(
        &mut self,
        reads: u8,
    ) -> MaildirCoroutineState<MaildirYield, <Self as MaildirCoroutine>::Return> {
        let paths = BTreeSet::from([self.maildir.new(), self.maildir.cur()]);
        self.state = State::Read { reads };
        MaildirCoroutineState::Yielded(MaildirYield::WantsDirRead(paths))
    }
}

impl MaildirCoroutine for MaildirEntryList {
    type Yield = MaildirYield;
    type Return = Result<BTreeSet<MaildirEntry>, MaildirEntryListError>;

    fn resume(
        &mut self,
        arg: Option<MaildirReply>,
    ) -> MaildirCoroutineState<Self::Yield, Self::Return> {
        match (&mut self.state, arg) {
            (State::Start, None) => self.read(1),
            (State::Read { reads }, Some(MaildirReply::DirRead(entries))) => {
                let reads = *reads;
                let mut found = BTreeSet::new();
                let mut gone = false;

                for (path, kind) in entries.into_values().flatten() {
                    if path.file_name().is_none_or(|name| name.starts_with('.')) {
                        continue;
                    }

                    match kind {
                        MaildirFileKind::File => {
                            found.insert(MaildirEntry::from_path(path));
                        }
                        MaildirFileKind::Missing => gone = true,
                        MaildirFileKind::Dir | MaildirFileKind::Other => {}
                    }
                }

                if !gone {
                    debug!("listed {} entries", found.len());
                    return MaildirCoroutineState::Complete(Ok(found));
                }

                if reads == MAX_DIR_READS {
                    let err = MaildirEntryListError::Unstable(self.maildir.path().clone());
                    return MaildirCoroutineState::Complete(Err(err));
                }

                self.read(reads + 1)
            }
            (_, arg) => {
                let err = MaildirEntryListError::UnexpectedArg(arg);
                MaildirCoroutineState::Complete(Err(err))
            }
        }
    }
}

#[derive(Debug)]
enum State {
    Start,
    Read { reads: u8 },
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Start => f.write_str("start"),
            Self::Read { .. } => f.write_str("read subdirs"),
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::{
        string::{String, ToString},
        vec::Vec,
    };

    use crate::entry::list::*;

    const FILE: MaildirFileKind = MaildirFileKind::File;
    const MISSING: MaildirFileKind = MaildirFileKind::Missing;

    fn maildir() -> Maildir {
        Maildir::from_path("root")
    }

    /// Lists a Maildir holding the next of `reads` at each directory
    /// read, returning the paths listed and the requests it took.
    fn list(
        reads: &[&[(&str, MaildirFileKind)]],
    ) -> (Result<Vec<String>, MaildirEntryListError>, usize) {
        let mut cor = MaildirEntryList::new(maildir());
        let mut requests = 0;
        let mut arg = None;

        loop {
            let dirs = match cor.resume(arg.take()) {
                MaildirCoroutineState::Complete(out) => {
                    let paths = |entries: BTreeSet<MaildirEntry>| {
                        let entries = entries.iter();
                        entries.map(|entry| entry.path().to_string()).collect()
                    };
                    return (out.map(paths), requests);
                }
                MaildirCoroutineState::Yielded(MaildirYield::WantsDirRead(dirs)) => dirs,
                state => panic!("expected WantsDirRead, got {state:?}"),
            };

            let tree = reads[requests.min(reads.len() - 1)];
            requests += 1;
            arg = Some(MaildirReply::read(dirs, tree));
        }
    }

    #[test]
    fn empty_maildir_returns_empty() {
        let (out, requests) = list(&[&[]]);
        assert!(out.unwrap().is_empty());
        assert_eq!(requests, 1);
    }

    #[test]
    fn a_listing_keeps_the_files_and_costs_one_request() {
        let tree = [
            ("root/new/a", FILE),
            ("root/cur/b:2,S", FILE),
            ("root/cur/.hidden", FILE),
            ("root/cur/folder", MaildirFileKind::Dir),
            ("root/cur/socket", MaildirFileKind::Other),
            ("root/tmp/c", FILE),
        ];

        let (out, requests) = list(&[&tree]);
        assert_eq!(out.unwrap(), ["root/cur/b:2,S", "root/new/a"]);
        assert_eq!(requests, 1);
    }

    #[test]
    fn an_entry_gone_while_read_is_listed_under_the_name_it_took() {
        let first = [("root/cur/a:2,", MISSING), ("root/cur/b:2,S", FILE)];
        let second = [("root/cur/a:2,S", FILE), ("root/cur/b:2,S", FILE)];

        let (out, requests) = list(&[&first, &second]);
        assert_eq!(out.unwrap(), ["root/cur/a:2,S", "root/cur/b:2,S"]);
        assert_eq!(requests, 2);
    }

    #[test]
    fn a_maildir_that_keeps_changing_is_an_error_not_a_short_listing() {
        let tree = [("root/cur/a:2,", MISSING), ("root/cur/b:2,S", FILE)];

        let (out, requests) = list(&[&tree]);
        out.unwrap_err();
        assert_eq!(requests, 3);
    }

    #[test]
    fn unexpected_reply_returns_error() {
        let mut cor = MaildirEntryList::new(maildir());
        let _ = cor.resume(None);

        let err = expect_complete_err(&mut cor, Some(MaildirReply::DirCreate));
        assert!(matches!(err, MaildirEntryListError::UnexpectedArg(_)));
    }

    fn expect_complete_err(
        cor: &mut MaildirEntryList,
        arg: Option<MaildirReply>,
    ) -> MaildirEntryListError {
        match cor.resume(arg) {
            MaildirCoroutineState::Complete(Err(err)) => err,
            state => panic!("expected Complete(Err), got {state:?}"),
        }
    }
}
