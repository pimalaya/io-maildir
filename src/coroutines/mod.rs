//! Collection of I/O-free, resumable and composable Maildir state
//! machines.
//!
//! Coroutines emit [`FsInput`] requests that need to be processed by
//! an [io-fs runtime] in order to continue their progression.
//!
//! [`FsInput`]: io_fs::io::FsInput
//! [io-fs runtime]: https://docs.rs/io-fs/latest/io_fs/runtimes/index.html

pub mod flags_add;
pub mod flags_remove;
pub mod flags_set;
pub mod maildir_create;
pub mod maildir_delete;
pub mod maildir_list;
pub mod maildir_rename;
pub mod message_copy;
pub mod message_get;
pub mod message_list;
pub mod message_locate;
pub mod message_move;
pub mod message_store;
