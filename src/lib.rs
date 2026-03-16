#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]
#![doc = include_str!("../README.md")]

pub mod coroutines;
pub mod flag;
pub mod maildir;
pub mod message;

pub use mail_parser as types;
