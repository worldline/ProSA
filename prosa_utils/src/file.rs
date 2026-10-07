//! Module to be told when files change, through a single watcher shared by the whole process
//!
//! ```
//! use std::sync::{Arc, atomic::{AtomicU64, Ordering}};
//! use prosa_utils::file::FileWatch;
//!
//! # fn main() -> Result<(), notify::Error> {
//! let changes = Arc::new(AtomicU64::new(0));
//! let counter = changes.clone();
//! let watch = FileWatch::new(vec!["Cargo.toml".into()], move |_event| {
//!     counter.fetch_add(1, Ordering::Relaxed);
//! })?;
//!
//! // The callback is called on every change of `Cargo.toml` until `watch` is dropped
//! drop(watch);
//! # Ok(())
//! # }
//! ```

use std::{
    collections::BTreeMap,
    fmt, fs,
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard, PoisonError, mpsc},
};

use notify::{
    Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher as _,
    event::{AccessKind, AccessMode},
};

/// Callback of a [`FileWatch`]
type OnChange = Arc<dyn Fn(&Event) + Send + Sync>;

/// Watch on files, directories or glob patterns, calling a callback when they change.
///
/// Every watch is served by a single notify watcher shared by the whole process, created on first
/// use, so a watch costs no inotify instance nor thread of its own. The paths are watched through
/// their directory, so a file saved by renaming a new one over it, a file appearing where none
/// was, and a symlink swapped along the way (as Kubernetes does for its mounted ConfigMaps and
/// Secrets) are all changes, while the other files of that directory aren't.
///
/// The callback runs on the watcher thread: keep it short. It may be called a few times for a
/// single save, and once more while the watch is being dropped.
pub struct FileWatch(u64);

impl FileWatch {
    /// Method to watch `paths`, each one a file, a directory or a glob pattern, and call
    /// `on_change` whenever one of them changes.
    ///
    /// A directory is watched with everything under it. A path that doesn't exist yet is watched
    /// through the directory it will appear in. A path that can't be watched is logged rather than
    /// returned, and watched again on the next change around it. The only error is the watcher of
    /// the process failing to start.
    pub fn new(
        paths: Vec<PathBuf>,
        on_change: impl Fn(&Event) + Send + Sync + 'static,
    ) -> notify::Result<FileWatch> {
        let mut registry = lock_registry();
        if registry.is_none() {
            *registry = Some(Registry::new()?);
        }
        let Some(registry) = registry.as_mut() else {
            unreachable!("The registry was just created");
        };

        let id = registry.next_id;
        registry.next_id += 1;
        let sources = paths
            .iter()
            .map(|path| std::path::absolute(path).unwrap_or_else(|_| path.clone()))
            .collect::<Vec<_>>();
        let targets = Targets::resolve(&sources);
        registry.entries.insert(
            id,
            Entry {
                sources,
                targets,
                on_change: Arc::new(on_change),
            },
        );
        registry.settle(&[id], &[]);

        Ok(FileWatch(id))
    }
}

impl Drop for FileWatch {
    fn drop(&mut self) {
        let entry = lock_registry().as_mut().and_then(|registry| {
            let entry = registry.entries.remove(&self.0);
            registry.sync(&[]);
            entry
        });

        // Outside the lock, its callback may own what drops another watch
        drop(entry);
    }
}

impl fmt::Debug for FileWatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sources = lock_registry()
            .as_ref()
            .and_then(|registry| registry.entries.get(&self.0))
            .map(|entry| entry.sources.clone())
            .unwrap_or_default();
        f.debug_tuple("FileWatch").field(&sources).finish()
    }
}

/// Method to know if a file system event can change what a file holds: an access can't, except
/// closing a file that was written, which tells the writer is done.
pub fn is_change(event: &Event) -> bool {
    match event.kind {
        EventKind::Access(AccessKind::Close(AccessMode::Write)) => true,
        EventKind::Access(_) => false,
        _ => true,
    }
}

/// Watcher of the process, created by the first [`FileWatch`]
static REGISTRY: Mutex<Option<Registry>> = Mutex::new(None);

fn lock_registry() -> MutexGuard<'static, Option<Registry>> {
    REGISTRY.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What a [`FileWatch`] watches
