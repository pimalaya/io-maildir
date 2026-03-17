//! Collection of I/O-free, resumable and composable Vdir state
//! machines.
//!
//! Coroutines emit [I/O] requests that need to be processed by
//! [runtimes] in order to continue their progression.
//!
//! [I/O]: crate::io
//! [runtimes]: crate::runtimes

#[path = "add-flags.rs"]
pub mod add_flags;
#[path = "copy-message.rs"]
pub mod copy_message;
#[path = "create-maildir.rs"]
pub mod create_maildir;
#[path = "delete-maildir.rs"]
pub mod delete_maildir;
#[path = "get-message.rs"]
pub mod get_message;
#[path = "list-maildirs.rs"]
pub mod list_maildirs;
#[path = "list-messages.rs"]
pub mod list_messages;
#[path = "locate-message-by-id.rs"]
pub mod locate_message_by_id;
#[path = "move-message.rs"]
pub mod move_message;
#[path = "remove-flags.rs"]
pub mod remove_flags;
#[path = "rename-maildir.rs"]
pub mod rename_maildir;
#[path = "set-flags.rs"]
pub mod set_flags;
#[path = "store-message.rs"]
pub mod store_message;
