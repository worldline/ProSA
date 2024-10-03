//!
//! <svg width="40" height="40">
#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/doc_assets/settings.svg"))]
//! </svg>

use std::io::{self, Write};

use prosa_utils::config::observability::Observability;
use serde::Serialize;

/// Implement the trait [`Settings`]
pub use prosa_macros::settings;

/// Running settings of a ProSA
/// Need to be implemented by the top settings layer of a ProSA
///
/// ```
/// use prosa::core::settings::{settings, Settings};
/// use serde::Serialize;
///
/// // My ProSA setting structure
/// #[settings]
/// #[derive(Debug, Serialize)]
/// struct MySettings {
///     test_val: String
/// }
///
/// #[settings]
/// impl Default for MySettings {
///     fn default() -> Self {
///         MySettings {
///             test_val: "test".into(),
///         }
///     }
/// }
///
/// assert_eq!("test", MySettings::default().test_val);
/// ```
///
/// is equivalent to
///
/// ```
/// use prosa::core::settings::Settings;
/// use prosa_utils::config::observability::Observability;
/// use serde::Serialize;
///
/// #[derive(Debug, Serialize)]
/// struct MySameSettings {
///     test_val: String,
///     name: Option<String>,
///     observability: Observability,
/// }
///
/// impl Settings for MySameSettings {
///     fn get_prosa_name(&self) -> String {
///         if let Some(name) = &self.name {
///             name.clone()
///         } else if let Ok(hostname) = std::env::var("HOSTNAME") {
///             format!("prosa-{}", hostname)
///         } else {
///             String::from("prosa")
///         }
///     }
///
///     fn set_prosa_name(&mut self, name: String) {
///         self.name = Some(name);
///     }
///
///     fn get_observability(&self) -> &Observability {
///         &self.observability
///     }
/// }
///
/// impl Default for MySameSettings {
///     fn default() -> Self {
///         MySameSettings {
///             test_val: "test".into(),
///             name: None,
///             observability: Observability::default(),
///         }
///     }
/// }
///
/// assert_eq!("test", MySameSettings::default().test_val);
/// ```
pub trait Settings: Serialize {
    /// Getter of the ProSA running name
    fn get_prosa_name(&self) -> String;
    /// Setter of the ProSA running name
    fn set_prosa_name(&mut self, name: String);
    /// Getter of the Observability configuration
    fn get_observability(&self) -> &Observability;
    /// Method to write the configuration into a file
    fn write_config(&self, config_path: &str) -> io::Result<()> {
        let mut f = std::fs::File::create(std::path::Path::new(config_path))?;
        writeln!(f, "# ProSA default settings")?;
        if config_path.ends_with(".toml") {
            writeln!(
                f,
                "{}",
                toml::to_string(&self)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?
            )
        } else {
            writeln!(
                f,
                "{}",
                serde_yaml::to_string(&self)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prosa_macros::settings;

    extern crate self as prosa;

    #[test]
    fn test_settings() {
        #[settings]
        #[derive(Debug, Serialize)]
        struct TestSettings {
            name_test: String,
            name_test2: String,
        }

        #[settings]
        impl Default for TestSettings {
            fn default() -> Self {
                let _test_settings = TestSettings {
                    name_test: "test".into(),
                    name_test2: "test2".into(),
                };

                TestSettings {
                    name_test: "test".into(),
                    name_test2: "test2".into(),
                }
            }
        }

        let test_settings = TestSettings::default();
        assert_eq!("test", test_settings.name_test);
        assert_eq!("test2", test_settings.name_test2);
    }
}