struct Entry {
    /// Absolute paths or patterns it was created with
    sources: Vec<PathBuf>,
    /// Where they lead now
    targets: Targets,
    on_change: OnChange,
}

struct Registry {
    watcher: RecommendedWatcher,
    /// Paths watched by the notify watcher
    watched: BTreeMap<PathBuf, RecursiveMode>,
    entries: BTreeMap<u64, Entry>,
    next_id: u64,
}

impl Registry {
    fn new() -> notify::Result<Registry> {
        // The notify event loop can't be asked to watch from its own callback, so the events are
        // handled on a thread of their own
        let (tx, rx) = mpsc::channel();
        let watcher = notify::recommended_watcher(tx)?;
        std::thread::Builder::new()
            .name("prosa-file-watch".into())
            .spawn(move || {
                for event in rx {
                    dispatch(event);
                }
            })
            .map_err(notify::Error::io)?;

        Ok(Registry {
            watcher,
            watched: BTreeMap::new(),
            entries: BTreeMap::new(),
            next_id: 0,
        })
    }

    /// Watch what the entries need, then resolve the paths of entries `ids` again until they lead
    /// nowhere new: a directory created before its parent was watched sends no event, while it may
    /// already hold what they look for
    fn settle(&mut self, ids: &[u64], replaced: &[PathBuf]) {
        self.sync(replaced);
        for _ in 0..16 {
            let mut moved = false;
            for id in ids {
                if let Some(entry) = self.entries.get_mut(id) {
                    let targets = Targets::resolve(&entry.sources);
                    if targets.watches != entry.targets.watches {
                        entry.targets = targets;
                        moved = true;
                    }
                }
            }

            if !moved {
                break;
            }
            self.sync(&[]);
        }
    }

    /// Watch what the entries need, and watch again the paths under `replaced`: a watch follows
    /// the directory it was put on, not its path
    fn sync(&mut self, replaced: &[PathBuf]) {
        let mut desired = BTreeMap::new();
        for entry in self.entries.values() {
            for (path, mode) in &entry.targets.watches {
                let watched = desired.entry(path.clone()).or_insert(*mode);
                if *mode == RecursiveMode::Recursive {
                    *watched = RecursiveMode::Recursive;
                }
            }
        }

        // Covered by a recursive watch above. Watching it too would merge both in notify, and
        // removing one would drop the other
        let recursive = desired
            .iter()
            .filter(|(_, mode)| **mode == RecursiveMode::Recursive)
            .map(|(path, _)| path.clone())
            .collect::<Vec<_>>();
        desired.retain(|path, _| {
            !recursive
                .iter()
                .any(|dir| path != dir && path.starts_with(dir))
        });

        let mut removed_recursive = Vec::new();
        for (path, mode) in std::mem::take(&mut self.watched) {
            let is_replaced = replaced.iter().any(|replaced| path.starts_with(replaced));
            if !is_replaced && desired.get(&path) == Some(&mode) {
                self.watched.insert(path, mode);
            } else {
                // Fails when the directory is gone, which removed its watch already
                let _ = self.watcher.unwatch(&path);
                if mode == RecursiveMode::Recursive {
                    removed_recursive.push(path);
                }
            }
        }

        // Removing a recursive watch removes every watch under it in notify
        self.watched.retain(|path, _| {
            !removed_recursive
                .iter()
                .any(|removed| path.starts_with(removed))
        });

        for (path, mode) in desired {
            if !self.watched.contains_key(&path) {
                match self.watcher.watch(&path, mode) {
                    Ok(()) => {
                        self.watched.insert(path, mode);
                    }
                    // Removed since it was resolved, its parent reports the change
                    Err(err) if matches!(err.kind, notify::ErrorKind::PathNotFound) => {}
                    Err(err) => log::warn!("Can't watch {}: {err}", path.display()),
                }
            }
        }
    }
}

