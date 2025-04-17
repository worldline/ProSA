//! Module for ProSA internal messaging object

pub mod simple_string_tvf;
pub mod tvf;
pub mod value;

#[cfg(feature = "serde")]
pub mod serialize;

#[cfg(feature = "serde")]
pub mod deserialize;
