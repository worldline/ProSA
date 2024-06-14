#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/COPYRIGHT"))]
//!
//! [![github]](https://github.com/worldline/prosa)&ensp;[![crates-io]](https://crates.io/crates/cargo-prosa)&ensp;[![docs-rs]](crate)
//!
#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/doc_assets/header_badges.md"))]
//!
//! ProSA Cargo to build an entire ProSA
#![warn(missing_docs)]
#![deny(unreachable_pub)]

/// Configuration file name for ProSA. Define all processor list
pub const CONFIGURATION_FILENAME: &str = "ProSA.toml";

pub mod builder;

pub mod cargo;
