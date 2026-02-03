//! Module for ProSA configuration object
//!
//! <svg width="40" height="40">
#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/doc_assets/settings.svg"))]
//! </svg>

use std::{path::PathBuf, process::Command};

use thiserror::Error;
use url::Url;
use base64::{Engine as _, engine::general_purpose::STANDARD};

// Feature openssl or rusttls,...
pub mod ssl;

// Feature opentelemetry
#[cfg(feature = "config-observability")]
pub mod observability;

// Feature tracing
#[cfg(feature = "config-observability")]
pub mod tracing;

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
    #[cfg(target_family = "unix")]
    if let Some(host) = option_env!("HOSTNAME").map(str::trim)
        && !host.is_empty()
        && !host.contains('\n')
    {
        return Some(String::from(host));
    }

    #[cfg(target_family = "unix")]
    return Command::new("hostname")
        .arg("-s")
        .output()
        .ok()
        .and_then(|h| {
            str::from_utf8(h.stdout.trim_ascii())
                .ok()
                .filter(|h| !h.is_empty() && !h.contains('\n'))
                .map(|h| h.to_string())
        });

    #[cfg(target_family = "windows")]
    return Command::new("hostname").output().ok().and_then(|h| {
        str::from_utf8(h.stdout.trim_ascii())
            .ok()
            .filter(|h| !h.is_empty() && !h.contains('\n'))
            .map(|h| h.to_string())
    });

    #[cfg(all(not(target_family = "unix"), not(target_family = "windows")))]
    return None;
}

/// Method to get authentication value out of URL username/password
///
/// - If user password is provided, it return *Basic* authentication with base64 encoded username:password
/// - If only password is provided, it return *Bearer* authentication with the password as token
///
/// ```
/// use url::Url;
/// use prosa::io::stream::TargetSetting;
///
/// let basic_auth_target = Url::parse("http://user:pass@localhost:8080").unwrap();
/// assert_eq!(Some(String::from("Basic dXNlcjpwYXNz")), url_authentication(&basic_auth_target));
///
/// let bearer_auth_target = Url::parse("http://:token@localhost:8080").unwrap();
/// assert_eq!(Some(String::from("Bearer token")), url_authentication(&bearer_auth_target));
/// ```
pub fn url_authentication(url: &Url) -> Option<String> {
    if let Some(password) = url.password() {
        if url.username().is_empty() {
            Some(format!("Bearer {password}"))
        } else {
            Some(format!(
                "Basic {}",
                STANDARD.encode(format!("{}:{}", url.username(), password))
            ))
        }
    } else {
        None
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
    fn test_url_authentication_basic() {
        let basic_auth_target = Url::parse("http://user:pass@localhost:8080").unwrap();
        assert_eq!(Some(String::from("Basic dXNlcjpwYXNz")), url_authentication(&basic_auth_target));
    }

    #[test]
    fn test_url_authentication_bearer() {
        let bearer_auth_target = Url::parse("http://:token@localhost:8080").unwrap();
        assert_eq!(Some(String::from("Bearer token")), url_authentication(&bearer_auth_target));
    }
}
