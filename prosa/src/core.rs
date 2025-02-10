//! Core modules to define the structure of a ProSA
//!
//! <svg width="40" height="40">
#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/doc_assets/main.svg"))]
//! </svg>
//! Main
//! <br>
//! <svg width="40" height="40">
#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/doc_assets/proc.svg"))]
//! </svg>
//! Processor
//! <br>
//! <svg width="40" height="40">
#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/doc_assets/settings.svg"))]
//! </svg>
//! Settings
//! <br>
//! <svg width="40" height="40">
#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/doc_assets/adaptor.svg"))]
//! </svg>
//! Adaptor
//!
//!

/// Adaptor module to adapt processor object and internal messages
pub mod adaptor;
/// Define error types for adaptor and processor
pub mod error;
/// The module define ProSA main processing to bring asynchronous handler for all processors
pub mod main;
/// Module to define ProSA messages
/// Messages are implement to be handle in an asynchronous context. Every data in message will be TVF formatted
pub mod msg;
/// A processor in ProSA is an element that process transactions and can contact external component. It's similar to a micro service.
/// It can answer to a service request or ask something to a service.
pub mod proc;
/// Service defined for a ProSA
pub mod service;
/// Settings module of a ProSA
pub mod settings;
