//! I/O-free coroutine to create a Vdir item.

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
#[cfg(windows)]
use std::os::windows::fs::MetadataExt;
use std::{
    path::PathBuf,
    process,
    sync::atomic::{AtomicUsize, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use gethostname::gethostname;
use io_fs::{
    coroutines::{create_file::CreateFile, rename::Rename},
    error::{FsError, FsResult},
    io::FsIo,
};
use thiserror::Error;

use crate::{
    flag::Flags,
    maildir::{Maildir, MaildirSubdir},
    message::INFORMATIONAL_SUFFIX_SEPARATOR,
};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Errors that can occur during the coroutine progression.
#[derive(Clone, Debug, Error)]
pub enum StoreMaildirMessageError {
    /// An error occured during the file creation.
    #[error("Store Maildir message to tmp error")]
    CreateFile(#[source] FsError),
    /// An error occured during the file creation.
    #[error("Rename tmp message to new/cur error")]
    Rename(#[source] FsError),
}

/// Output emitted when the coroutine terminates its progression.
#[derive(Clone, Debug)]
pub enum StoreMaildirMessageResult {
    /// The coroutine successfully terminated its progression.
    Ok { id: String, path: PathBuf },

    /// The coroutine encountered an error.
    Err(StoreMaildirMessageError),

    /// An I/O needs to be processed in order to make the coroutine
    /// progress further.
    Io(FsIo),
}

#[derive(Debug)]
enum State {
    Create(CreateFile),
    Rename(Rename),
}

/// I/O-free coroutine to create a Vdir item.
#[derive(Debug)]
pub struct StoreMaildirMessage {
    subdir: MaildirSubdir,
    flags: Flags,
    id: String,
    tmp_path: PathBuf,
    final_path: PathBuf,
    state: State,
}

impl StoreMaildirMessage {
    /// Creates a new coroutine from the given item.
    pub fn new(maildir: Maildir, subdir: MaildirSubdir, flags: Flags, contents: Vec<u8>) -> Self {
        let tmp_path = maildir.tmp().to_owned();
        let final_path = maildir.subdir(&subdir).to_owned();

        let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        let secs = ts.as_secs();
        let nanos = ts.subsec_nanos();
        let counter = COUNTER.fetch_add(1, Ordering::SeqCst);
        let pid = process::id();
        let id = format!("{secs}.#{counter:x}M{nanos}P{pid}");

        let coroutine = CreateFile::new(tmp_path.join(&id), contents);

        Self {
            subdir,
            flags,
            id,
            tmp_path,
            final_path,
            state: State::Create(coroutine),
        }
    }

    /// Makes the coroutine progress.
    pub fn resume(&mut self, mut arg: Option<FsIo>) -> StoreMaildirMessageResult {
        loop {
            match &mut self.state {
                State::Create(coroutine) => {
                    let metadata = match coroutine.resume(arg.take()) {
                        FsResult::Ok(metadata) => metadata,
                        FsResult::Io(io) => return StoreMaildirMessageResult::Io(io),
                        FsResult::Err(err) => {
                            let err = StoreMaildirMessageError::CreateFile(err);
                            return StoreMaildirMessageResult::Err(err);
                        }
                    };

                    let tmp_path = self.tmp_path.join(&self.id);

                    #[cfg(unix)]
                    let dev = metadata.dev();
                    #[cfg(windows)]
                    let dev: u64 = 0;

                    #[cfg(unix)]
                    let ino = metadata.ino();
                    #[cfg(windows)]
                    let ino: u64 = 0;

                    let hostname = gethostname().into_string().unwrap();
                    self.id = format!("{}V{dev}I{ino}.{hostname}", self.id);

                    let mut id_with_info = self.id.clone();

                    if let MaildirSubdir::Cur = self.subdir {
                        id_with_info.push(INFORMATIONAL_SUFFIX_SEPARATOR);
                        id_with_info.push_str("2,");
                        id_with_info.push_str(&self.flags.to_string());
                    }

                    if self.final_path.is_dir() {
                        self.final_path = self.final_path.join(id_with_info);
                    }

                    let coroutine = Rename::new(Some((tmp_path, self.final_path.clone())));
                    self.state = State::Rename(coroutine);
                }
                State::Rename(coroutine) => {
                    break match coroutine.resume(arg.take()) {
                        FsResult::Ok(()) => StoreMaildirMessageResult::Ok {
                            id: self.id.clone(),
                            path: self.final_path.clone(),
                        },
                        FsResult::Io(io) => StoreMaildirMessageResult::Io(io),
                        FsResult::Err(err) => {
                            let err = StoreMaildirMessageError::Rename(err);
                            return StoreMaildirMessageResult::Err(err);
                        }
                    }
                }
            }
        }
    }
}