/// Handle a notify event: watch again what changed around the entries it concerns, then call them
fn dispatch(event: notify::Result<Event>) {
    let event = match event {
        Ok(event) => event,
        Err(err) => {
            log::warn!("Error watching files: {err}");
            return;
        }
    };
    if !is_change(&event) {
        return;
    }

    let callbacks = {
        let mut registry = lock_registry();
        let Some(registry) = registry.as_mut() else {
            return;
        };

        let mut ids = Vec::new();
        let mut callbacks = Vec::new();
        for (id, entry) in registry.entries.iter_mut() {
            if event.need_rescan() || event.paths.iter().any(|path| entry.targets.matches(path)) {
                entry.targets = Targets::resolve(&entry.sources);
                ids.push(*id);
                callbacks.push(entry.on_change.clone());
            }
        }

        // A watched directory that was moved, removed or replaced, watched again on its path
        let replaced = event
            .paths
            .iter()
            .filter(|path| {
                registry
                    .watched
                    .keys()
                    .any(|watched| watched.starts_with(path))
            })
            .cloned()
            .collect::<Vec<_>>();
        if !ids.is_empty() || !replaced.is_empty() {
            registry.settle(&ids, &replaced);
        }
        callbacks
    };

    // Outside the lock, so a callback can create or drop a watch
    for callback in callbacks {
        callback(&event);
    }
}

/// Change that concerns a watched path
#[derive(Debug)]
enum Trigger {
    /// This path, or one of its ancestors
    Path(PathBuf),
    /// Anything under this directory
    Under(PathBuf),
    /// A path matching this pattern, or under one that does
    Glob(glob::Pattern),
}

/// Directories to watch for some paths, and the changes that concern them
#[derive(Debug, Default)]
struct Targets {
    watches: BTreeMap<PathBuf, RecursiveMode>,
    triggers: Vec<Trigger>,
}

impl Targets {
    fn resolve(sources: &[PathBuf]) -> Targets {
        let mut targets = Targets::default();
        for source in sources {
            match source.to_str() {
                Some(pattern) if pattern.contains(['*', '?', '[']) => targets.add_glob(pattern),
                _ => targets.add_path(source),
            }
        }

        targets
    }

    fn matches(&self, path: &Path) -> bool {
        let options = glob::MatchOptions {
            require_literal_separator: true,
            ..Default::default()
        };
        self.triggers.iter().any(|trigger| match trigger {
            Trigger::Path(target) => target.starts_with(path),
            Trigger::Under(dir) => path.starts_with(dir),
            Trigger::Glob(pattern) => path
                .ancestors()
                .any(|ancestor| pattern.matches_path_with(ancestor, options)),
        })
    }

    fn watch(&mut self, path: &Path, mode: RecursiveMode) {
        let watched = self.watches.entry(path.to_path_buf()).or_insert(mode);
        if mode == RecursiveMode::Recursive {
            *watched = mode;
        }
    }

    fn add_path(&mut self, path: &Path) {
        let Some(resolved) = self.follow(path) else {
            return;
        };

        if let Some(parent) = resolved.parent() {
            self.watch(parent, RecursiveMode::NonRecursive);
        }
        if resolved.is_dir() {
            self.watch(&resolved, RecursiveMode::Recursive);
            self.triggers.push(Trigger::Under(resolved.clone()));
        }
        self.triggers.push(Trigger::Path(resolved));
    }

    fn add_glob(&mut self, pattern: &str) {
        let components = Path::new(pattern).components().collect::<Vec<_>>();
        let split = components
            .iter()
            .position(|component| {
                component
                    .as_os_str()
                    .to_str()
                    .is_some_and(|part| part.contains(['*', '?', '[']))
            })
            .unwrap_or(components.len());
        let dir = components[..split].iter().collect::<PathBuf>();
        let rest = components[split..].iter().collect::<PathBuf>();
        let mode = if components.len() - split > 1 || pattern.contains("**") {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        };
        let Some(real_dir) = self.follow(&dir) else {
            return;
        };

        // Events name the directory that is watched, the one its symlinks lead to
        let real_pattern = real_dir
            .to_str()
            .map(|real_dir| Path::new(&glob::Pattern::escape(real_dir)).join(&rest))
            .and_then(|real_pattern| real_pattern.to_str().map(String::from));
        let Some((real_pattern, glob_pattern)) = real_pattern.and_then(|real_pattern| {
            glob::Pattern::new(&real_pattern)
                .ok()
                .map(|glob_pattern| (real_pattern, glob_pattern))
        }) else {
            log::warn!("Can't watch the pattern {pattern}");
            return;
        };

        if let Some(parent) = real_dir.parent() {
            self.watch(parent, RecursiveMode::NonRecursive);
        }
        self.watch(&real_dir, mode);
        self.triggers.push(Trigger::Path(real_dir));
        for matched in glob::glob(&real_pattern).into_iter().flatten().flatten() {
            if matched.is_dir() {
                self.watch(&matched, RecursiveMode::Recursive);
            }
        }
        self.triggers.push(Trigger::Glob(glob_pattern));
    }

