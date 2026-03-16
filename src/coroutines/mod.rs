//! Collection of I/O-free, resumable and composable Vdir state
//! machines.
//!
//! Coroutines emit [I/O] requests that need to be processed by
//! [runtimes] in order to continue their progression.
//!
//! [I/O]: crate::io
//! [runtimes]: crate::runtimes

#[path = "copy-message.rs"]
pub mod copy_message;
#[path = "create-maildir.rs"]
pub mod create_maildir;
#[path = "get-message.rs"]
pub mod get_message;
#[path = "list-messages.rs"]
pub mod list_messages;
#[path = "locate-message-by-id.rs"]
pub mod locate_message_by_id;
#[path = "move-message.rs"]
pub mod move_message;
#[path = "store-message.rs"]
pub mod store_message;
