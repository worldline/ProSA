//!
//! <svg width="40" height="40">
#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/doc_assets/settings.svg"))]
//! </svg>

use std::{
    collections::HashMap,
    ffi::OsStr,
    fs,
    future::Future,
    io::{self, Write},
    path::{Path, PathBuf},
};

use config::{Config, ConfigBuilder, File, ValueKind, builder::DefaultState};
use glob::glob;
use notify::Event;
use prosa_utils::{config::observability::Observability, file::FileWatch};
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
                yaml_serde::to_string(&self)
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

/// Loaded ProSA configuration.
#[derive(Clone, Debug)]
pub struct ProsaConfig {
    config: Config,
    adaptor_configs: HashMap<String, Config>,
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
        for (proc_config_key, config_path) in get_proc_adaptor_config_paths(&config) {
            adaptor_configs.insert(proc_config_key, Self::load_adaptor_config(&config_path)?);
        }

        Ok(Self {
            config,
            adaptor_configs,
        })
    }

    /// Load an adaptor config path or glob pattern.
    pub(crate) fn load_adaptor_config(config_path: &str) -> Result<Config, config::ConfigError> {
        let mut builder = Config::builder();
        for path in glob(config_path)
            .map_err(|e| {
                config::ConfigError::Message(format!(
                    "Wrong config path pattern `{config_path}`: `{e}`"
                ))
            })?
            .filter_map(Result::ok)
        {
            builder = add_config_path(builder, &path)
                .map_err(|e| config::ConfigError::Foreign(Box::new(e)))?;
        }

        builder.build()
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

    /// Return every configuration path watched to maintain this configuration: the ProSA
    /// configuration path and every adaptor configuration path or glob pattern, sorted.
    ///
    /// Hand them to [`ConfigWatcher::set_paths`], which watches what they name.
    pub fn watch_paths(&self, config_path: &str) -> Vec<PathBuf> {
        let mut paths = get_proc_adaptor_config_paths(&self.config)
            .into_values()
            .map(PathBuf::from)
            .collect::<Vec<_>>();
        paths.push(PathBuf::from(config_path));
        paths.sort();
        paths.dedup();
        paths
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
///
/// A path can be a file, a directory, or a glob pattern, watched as [`FileWatch`] describes: a
/// file saved by renaming a new one over it, a path appearing, and a symlink swapped along the way
/// are all changes, while the other files of its directory aren't.
pub struct ConfigWatcher {
    watch: Option<FileWatch>,
    tx: mpsc::UnboundedSender<notify::Result<Event>>,
    events: mpsc::UnboundedReceiver<notify::Result<Event>>,
}

impl ConfigWatcher {
    /// Wait for the next configuration file system event.
    pub async fn changed(&mut self) -> Option<notify::Result<Event>> {
        self.events.recv().await
    }

    /// Replace the set of paths watched for configuration changes.
    pub fn set_paths(&mut self, paths: Vec<PathBuf>) -> notify::Result<()> {
        let tx = self.tx.clone();
        let watch = FileWatch::new(paths, move |event| {
            let _ = tx.send(Ok(event.clone()));
        })?;

        // Replaced once the new one watches, so no change is missed in between
        self.watch = Some(watch);
        Ok(())
    }
}

/// Create a watcher for multiple configuration files, directories or glob patterns.
pub fn watch_config_paths(paths: Vec<PathBuf>) -> notify::Result<ConfigWatcher> {
    let (tx, events) = mpsc::unbounded_channel();
    let mut config_watcher = ConfigWatcher {
        watch: None,
        tx,
        events,
    };
    config_watcher.set_paths(paths)?;
    Ok(config_watcher)
}

/// Filter out file system events that cannot affect configuration content.
pub fn is_config_reload_event(event: &Event) -> bool {
    prosa_utils::file::is_change(event)
}

/// Watch and reload a configuration path on file system changes.
///
/// A configuration is reloaded when one of the files it's read from changes, and applied when it
/// differs from the current one once loaded. A configuration that can't be loaded or deserialized
/// is reported and left aside, the current one stays until the files change again.
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

    // Read once watched, for a change made since the current configuration was loaded
    let mut changed = true;
    loop {
        if !changed {
            match config_watcher.changed().await {
                Some(Ok(event)) if is_config_reload_event(&event) => {}
                Some(Ok(_)) => continue,
                Some(Err(err)) => {
                    log::warn!("Error watching configuration {config_path}: {err}");
                    continue;
                }
                None => break,
            }

            // The events queued with it are read along, a burst of writes is loaded once
            while config_watcher.events.try_recv().is_ok() {}
        }
        changed = false;

        let new_config = match load_config(&config_path) {
            Ok(new_config) if new_config != current_config => new_config,
            Ok(_) => continue,
            Err(err) => {
                log::warn!("Can't reload configuration {config_path}: {err}");
                continue;
            }
        };

        match new_config.try_deserialize::<S>() {
            Ok(settings) => {
                if apply_config(settings, new_config.clone()).await {
                    // Its adaptor configurations may be read from other paths now
                    let watch_paths = new_config.watch_paths(&config_path);
                    if watch_paths != current_config.watch_paths(&config_path) {
                        if let Err(err) = config_watcher.set_paths(watch_paths) {
                            log::warn!("Can't update watched configuration paths: {err}");
                        }

                        // A change made before the new paths were watched
                        changed = true;
                    }
                    current_config = new_config;
                }
            }
            Err(err) => log::error!("Configuration changed but can't be deserialized: {err}"),
        }
    }

    log::warn!("Configuration watcher stopped for {config_path}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use prosa_macros::settings;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

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

    /// Reload settings driven by [`spawn_reload`]
    #[derive(Debug, serde::Deserialize)]
    struct WatchedSettings {
        proc_1: WatchedProc,
    }

    #[derive(Debug, serde::Deserialize)]
    struct WatchedProc {
        tick_secs: u64,
    }

    /// Reload driven by [`spawn_reload`]: what it applied, and how many times it read the
    /// configuration
    struct Reload {
        task: tokio::task::JoinHandle<()>,
        applied: tokio::sync::mpsc::UnboundedReceiver<(u64, Option<i64>)>,
        loads: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    }

    impl Reload {
        /// Next applied `tick_secs` and adaptor `sleep_ms`, [`None`] if nothing is applied
        async fn next(&mut self) -> Option<(u64, Option<i64>)> {
            tokio::time::timeout(Duration::from_secs(5), self.applied.recv())
                .await
                .ok()
                .flatten()
        }

        fn loads(&self) -> usize {
            self.loads.load(std::sync::atomic::Ordering::Relaxed)
        }
    }

    impl Drop for Reload {
        fn drop(&mut self) {
            self.task.abort();
        }
    }

    /// Drive `watch_config_reload` over `config_path`
    async fn spawn_reload(config_path: &Path) -> Result<Reload, Box<dyn std::error::Error>> {
        let watched = config_path.to_string_lossy().into_owned();
        let config = ProsaConfig::from_path(&watched)?;
        let (tx, applied) = tokio::sync::mpsc::unbounded_channel();
        let loads = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let load_counter = loads.clone();

        let task = tokio::spawn(async move {
            watch_config_reload::<WatchedSettings, _, _, _>(
                watched,
                config,
                move |path: &str| {
                    load_counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    ProsaConfig::from_path(path)
                },
                move |settings: WatchedSettings, config: ProsaConfig| {
                    let tx = tx.clone();
                    async move {
                        let sleep_ms = config
                            .adaptor_configs
                            .get("proc_1")
                            .and_then(|adaptor| adaptor.get_int("sleep_ms").ok());
                        let _ = tx.send((settings.proc_1.tick_secs, sleep_ms));
                        true
                    }
                },
            )
            .await;
        });

        // The configuration is watched from within the task, so nothing may change on disk
        // before it ran. It's read once watched
        tokio::time::sleep(Duration::from_millis(300)).await;
        loads.store(0, std::sync::atomic::Ordering::Relaxed);

        Ok(Reload {
            task,
            applied,
            loads,
        })
    }

    /// Save a file the way editors and configuration tools do, renaming a new file over it
    fn save(path: &Path, content: &str) -> io::Result<()> {
        let staging = path.with_extension("staging");
        fs::write(&staging, content)?;
        fs::rename(&staging, path)
    }

    /// Replace a whole configuration directory by renaming a new one over it
    fn publish_dir(dir: &Path, round: u32, content: &str) -> io::Result<()> {
        let staging = dir.with_extension(format!("new-{round}"));
        fs::create_dir_all(&staging)?;
        fs::write(staging.join("main.yml"), content)?;

        let replaced = dir.with_extension(format!("old-{round}"));
        fs::rename(dir, &replaced)?;
        fs::rename(&staging, dir)?;
        fs::remove_dir_all(replaced)
    }

    #[tokio::test]
    async fn test_reload_saved_config_file() -> Result<(), Box<dyn std::error::Error>> {
        let root = unique_test_dir("prosa-saved-file");
        fs::create_dir_all(&root)?;
        let config_path = root.join("main.yml");
        fs::write(&config_path, "proc_1:\n  tick_secs: 1\n")?;

        let mut reload = spawn_reload(&config_path).await?;
        for tick_secs in 2..=4 {
            save(
                &config_path,
                &format!("proc_1:\n  tick_secs: {tick_secs}\n"),
            )?;
            assert_eq!(Some((tick_secs, None)), reload.next().await);
        }

        // Written in place, the way `>` does it
        fs::write(&config_path, "proc_1:\n  tick_secs: 5\n")?;
        assert_eq!(Some((5, None)), reload.next().await);

        fs::remove_dir_all(root)?;
        Ok(())
    }

    /// A configuration directory replaced as a whole leaves its watch on the replaced directory
    #[tokio::test]
    async fn test_reload_replaced_config_dir() -> Result<(), Box<dyn std::error::Error>> {
        let root = unique_test_dir("prosa-replaced-dir");
        let config_path = root.join("conf");
        fs::create_dir_all(&config_path)?;
        fs::write(config_path.join("main.yml"), "proc_1:\n  tick_secs: 1\n")?;

        let mut reload = spawn_reload(&config_path).await?;
        for tick_secs in 2..=3 {
            publish_dir(
                &config_path,
                tick_secs as u32,
                &format!("proc_1:\n  tick_secs: {tick_secs}\n"),
            )?;
            assert_eq!(Some((tick_secs, None)), reload.next().await);
        }

        fs::remove_dir_all(root)?;
        Ok(())
    }

    /// A configuration that isn't applied still has to leave the configuration watched: it was
    /// published like any other, replacing what was watched
    #[tokio::test]
    async fn test_reload_after_rejected_config() -> Result<(), Box<dyn std::error::Error>> {
        let root = unique_test_dir("prosa-rejected");
        let config_path = root.join("conf");
        fs::create_dir_all(&config_path)?;
        fs::write(config_path.join("main.yml"), "proc_1:\n  tick_secs: 1\n")?;

        let mut reload = spawn_reload(&config_path).await?;

        // Doesn't change anything, doesn't deserialize, doesn't load. Each one is left to be read
        // on its own rather than along with the next
        for (round, content) in [
            "proc_1:\n  tick_secs: 1\n",
            "nothing_it_knows: true\n",
            "proc_1: [unclosed\n",
        ]
        .into_iter()
        .enumerate()
        {
            publish_dir(&config_path, round as u32, content)?;
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
        assert!(reload.applied.try_recv().is_err());

        publish_dir(&config_path, 4, "proc_1:\n  tick_secs: 7\n")?;
        assert_eq!(Some((7, None)), reload.next().await);

        fs::remove_dir_all(root)?;
        Ok(())
    }

    /// A file written beside the configuration is no reason to read it again: a ProSA logging
    /// next to its configuration would otherwise answer its own writes
    #[tokio::test]
    async fn test_neighbour_file_is_not_a_config_change() -> Result<(), Box<dyn std::error::Error>>
    {
        let root = unique_test_dir("prosa-neighbour");
        fs::create_dir_all(&root)?;
        let config_path = root.join("main.yml");
        fs::write(&config_path, "proc_1:\n  tick_secs: 1\n")?;

        let mut reload = spawn_reload(&config_path).await?;
        for line in 1..=20 {
            fs::write(root.join("prosa.log"), format!("line {line}\n"))?;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert_eq!(0, reload.loads());

        save(&config_path, "proc_1:\n  tick_secs: 5\n")?;
        assert_eq!(Some((5, None)), reload.next().await);

        fs::remove_dir_all(root)?;
        Ok(())
    }

    /// An adaptor configuration pattern is watched for the files it will match, not only the ones
    /// it matched when loaded
    #[tokio::test]
    async fn test_reload_new_adaptor_config_match() -> Result<(), Box<dyn std::error::Error>> {
        let root = unique_test_dir("prosa-adaptor-glob");
        let adaptor_dir = root.join("adaptor");
        fs::create_dir_all(&adaptor_dir)?;
        fs::write(adaptor_dir.join("a.yml"), "other: 1\n")?;
        let config_path = root.join("main.yml");
        fs::write(
            &config_path,
            format!(
                "proc_1:\n  tick_secs: 1\n  adaptor_config_path: \"{}/*.yml\"\n",
                adaptor_dir.display()
            ),
        )?;

        let mut reload = spawn_reload(&config_path).await?;
        fs::write(adaptor_dir.join("b.yml"), "sleep_ms: 200\n")?;
        assert_eq!(Some((1, Some(200))), reload.next().await);

        fs::remove_dir_all(root)?;
        Ok(())
    }

    /// An adaptor configuration path that changes is watched where it points now
    #[tokio::test]
    async fn test_reload_moved_adaptor_config() -> Result<(), Box<dyn std::error::Error>> {
        let root = unique_test_dir("prosa-adaptor-moved");
        fs::create_dir_all(&root)?;
        fs::write(root.join("first.yml"), "sleep_ms: 100\n")?;
        fs::write(root.join("second.yml"), "sleep_ms: 200\n")?;
        let config_path = root.join("main.yml");
        let config = |adaptor: &str| {
            format!(
                "proc_1:\n  tick_secs: 1\n  adaptor_config_path: \"{}\"\n",
                root.join(adaptor).display()
            )
        };
        fs::write(&config_path, config("first.yml"))?;

        let mut reload = spawn_reload(&config_path).await?;
        save(&config_path, &config("second.yml"))?;
        assert_eq!(Some((1, Some(200))), reload.next().await);

        save(&root.join("second.yml"), "sleep_ms: 300\n")?;
        assert_eq!(Some((1, Some(300))), reload.next().await);

        fs::remove_dir_all(root)?;
        Ok(())
    }

    /// A configuration file reached through symlinks swapped over, as a Kubernetes ConfigMap
    /// is: nothing writes to the file named, a link along the way is replaced
    #[cfg(target_family = "unix")]
    #[tokio::test]
    async fn test_reload_swapped_symlink() -> Result<(), Box<dyn std::error::Error>> {
        use std::os::unix::fs::symlink;

        let root = unique_test_dir("prosa-symlink");
        fs::create_dir_all(&root)?;
        let publish = |revision: u64| -> io::Result<()> {
            let data = root.join(format!("..{revision}"));
            fs::create_dir_all(&data)?;
            fs::write(
                data.join("main.yml"),
                format!("proc_1:\n  tick_secs: {revision}\n"),
            )?;

            // The previous revision is kept, so no event comes from its deletion
            let staged = root.join("..data_tmp");
            symlink(format!("..{revision}"), &staged)?;
            fs::rename(staged, root.join("..data"))
        };
        publish(1)?;
        symlink("..data/main.yml", root.join("main.yml"))?;

        let mut reload = spawn_reload(&root.join("main.yml")).await?;
        for revision in 2..=3 {
            publish(revision)?;
            assert_eq!(Some((revision, None)), reload.next().await);
        }

        fs::remove_dir_all(root)?;
        Ok(())
    }

    /// A configuration file whose parent directory is swapped through a symlink
    #[cfg(target_family = "unix")]
    #[tokio::test]
    async fn test_reload_swapped_parent_symlink() -> Result<(), Box<dyn std::error::Error>> {
        use std::os::unix::fs::symlink;

        let root = unique_test_dir("prosa-parent-symlink");
        fs::create_dir_all(&root)?;
        let first = root.join("first");
        let second = root.join("second");
        fs::create_dir(&first)?;
        fs::create_dir(&second)?;
        fs::write(first.join("main.yml"), "proc_1:\n  tick_secs: 1\n")?;
        fs::write(second.join("main.yml"), "proc_1:\n  tick_secs: 2\n")?;
        symlink(&first, root.join("live"))?;
        let mut reload = spawn_reload(&root.join("live/main.yml")).await?;
        symlink(&second, root.join("staged"))?;
        fs::rename(root.join("staged"), root.join("live"))?;
        assert_eq!(Some((2, None)), reload.next().await);
        fs::remove_dir_all(root)?;
        Ok(())
    }

    /// A change made between loading the configuration and watching it
    #[tokio::test]
    async fn test_reload_catches_a_change_before_watching() -> Result<(), Box<dyn std::error::Error>>
    {
        let root = unique_test_dir("prosa-startup-change");
        fs::create_dir_all(&root)?;
        let path = root.join("main.yml");
        fs::write(&path, "proc_1:\n  tick_secs: 1\n")?;
        let config = ProsaConfig::from_path(&path.to_string_lossy())?;
        fs::write(&path, "proc_1:\n  tick_secs: 2\n")?;
        let (sent, mut received) = mpsc::unbounded_channel();
        let task = tokio::spawn(watch_config_reload::<WatchedSettings, _, _, _>(
            path.to_string_lossy().into_owned(),
            config,
            ProsaConfig::from_path,
            move |settings, _| {
                let sent = sent.clone();
                async move {
                    let _ = sent.send(settings.proc_1.tick_secs);
                    true
                }
            },
        ));
        let received = tokio::time::timeout(Duration::from_secs(5), received.recv()).await;
        task.abort();
        fs::remove_dir_all(root)?;
        assert_eq!(Some(2), received?);
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