    /// Follow `path` component by component, watching every symlink met from the directory it is
    /// in, and return the file or directory it leads to. A missing path is watched from the
    /// directory it will appear in, one level at a time, and [`None`] is returned.
    fn follow(&mut self, path: &Path) -> Option<PathBuf> {
        let components = |path: &Path| {
            path.components()
                .rev()
                .map(|component| PathBuf::from(component.as_os_str()))
                .collect::<Vec<_>>()
        };

        let mut resolved = PathBuf::new();
        let mut pending = components(path);
        let mut links = 0;
        while let Some(component) = pending.pop() {
            match component.components().next() {
                Some(Component::Normal(_)) => {}
                Some(Component::ParentDir) => {
                    resolved.pop();
                    continue;
                }
                Some(Component::CurDir) | None => continue,
                Some(_) => {
                    resolved.push(component);
                    continue;
                }
            }

            let next = resolved.join(component);
            match fs::symlink_metadata(&next) {
                Ok(metadata) if metadata.is_symlink() && links < 40 => {
                    links += 1;
                    self.triggers.push(Trigger::Path(next.clone()));
                    self.watch(&resolved, RecursiveMode::NonRecursive);
                    if let Ok(target) = fs::read_link(&next) {
                        if target.is_absolute() {
                            resolved = PathBuf::new();
                        }
                        pending.extend(components(&target));
                    }
                }
                Ok(_) => resolved = next,
                Err(_) => {
                    let mut missing = next;
                    missing.extend(pending.iter().rev());
                    self.triggers.push(Trigger::Path(missing));
                    self.watch(&resolved, RecursiveMode::NonRecursive);
                    return None;
                }
            }
        }

        Some(resolved)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime};

