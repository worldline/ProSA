//!
//! <svg width="40" height="40">
#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/doc_assets/settings.svg"))]
//! </svg>

use std::{
    collections::{HashMap, HashSet},
    ffi::OsStr,
    fs,
    future::Future,
    io::{self, Write},
    path::{Path, PathBuf},
};

use config::{Config, ConfigBuilder, File, ValueKind, builder::DefaultState};
use glob::glob;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use prosa_utils::config::observability::Observability;
use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::sync::mpsc;

use super::adaptor::Adaptor;
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
    add_config_path(Config::builder(), Path::new(path))
}

fn add_config_path(
    mut builder: ConfigBuilder<DefaultState>,
    path: &Path,
) -> io::Result<ConfigBuilder<DefaultState>> {
    let path_attr = fs::metadata(path)?;

    if path_attr.is_file() {
        Ok(builder.add_source(File::from(path.to_path_buf())))
    } else if path_attr.is_dir() {
        for path_subdir in sorted_dir_entries(path)? {
            let path_attr = fs::metadata(&path_subdir)?;
            if path_attr.is_dir() {
                builder = add_config_path(builder, &path_subdir)?;
            } else if path_attr.is_file()
                && path_subdir
                    .extension()
                    .and_then(OsStr::to_str)
                    .is_some_and(|ext| matches!(ext, "yml" | "yaml" | "toml"))
            {
                builder = builder.add_source(File::from(path_subdir));
            }
        }

        Ok(builder)
    } else {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            format!("Unrecognize filetype for path `{}`", path.display()),
        ))
    }
}

fn sorted_dir_entries(path: &Path) -> io::Result<Vec<PathBuf>> {
    let mut paths = fs::read_dir(path)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<io::Result<Vec<_>>>()?;
    paths.sort();
    Ok(paths)
}

fn sorted_paths(paths: HashSet<PathBuf>) -> Vec<PathBuf> {
    let mut paths = paths.into_iter().collect::<Vec<_>>();
    paths.sort();
    paths
}

fn config_watch_paths(path: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    if path.is_dir() || path.is_file() {
        paths.push(path.to_path_buf());
    }

    if path.is_file()
        && let Some(parent) = path.parent().filter(|parent| parent.exists())
    {
        paths.push(parent.to_path_buf());
    }

    paths
}

fn fallback_watch_paths(config_path: &str) -> Vec<PathBuf> {
    let fallback = if has_glob_pattern(config_path) {
        let mut parent = PathBuf::new();

        for component in Path::new(config_path).components() {
            let component = component.as_os_str();
            if component.to_str().is_some_and(has_glob_pattern) {
                break;
            }
            parent.push(component);
        }

        if parent.as_os_str().is_empty() {
            Some(PathBuf::from("."))
        } else if parent.exists() && parent.is_file() {
            parent.parent().map(Path::to_path_buf)
        } else {
            Some(parent)
        }
    } else {
        Path::new(config_path).parent().map(Path::to_path_buf)
    };

    fallback.filter(|path| path.exists()).into_iter().collect()
}

fn has_glob_pattern(config_path: &str) -> bool {
    config_path.chars().any(|c| matches!(c, '*' | '?' | '['))
}

/// Loaded ProSA configuration.
#[derive(Clone, Debug)]
pub struct ProsaConfig {
    config: Config,
    adaptor_configs: HashMap<String, Config>,
    adaptor_config_watch_paths: Vec<PathBuf>,
}

impl ProsaConfig {
    /// Load a ProSA configuration from a file or directory path.
    pub fn from_path(config_path: &str) -> Result<Self, config::ConfigError> {
        let config = get_config_builder(config_path)
            .map_err(|e| config::ConfigError::Foreign(Box::new(e)))?
            .add_source(
                config::Environment::with_prefix("PROSA")
                    .try_parsing(true)
                    .separator("_")
                    .list_separator(" "),
            )
            .build()?;

        Self::from_config(config)
    }

    /// Create a ProSA configuration wrapper and load all processor adaptor configs.
    pub fn from_config(config: Config) -> Result<Self, config::ConfigError> {
        let mut adaptor_configs = HashMap::new();
        let mut adaptor_config_watch_paths = HashSet::new();

        for (proc_config_key, config_path) in get_proc_adaptor_config_paths(&config) {
            let (adaptor_config, watch_paths) = Self::load_adaptor_config(&config_path)?;
            adaptor_configs.insert(proc_config_key, adaptor_config);
            adaptor_config_watch_paths.extend(watch_paths);
        }

        Ok(Self {
            config,
            adaptor_configs,
            adaptor_config_watch_paths: sorted_paths(adaptor_config_watch_paths),
        })
    }

