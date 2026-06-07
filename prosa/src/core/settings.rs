//!
//! <svg width="40" height="40">
#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/doc_assets/settings.svg"))]
//! </svg>

use std::{
    ffi::OsStr,
    fs,
    future::Future,
    io::{self, Write},
    path::Path,
};

use config::{Config, ConfigBuilder, ValueKind, builder::DefaultState};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use prosa_utils::config::observability::Observability;
use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::sync::mpsc;

use super::proc::ProcBusParam;

/// Re-export of prosa_utils for observability config
pub use prosa_utils::config::{observability, tracing};

/// Implement the trait [`Settings`]
pub use prosa_macros::settings;

/// Running settings of a ProSA
/// Need to be implemented by the top settings layer of a ProSA
///
/// ```
/// use prosa::core::settings::{settings, Settings};
/// use serde::{Deserialize, Serialize};
///
/// // My ProSA setting structure
/// #[settings]
/// #[derive(Debug, Deserialize, Serialize)]
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
/// use prosa::core::settings::{Settings, observability::Observability};
/// use serde::{Deserialize, Serialize};
///
/// #[derive(Debug, Deserialize, Serialize)]
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

/// Method to create a `ConfigBuilder` from a path. It can be
/// - a folder with multiple configuration files in it
/// - a file with the entire configuration in it
pub fn get_config_builder(path: &str) -> io::Result<ConfigBuilder<DefaultState>> {
    let mut builder = Config::builder();

    let mut path_attr = std::fs::metadata(path)?;
    if path_attr.is_symlink() {
        path_attr = std::fs::metadata(fs::read_link(path)?)?;
    }

    if path_attr.is_file() {
        Ok(builder.add_source(config::File::with_name(path)))
    } else if path_attr.is_dir() {
        for entry in fs::read_dir(path)? {
            let path_subdir = entry?.path();
            if path_subdir.is_file()
                && path_subdir
                    .extension()
                    .and_then(OsStr::to_str)
                    .is_some_and(|ext| matches!(ext, "yml" | "yaml" | "toml"))
            {
                builder = builder.add_source(config::File::from(path_subdir));
            }
        }

        Ok(builder)
    } else {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            format!("Unrecognize filetype for path `{path}`"),
        ))
    }
}

/// Loaded ProSA configuration.
#[derive(Clone, Debug)]
pub struct ProsaConfig {
    config: Config,
}

impl ProsaConfig {
    /// Create a ProSA configuration wrapper from a loaded [`Config`].
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    /// Load a ProSA configuration from a file or directory path.
    pub fn from_path(config_path: &str) -> Result<Self, config::ConfigError> {
        get_config_builder(config_path)
            .map_err(|e| config::ConfigError::Foreign(Box::new(e)))?
            .add_source(
                config::Environment::with_prefix("PROSA")
                    .try_parsing(true)
                    .separator("_")
                    .list_separator(" "),
            )
            .build()
            .map(Self::new)
    }

    /// Access the underlying loaded configuration.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Deserialize the full configuration.
    pub fn try_deserialize<C>(&self) -> Result<C, config::ConfigError>
    where
        C: DeserializeOwned,
    {
        self.config.clone().try_deserialize()
    }

    /// Deserialize a processor configuration from its processor name.
    pub fn get_proc<C>(&self, proc: &impl ProcBusParam) -> Result<C, config::ConfigError>
    where
        C: DeserializeOwned,
    {
        self.config.get::<C>(&proc.get_proc_config_key())
    }

    /// Check if this configuration differs from another loaded configuration.
    pub fn has_changed(&self, new: &Self) -> bool {
        self.config.cache != new.config.cache
    }

    /// Check if one processor configuration differs from another loaded configuration.
    pub fn has_proc_changed(&self, new: &Self, proc_config_key: &str) -> bool {
        if let (ValueKind::Table(current_table), ValueKind::Table(new_table)) =
            (&self.config.cache.kind, &new.config.cache.kind)
        {
            current_table.get(proc_config_key) != new_table.get(proc_config_key)
        } else {
            false
        }
    }
}