    fn unique_test_dir(prefix: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).expect("The test directory should be created");
        dir
    }

    /// Watch `paths`, counting the changes
    fn watch(paths: &[&Path]) -> (FileWatch, mpsc::Receiver<()>) {
        let (tx, rx) = mpsc::channel();
        let watch = FileWatch::new(
            paths.iter().map(|path| path.to_path_buf()).collect(),
            move |_| {
                let _ = tx.send(());
            },
        )
        .expect("The watcher should start");
        (watch, rx)
    }

    /// Whether a change was reported, then forget the ones that came with it
    fn changed(rx: &mpsc::Receiver<()>) -> bool {
        let changed = rx.recv_timeout(Duration::from_secs(5)).is_ok();
        std::thread::sleep(Duration::from_millis(100));
        while rx.try_recv().is_ok() {}
        changed
    }

    fn unchanged(rx: &mpsc::Receiver<()>) -> bool {
        rx.recv_timeout(Duration::from_millis(300)).is_err()
    }

    /// Save a file the way editors and configuration tools do, renaming a new file over it
    fn save(path: &Path, content: &str) -> std::io::Result<()> {
        let staging = path.with_extension("staging");
        fs::write(&staging, content)?;
        fs::rename(&staging, path)
    }

    #[test]
    fn file_watch_follows_a_file() -> std::io::Result<()> {
        let dir = unique_test_dir("prosa-watch-file");
        let file = dir.join("cert.pem");
        fs::write(&file, "first")?;
        let (_watch, rx) = watch(&[&file]);

        fs::write(&file, "written in place")?;
        assert!(changed(&rx), "a write in place wasn't reported");

        for round in 0..3 {
            save(&file, &format!("saved {round}"))?;
            assert!(changed(&rx), "save {round} wasn't reported");
        }

        // The other files of its directory aren't watched
        for line in 0..10 {
            fs::write(dir.join("prosa.log"), format!("line {line}"))?;
        }
        assert!(unchanged(&rx));

        fs::remove_file(&file)?;
        assert!(changed(&rx), "the removal wasn't reported");
        fs::write(&file, "restored")?;
        assert!(changed(&rx), "the file appearing again wasn't reported");

        fs::remove_dir_all(dir)
    }

    #[test]
    fn file_watch_waits_for_a_missing_path() -> std::io::Result<()> {
        let dir = unique_test_dir("prosa-watch-missing");
        let file = dir.join("not/yet/there.yml");
        let (_watch, rx) = watch(&[&file]);

        fs::create_dir_all(file.parent().expect("The file has a parent"))?;
        assert!(changed(&rx));
        fs::write(&file, "here")?;
        assert!(changed(&rx), "the file appearing wasn't reported");
        fs::write(&file, "changed")?;
        assert!(changed(&rx), "the file changing wasn't reported");

        fs::remove_dir_all(dir)
    }

    #[test]
    fn file_watch_follows_a_directory() -> std::io::Result<()> {
        let root = unique_test_dir("prosa-watch-dir");
        let dir = root.join("store");
        fs::create_dir_all(dir.join("nested"))?;
        let (_watch, rx) = watch(&[&dir]);

        fs::write(dir.join("nested/ca.crt"), "nested")?;
        assert!(changed(&rx), "a nested addition wasn't reported");

        // Replaced as a whole by renaming another one over it
        let staging = root.join("staging");
        fs::create_dir_all(&staging)?;
        fs::write(staging.join("ca.crt"), "new store")?;
        fs::rename(&dir, root.join("old"))?;
        fs::rename(&staging, &dir)?;
        assert!(changed(&rx), "the replacement wasn't reported");
        fs::write(dir.join("ca.crt"), "written in the new one")?;
        assert!(changed(&rx), "a write in the new directory wasn't reported");

        fs::write(root.join("neighbour"), "not in it")?;
        assert!(unchanged(&rx));

        fs::remove_dir_all(root)
    }

    #[test]
    fn file_watch_follows_a_glob() -> std::io::Result<()> {
        let dir = unique_test_dir("prosa-watch-glob");
        fs::create_dir_all(dir.join("sub"))?;
        let pattern = dir.join("*.yml");
        let (_watch, rx) = watch(&[&pattern]);

        fs::write(dir.join("a.yml"), "new match")?;
        assert!(changed(&rx), "a new match wasn't reported");
        fs::write(dir.join("a.log"), "not a match")?;
        fs::write(dir.join("sub/b.yml"), "not at that level")?;
        assert!(unchanged(&rx));

        let nested_pattern = dir.join("*/*.yml");
        let (_nested, nested_rx) = watch(&[&nested_pattern]);
        fs::write(dir.join("sub/c.yml"), "nested match")?;
        assert!(changed(&nested_rx), "a nested match wasn't reported");

        fs::remove_dir_all(dir)
    }

    /// A file reached through symlinks swapped over, as a Kubernetes ConfigMap or Secret is:
    /// nothing writes to the file named, a link along the way is replaced
    #[cfg(target_family = "unix")]
    #[test]
    fn file_watch_follows_swapped_symlinks() -> std::io::Result<()> {
        use std::os::unix::fs::symlink;

        let dir = unique_test_dir("prosa-watch-symlink");
        let publish = |revision: u32| -> std::io::Result<()> {
            let data = dir.join(format!("..{revision}"));
            fs::create_dir_all(&data)?;
            fs::write(data.join("tls.crt"), format!("revision {revision}"))?;

            let staged = dir.join("..data_tmp");
            symlink(format!("..{revision}"), &staged)?;
            fs::rename(staged, dir.join("..data"))
        };
        publish(1)?;
        symlink("..data/tls.crt", dir.join("tls.crt"))?;

        let (_watch, rx) = watch(&[&dir.join("tls.crt")]);
        for revision in 2..=3 {
            publish(revision)?;
            assert!(changed(&rx), "revision {revision} wasn't reported");
        }

        fs::remove_dir_all(dir)
    }

    #[test]
    fn file_watch_stops_once_dropped() -> std::io::Result<()> {
        let dir = unique_test_dir("prosa-watch-drop");
        let file = dir.join("cert.pem");
        fs::write(&file, "first")?;
        let (dropped, rx) = watch(&[&file]);
        let (_other, other_rx) = watch(&[&file]);

        drop(dropped);
        fs::write(&file, "changed")?;

        // The watch shared with another one stays for it
        assert!(changed(&other_rx));
        assert!(unchanged(&rx));

        fs::remove_dir_all(dir)
    }
}
