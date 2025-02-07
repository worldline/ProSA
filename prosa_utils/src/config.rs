//! Module for ProSA configuration object
//!
//! <svg width="40" height="40">
#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/doc_assets/settings.svg"))]
//! </svg>

use thiserror::Error;

// Feature openssl or rusttls,...
#[cfg(feature = "config-openssl")]
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
    /// Error that indicate a wrong path format in filesystem
    #[error("The path `{0}` provided is not correct `{1}`")]
    WrongPath(String, glob::PatternError),
    /// Error on a file read
    #[error("The file `{0}` can't be read `{1}`")]
    IoFile(String, std::io::Error),
    #[cfg(feature = "config-openssl")]
    /// SSL error
    #[error("Openssl error `{0}`")]
    Ssl(#[from] openssl::error::ErrorStack),
}

/// Method to get the country name from the OS
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
}