    /// Load an adaptor config path or glob pattern and return its watcher paths.
    pub(crate) fn load_adaptor_config(
        config_path: &str,
    ) -> Result<(Config, Vec<PathBuf>), config::ConfigError> {
        let mut builder = Config::builder();
        let mut watch_paths = HashSet::new();
        let mut matched = false;

        for path in glob(config_path)
            .map_err(|e| {
                config::ConfigError::Message(format!(
                    "Wrong config path pattern `{config_path}`: `{e}`"
                ))
            })?
            .filter_map(Result::ok)
        {
            matched = true;
            watch_paths.extend(config_watch_paths(&path));
            builder = add_config_path(builder, &path)
                .map_err(|e| config::ConfigError::Foreign(Box::new(e)))?;
        }

        if !matched {
            watch_paths.extend(fallback_watch_paths(config_path));
        }

        Ok((builder.build()?, sorted_paths(watch_paths)))
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
    pub fn get_proc<C>(&self, proc: &(impl ProcBusParam + ?Sized)) -> Result<C, config::ConfigError>
    where
        C: DeserializeOwned,
    {
        self.config.get::<C>(&proc.get_proc_config_key())
    }

    /// Access a processor adaptor configuration from its processor name.
    pub fn get_adaptor_config(&self, proc: &(impl ProcBusParam + ?Sized)) -> Option<&Config> {
        self.adaptor_configs.get(&proc.get_proc_config_key())
    }

    /// Reload a processor's settings and adaptor configuration in one step.
    ///
    /// Returns an error if either cannot be reloaded:
    ///
    /// ```rust,ignore
    /// InternalMsg::Config(config) => {
    ///     match config.reload_proc::<MyProcSettings>(self.proc.as_ref(), &adaptor) {
    ///         Ok(settings) => {
    ///             // ... apply the difference between `settings` and `self.settings`
    ///             self.settings = settings;
    ///         }
    ///         Err(err) => prosa::tracing::warn!(
    ///             "Failed to reload configuration for processor {}: {err}",
    ///             self.name()
    ///         ),
    ///     }
    /// }
    /// ```
    pub fn reload_proc<S>(
        &self,
        proc: &dyn ProcBusParam,
        adaptor: &dyn Adaptor,
    ) -> Result<S, config::ConfigError>
    where
        S: DeserializeOwned,
    {
        let settings = self.get_proc::<S>(proc)?;
        adaptor.reload_config(self.get_adaptor_config(proc))?;
        Ok(settings)
    }

    /// Return every configuration path watched to maintain this configuration.
    pub fn watch_paths(&self, config_path: &str) -> Vec<PathBuf> {
        let mut watch_paths = config_watch_paths(Path::new(config_path))
            .into_iter()
            .collect::<HashSet<_>>();

        watch_paths.extend(self.adaptor_config_watch_paths.iter().cloned());

        sorted_paths(watch_paths)
    }

    /// Check if one processor configuration differs from another loaded configuration.
    pub fn has_proc_changed(&self, new: &Self, proc_config_key: &str) -> bool {
        let proc_config_changed =
            if let (ValueKind::Table(current_table), ValueKind::Table(new_table)) =
                (&self.config.cache.kind, &new.config.cache.kind)
            {
                current_table.get(proc_config_key) != new_table.get(proc_config_key)
            } else {
                false
            };

        proc_config_changed
            || self
                .adaptor_configs
                .get(proc_config_key)
                .map(|config| &config.cache)
                != new
                    .adaptor_configs
                    .get(proc_config_key)
                    .map(|config| &config.cache)
    }
}

impl PartialEq for ProsaConfig {
    fn eq(&self, other: &Self) -> bool {
        self.config.cache == other.config.cache
            && self.adaptor_configs.len() == other.adaptor_configs.len()
            && self
                .adaptor_configs
                .iter()
                .all(|(proc_config_key, current_config)| {
                    other
                        .adaptor_configs
                        .get(proc_config_key)
                        .is_some_and(|new_config| current_config.cache == new_config.cache)
                })
    }
}

impl Eq for ProsaConfig {}

fn get_proc_adaptor_config_paths(config: &Config) -> HashMap<String, String> {
    let mut adaptor_config_paths = HashMap::new();

    if let ValueKind::Table(config_table) = &config.cache.kind {
        for (proc_config_key, proc_config) in config_table {
            if let ValueKind::Table(proc_config_table) = &proc_config.kind
                && let Some(adaptor_config_path) = proc_config_table
                    .get("adaptor_config_path")
                    .and_then(|value| {
                        if let ValueKind::String(path) = &value.kind {
                            Some(path)
                        } else {
                            None
                        }
                    })
            {
                adaptor_config_paths.insert(proc_config_key.clone(), adaptor_config_path.clone());
            }
        }
    }

    adaptor_config_paths
}

impl From<ProsaConfig> for Config {
    fn from(prosa_config: ProsaConfig) -> Self {
        prosa_config.config
    }
}

/// Watches a configuration path and exposes native file change events.
pub struct ConfigWatcher {
    watcher: RecommendedWatcher,
    events: mpsc::UnboundedReceiver<notify::Result<Event>>,
    watched_paths: HashSet<PathBuf>,
}

impl ConfigWatcher {
    /// Wait for the next configuration file system event.
    pub async fn changed(&mut self) -> Option<notify::Result<Event>> {
        self.events.recv().await
    }

