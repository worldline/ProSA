//! Module to define an injector processor to inject transaction at a regulated flow

/// Definition of the injector processor
///
/// <svg width="40" height="40">
#[doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/doc_assets/proc.svg"))]
/// </svg>
pub mod proc;

/// Definition of the injector adaptor
///
/// <svg width="40" height="40">
#[doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/doc_assets/adaptor.svg"))]
/// </svg>
pub mod adaptor;