impl From<Config> for ProsaConfig {
    fn from(config: Config) -> Self {
        Self::new(config)
    }
}

impl From<ProsaConfig> for Config {
    fn from(prosa_config: ProsaConfig) -> Self {
        prosa_config.config
    }
}

/// Watches a configuration path and exposes native file change events.
pub struct ConfigWatcher {
    _watcher: RecommendedWatcher,
    events: mpsc::UnboundedReceiver<notify::Result<Event>>,
}

impl ConfigWatcher {
    /// Wait for the next configuration file system event.
    pub async fn changed(&mut self) -> Option<notify::Result<Event>> {
        self.events.recv().await
    }
}

/// Create a watcher for the configuration file or directory.
pub fn watch_config_path(config_path: &str) -> notify::Result<ConfigWatcher> {
    let (tx, events) = mpsc::unbounded_channel();
    let mut watcher = notify::recommended_watcher(move |event| {
        let _ = tx.send(event);
    })?;

    watcher.watch(Path::new(config_path), RecursiveMode::NonRecursive)?;

    Ok(ConfigWatcher {
        _watcher: watcher,
        events,
    })
}

/// Filter out file system events that cannot affect configuration content.
pub fn is_config_reload_event(event: &Event) -> bool {
    matches!(
        event.kind,
        EventKind::Any | EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    )
}

/// Watch and reload a configuration path on file system changes.
pub async fn watch_config_reload<S, LoadConfig, ApplyConfig, ApplyFuture>(
    config_path: String,
    mut current_config: ProsaConfig,
    mut load_config: LoadConfig,
    mut apply_config: ApplyConfig,
) where
    S: DeserializeOwned,
    LoadConfig: FnMut(&str) -> Result<ProsaConfig, config::ConfigError>,
    ApplyConfig: FnMut(S, ProsaConfig) -> ApplyFuture,
    ApplyFuture: Future<Output = bool>,
{
    let mut config_watcher = match watch_config_path(&config_path) {
        Ok(config_watcher) => config_watcher,
        Err(err) => {
            log::warn!("Can't watch configuration {config_path}: {err}");
            return;
        }
    };

    loop {
        match config_watcher.changed().await {
            Some(Ok(event)) if is_config_reload_event(&event) => {}
            Some(Ok(_)) => continue,
            Some(Err(err)) => {
                log::warn!("Error watching configuration {config_path}: {err}");
                continue;
            }
            None => {
                log::warn!("Configuration watcher stopped for {config_path}");
                return;
            }
        }

        match load_config(&config_path) {
            Ok(new_config) if current_config.has_changed(&new_config) => {
                match new_config.try_deserialize::<S>() {
                    Ok(settings) => {
                        if apply_config(settings, new_config.clone()).await {
                            current_config = new_config;
                        }
                    }
                    Err(err) => {
                        log::error!("Configuration changed but can't be deserialized: {err}")
                    }
                }
            }
            Ok(_) => {}
            Err(err) => log::warn!("Can't reload configuration {config_path}: {err}"),
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

    #[test]
    fn test_proc_config_hash_change() -> Result<(), config::ConfigError> {
        let current = ProsaConfig::new(
            Config::builder()
                .set_override("proc_1.service_name", "PROC_TEST")?
                .set_override("proc_1.tick_secs", 4)?
                .set_override("proc_2.service_name", "PROC_TEST_2")?
                .set_override("proc_2.tick_secs", 4)?
                .build()?,
        );
        let new = ProsaConfig::new(
            Config::builder()
                .set_override("proc_1.service_name", "PROC_TEST_UPDATED")?
                .set_override("proc_1.tick_secs", 4)?
                .set_override("proc_2.service_name", "PROC_TEST_2")?
                .set_override("proc_2.tick_secs", 4)?
                .build()?,
        );

        assert!(current.has_proc_changed(&new, "proc_1"));
        assert!(!current.has_proc_changed(&new, "proc_2"));

        Ok(())
    }
}
