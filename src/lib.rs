#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]
#![doc = include_str!("../README.md")]

#[cfg_attr(feature = "client", macro_use)]
extern crate alloc;
#[cfg(feature = "client")]
extern crate std;

#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "client")]
pub mod coroutines;
#[cfg(feature = "client")]
pub mod flag;
#[cfg(feature = "client")]
pub mod maildir;
#[cfg(feature = "client")]
pub mod message;

pub use mail_parser as parser;
