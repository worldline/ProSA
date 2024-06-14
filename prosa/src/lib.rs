#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/COPYRIGHT"))]
//!
//! [![github]](https://github.com/worldline/prosa)&ensp;[![crates-io]](https://crates.io/crates/prosa)&ensp;[![docs-rs]](crate)
//!
#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/doc_assets/header_badges.md"))]
//!
//! ProSA base library that define standard modules and include procedural macros
#![warn(missing_docs)]
#![deny(unreachable_pub)]

pub mod core;

pub mod event;

pub mod io;

pub mod stub;
