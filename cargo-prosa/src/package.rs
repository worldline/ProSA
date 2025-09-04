//! Module to package a ProSA

const ASSETS_SYSTEMD_J2: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/systemd.j2"));

/// Module to package ProSA into a container image
pub mod container;

/// Module to package ProSA in debian package (`.deb`)
pub mod deb;

/// Module to package ProSA in Red Hat package (`.rpm`)
pub mod rpm;

#[cfg(any(target_os = "linux", target_os = "macos"))]
/// Module to install ProSA on Linux or MacOS
pub mod install;