    /// Replace the set of paths watched for configuration changes.
    pub fn set_paths(&mut self, paths: Vec<PathBuf>) -> notify::Result<()> {
        let paths = paths.into_iter().collect::<HashSet<_>>();

        for path in self.watched_paths.difference(&paths) {
            if let Err(err) = self.watcher.unwatch(path) {
                log::warn!(
                    "Can't stop watching configuration {}: {err}",
                    path.display()
                );
            }
        }

        for path in paths.difference(&self.watched_paths) {
            let recursive_mode = if path.is_dir() {
                RecursiveMode::Recursive
            } else {
                RecursiveMode::NonRecursive
            };
            self.watcher.watch(path, recursive_mode)?;
        }

        self.watched_paths = paths;

        Ok(())
    }
}

/// Create a watcher for multiple configuration files or directories.
pub fn watch_config_paths(paths: Vec<PathBuf>) -> notify::Result<ConfigWatcher> {
    let (tx, events) = mpsc::unbounded_channel();
    let mut watcher = notify::recommended_watcher(move |event| {
        let _ = tx.send(event);
    })?;

    let mut watched_paths = HashSet::new();
    for path in paths {
        let recursive_mode = if path.is_dir() {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        };
        watcher.watch(&path, recursive_mode)?;
        watched_paths.insert(path);
    }

    Ok(ConfigWatcher {
        watcher,
        events,
        watched_paths,
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
    let mut config_watcher = match watch_config_paths(current_config.watch_paths(&config_path)) {
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
            Ok(new_config) if current_config != new_config => {
                match new_config.try_deserialize::<S>() {
                    Ok(settings) => {
                        if apply_config(settings, new_config.clone()).await {
                            if let Err(err) =
                                config_watcher.set_paths(new_config.watch_paths(&config_path))
                            {
                                log::warn!("Can't update watched configuration paths: {err}");
                            }
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
    use std::time::{SystemTime, UNIX_EPOCH};

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
        let current = ProsaConfig::from_config(
            Config::builder()
                .set_override("proc_1.service_name", "PROC_TEST")?
                .set_override("proc_1.tick_secs", 4)?
                .set_override("proc_2.service_name", "PROC_TEST_2")?
                .set_override("proc_2.tick_secs", 4)?
                .build()?,
        )?;
        let new = ProsaConfig::from_config(
            Config::builder()
                .set_override("proc_1.service_name", "PROC_TEST_UPDATED")?
                .set_override("proc_1.tick_secs", 4)?
                .set_override("proc_2.service_name", "PROC_TEST_2")?
                .set_override("proc_2.tick_secs", 4)?
                .build()?,
        )?;

        assert!(current.has_proc_changed(&new, "proc_1"));
        assert!(!current.has_proc_changed(&new, "proc_2"));

        Ok(())
    }

    #[test]
    fn test_config_folder_is_loaded_recursively() -> Result<(), Box<dyn std::error::Error>> {
        let config_path = unique_test_dir("prosa-recursive-config");
        let nested_path = config_path.join("nested");
        fs::create_dir_all(&nested_path)?;
        fs::write(
            config_path.join("main.yml"),
            "proc_1:\n  service_name: PROC_TEST\n",
        )?;
        fs::write(
            nested_path.join("override.yml"),
            "proc_1:\n  tick_secs: 4\n",
        )?;

        let config = ProsaConfig::from_path(config_path.to_str().ok_or("invalid temp path")?)?;

        assert_eq!(
            "PROC_TEST",
            config.config().get_string("proc_1.service_name")?
        );
        assert_eq!(4, config.config().get_int("proc_1.tick_secs")?);

        fs::remove_dir_all(config_path)?;

        Ok(())
    }

    #[test]
    fn test_adaptor_config_change_is_proc_change() -> Result<(), Box<dyn std::error::Error>> {
        let config_path = unique_test_dir("prosa-adaptor-config");
        fs::create_dir_all(&config_path)?;
        let adaptor_config_path = config_path.join("adaptor.yml");
        fs::write(&adaptor_config_path, "sleep_ms: 100\n")?;
        fs::write(
            config_path.join("main.yml"),
            format!(
                "proc_1:\n  service_name: PROC_TEST\n  adaptor_config_path: {}\n",
                adaptor_config_path.display()
            ),
        )?;

        let current = ProsaConfig::from_path(config_path.to_str().ok_or("invalid temp path")?)?;
        assert_eq!(
            100,
            current
                .adaptor_configs
                .get("proc_1")
                .ok_or("missing adaptor config")?
                .get_int("sleep_ms")?
        );

        fs::write(&adaptor_config_path, "sleep_ms: 200\n")?;
        let new = ProsaConfig::from_path(config_path.to_str().ok_or("invalid temp path")?)?;

        assert_ne!(current, new);
        assert!(current.has_proc_changed(&new, "proc_1"));

        fs::remove_dir_all(config_path)?;

        Ok(())
    }

    #[test]
    fn test_reload_proc() -> Result<(), config::ConfigError> {
        struct TestProc(&'static str);
        impl ProcBusParam for TestProc {
            fn get_proc_id(&self) -> u32 {
                1
            }

            fn name(&self) -> &str {
                self.0
            }
        }

        struct TestAdaptor {
            fail: bool,
        }
        impl Adaptor for TestAdaptor {
            fn reload_config(&self, _config: Option<&Config>) -> Result<(), config::ConfigError> {
                if self.fail {
                    Err(config::ConfigError::Message("adaptor failure".into()))
                } else {
                    Ok(())
                }
            }

            fn terminate(&self) {}
        }

        #[derive(serde::Deserialize)]
        struct TestProcSettings {
            service_name: String,
        }

        let config = ProsaConfig::from_config(
            Config::builder()
                .set_override("proc_1.service_name", "PROC_TEST")?
                .build()?,
        )?;

        let settings = config
            .reload_proc::<TestProcSettings>(&TestProc("proc-1"), &TestAdaptor { fail: false })
            .expect("Processor settings should be reloaded");
        assert_eq!("PROC_TEST", settings.service_name);

        assert!(matches!(
            config
                .reload_proc::<TestProcSettings>(&TestProc("proc-1"), &TestAdaptor { fail: true }),
            Err(config::ConfigError::Message(message)) if message == "adaptor failure"
        ));

        // A processor without a configuration section returns the deserialization error
        assert!(matches!(
            config.reload_proc::<TestProcSettings>(
                &TestProc("proc-unknown"),
                &TestAdaptor { fail: false }
            ),
            Err(config::ConfigError::NotFound(_))
        ));

        // An invalid section returns the deserialization error
        let invalid_config = ProsaConfig::from_config(
            Config::builder()
                .set_override("proc_1.service_name", vec!["not", "a", "string"])?
                .build()?,
        )?;
        assert!(matches!(
            invalid_config
                .reload_proc::<TestProcSettings>(&TestProc("proc-1"), &TestAdaptor { fail: false }),
            Err(config::ConfigError::Type { .. })
        ));

        // A section that misses a mandatory setting returns `At` rather than the `NotFound` of an
        // absent section
        let incomplete_config = ProsaConfig::from_config(
            Config::builder()
                .set_override("proc_1.unrelated", "value")?
                .build()?,
        )?;
        assert!(matches!(
            incomplete_config
                .reload_proc::<TestProcSettings>(&TestProc("proc-1"), &TestAdaptor { fail: false }),
            Err(config::ConfigError::At { .. })
        ));

        Ok(())
    }

    fn unique_test_dir(prefix: &str) -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{}-{timestamp}", std::process::id()))
    }
}
