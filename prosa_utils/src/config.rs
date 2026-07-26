//! Module for ProSA configuration object
//!
//! <svg width="40" height="40">
#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/doc_assets/settings.svg"))]
//! </svg>

use std::{io, path::PathBuf, process::Command};

use thiserror::Error;
use uuid::Uuid;

// Feature openssl or rusttls,...
pub mod ssl;

// Feature opentelemetry
#[cfg(feature = "config-observability")]
pub mod observability;

// Feature tracing
#[cfg(feature = "config-observability")]
pub mod tracing;

pub mod url;
pub use url::url_authentication;

/// Error define for configuration object
#[derive(Debug, Error)]
pub enum ConfigError {
    /// Error that indicate a wrong path format in filesystem
    #[error("The config parameter {0} have an incorrect value `{1}`")]
    WrongValue(String, String),
    /// Error that indicate a wrong path format pattern in filesystem
    #[error("The path `{0}` provided don't match the pattern `{1}`")]
    WrongPathPattern(String, glob::PatternError),
    /// Error that indicate a wrong path format in filesystem
    #[error("The path `{0}` provided is not correct")]
    WrongPath(PathBuf),
    /// Error on a file read
    #[error("The file `{0}` can't be read `{1}`")]
    IoFile(String, std::io::Error),
    #[cfg(feature = "config-openssl")]
    /// SSL error
    #[error("Openssl error `{0}`")]
    OpenSsl(#[from] openssl::error::ErrorStack),
}

impl From<ConfigError> for io::Error {
    fn from(err: ConfigError) -> Self {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("ProSA Config error: {}", err),
        )
    }
}

/// Method to try get the country name from the OS
pub fn os_country() -> Option<String> {
    if let Some(lang) = option_env!("LANG") {
        let language = if let Some(pos) = lang.find('.') {
            &lang[..pos]
        } else {
            lang
        };

        if let Some(pos) = language.find('_') {
            return Some(String::from(&language[pos + 1..]));
        }
    }

    None
}

/// Method to try get the hostname from the OS
pub fn hostname() -> Option<String> {
    cfg_select! {
        target_family = "unix" => {
            if let Ok(host) = std::env::var("HOSTNAME").map(|h| h.trim().to_string())
                && !host.is_empty()
                && !host.contains('\n')
            {
                return Some(host);
            }

            Command::new("hostname")
                .arg("-s")
                .output()
                .ok()
                .and_then(|h| {
                    str::from_utf8(h.stdout.trim_ascii())
                        .ok()
                        .filter(|h| !h.is_empty() && !h.contains('\n'))
                        .map(|h| h.to_string())
                })
        }
        target_family = "windows" => {
            Command::new("hostname").output().ok().and_then(|h| {
                str::from_utf8(h.stdout.trim_ascii())
                    .ok()
                    .filter(|h| !h.is_empty() && !h.contains('\n'))
                    .map(|h| h.to_string())
            })
        }
        _ => None
    }
}

/// Method to get a consistant host ID (UUID v1 or UUID v4) useful for `service.instance.id`
pub fn hostid() -> String {
    cfg_select! {
        target_os = "linux" => {
            if let Ok(machine_id) = std::fs::read_to_string("/etc/machine-id")
                && let Ok(machine_uuid) = Uuid::parse_str(machine_id.trim())
            {
                return machine_uuid.to_string();
            }
        }
        target_os = "macos" => {
            if let Ok(output) = Command::new("ioreg")
                .args(["-rd1", "-c", "IOPlatformExpertDevice"])
                .output()
                && output.status.success()
                && let Ok(output_str) = String::from_utf8(output.stdout)
            {
                for line in output_str.lines() {
                    if line.contains("IOPlatformUUID")
                        && let Some(value) = line.split('"').nth(3)
                        && let Ok(machine_uuid) = Uuid::parse_str(value.trim())
                    {
                        return machine_uuid.to_string();
                    }
                }
            }
        }
        _ => {}
    }

    if let Some(hostname) = hostname() {
        let mut node_id = [0u8; 6];
        let len = hostname.len().min(6);
        node_id[..len].copy_from_slice(&hostname.as_bytes()[hostname.len() - len..]);

        Uuid::now_v1(&node_id).to_string()
    } else {
        Uuid::new_v4().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_os_country() {
        let country = os_country();
        if let Some(cn) = country {
            assert_eq!(2, cn.len());
        }
    }

    #[test]
    fn test_hostname() {
        let host = hostname();
        if let Some(hn) = host {
            assert!(!hn.is_empty());
        }
    }

    #[test]
    fn test_hostid() {
        let host_id = hostid();
        assert_eq!(host_id.len(), 36);
    }
}
