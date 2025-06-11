#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/COPYRIGHT"))]
//!
//! [![github]](https://github.com/worldline/ProSA)&ensp;[![crates-io]](https://crates.io/crates/prosa-utils)&ensp;[![docs-rs]](crate)&ensp;[![mdbook]](https://worldline.github.io/ProSA/)
//!
#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/doc_assets/header_badges.md"))]
//!
//! Utils for ProSA
#![warn(missing_docs)]
#![deny(unreachable_pub)]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]

pub mod msg;

#[cfg(feature = "config")]
pub mod config;
