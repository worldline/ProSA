#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/COPYRIGHT"))]
//!
//! [![github]](https://github.com/worldline/prosa)&ensp;[![crates-io]](https://crates.io/crates/prosa-utils)&ensp;[![docs-rs]](crate)
//!
#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/doc_assets/header_badges.md"))]
//!
//! Utils for ProSA

#![warn(missing_docs)]
pub mod msg;

#[cfg(feature = "config")]
pub mod config;
