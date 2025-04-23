//! Module to package a ProSA

/// Module to package ProSA into a container image
pub mod container;

/// Module to package ProSA in debian package (`.deb`)
pub mod deb;

/// Module to package ProSA in Red Hat package (`.rpm`)
pub mod rpm;
