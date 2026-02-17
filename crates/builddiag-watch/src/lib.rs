//! Polling watch loop for builddiag.
//!
//! This crate provides a small, deterministic polling watcher that tracks
//! build-contract input files and re-runs a callback when they change.

use anyhow::{Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::time::{Duration, UNIX_EPOCH};

const WATCHED_FILE_NAMES: &[&str] = &["Cargo.toml", "rust-toolchain.toml", "checksums.txt"];
const IGNORED_DIR_NAMES: &[&str] = &[".git", "target", ".builddiag-cache"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileStamp {
    modified_ms: u128,
    len: u64,
}

type Snapshot = BTreeMap<Utf8PathBuf, FileStamp>;

/// Runtime options for the watch loop.
#[derive(Debug, Clone)]
pub struct WatchOptions {
    /// Repository root to scan.
    pub root: Utf8PathBuf,
    /// Poll interval for filesystem snapshots.
    pub poll_interval: Duration,
    /// Debounce window to coalesce rapid file changes.
    pub debounce: Duration,
    /// Clear the terminal before each run.
    pub clear_screen: bool,
    /// Emit terminal bell when exit status changes.
    pub notify_on_status_change: bool,
    /// Optional maximum number of runs (for tests and scripting).
    pub max_runs: Option<usize>,
    /// Additional files (absolute or repo-relative) to track.
    pub extra_files: BTreeSet<Utf8PathBuf>,
}

impl WatchOptions {
    /// Creates watch options with sensible defaults for the given root.
    pub fn for_root(root: &Utf8Path) -> Self {
        Self {
            root: root.to_path_buf(),
            poll_interval: Duration::from_millis(250),
            debounce: Duration::from_millis(300),
            clear_screen: true,
            notify_on_status_change: false,
            max_runs: None,
            extra_files: BTreeSet::new(),
        }
    }
}

/// Clears the terminal using ANSI escape codes.
pub fn clear_terminal() {
    print!("\x1B[2J\x1B[H");
}

/// Runs a polling watch loop and invokes `run_once` on startup and each change.
///
/// The function returns only when `max_runs` is reached. Without `max_runs`,
/// the loop runs until the process is interrupted.
pub fn run_watch_loop<F>(options: &WatchOptions, mut run_once: F) -> Result<i32>
where
    F: FnMut() -> Result<i32>,
{
    if options.poll_interval.is_zero() {
        anyhow::bail!("watch poll interval must be > 0");
    }
    if options.debounce.is_zero() {
        anyhow::bail!("watch debounce must be > 0");
    }

    let mut snapshot = take_snapshot(options)?;
    let mut runs = 0usize;
    let mut previous_exit: Option<i32> = None;

    loop {
        if options.clear_screen {
            clear_terminal();
        }

        let exit = run_once()?;
        runs += 1;

        if options.notify_on_status_change
            && let Some(prev) = previous_exit
            && prev != exit
        {
            eprint!("\x07");
        }
        previous_exit = Some(exit);

        if options.max_runs.is_some_and(|max| runs >= max) {
            return Ok(exit);
        }

        wait_for_change(options, &mut snapshot)?;
    }
}

fn wait_for_change(options: &WatchOptions, snapshot: &mut Snapshot) -> Result<()> {
    loop {
        std::thread::sleep(options.poll_interval);
        let current = take_snapshot(options)?;
        if current == *snapshot {
            continue;
        }

        let mut stable = current;
        let mut stable_elapsed = Duration::ZERO;
        loop {
            std::thread::sleep(options.poll_interval);
            let next = take_snapshot(options)?;
            if next != stable {
                stable = next;
                stable_elapsed = Duration::ZERO;
                continue;
            }

            stable_elapsed += options.poll_interval;
            if stable_elapsed >= options.debounce {
                *snapshot = stable;
                return Ok(());
            }
        }
    }
}

fn take_snapshot(options: &WatchOptions) -> Result<Snapshot> {
    let mut snapshot = BTreeMap::new();
    collect_watched_files(&options.root, &options.root, &mut snapshot)?;
    collect_extra_files(&options.root, &options.extra_files, &mut snapshot)?;
    Ok(snapshot)
}

fn collect_extra_files(
    root: &Utf8Path,
    extra_files: &BTreeSet<Utf8PathBuf>,
    snapshot: &mut Snapshot,
) -> Result<()> {
    for path in extra_files {
        let absolute = if path.is_absolute() {
            path.clone()
        } else {
            root.join(path)
        };
        if !absolute.exists() || !absolute.is_file() {
            continue;
        }
        if let Some(stamp) = stat_file(&absolute)? {
            snapshot.insert(absolute, stamp);
        }
    }
    Ok(())
}

fn collect_watched_files(root: &Utf8Path, dir: &Utf8Path, snapshot: &mut Snapshot) -> Result<()> {
    let entries = fs::read_dir(dir).with_context(|| format!("read directory: {dir}"))?;
    for entry in entries {
        let entry = entry.with_context(|| format!("walk directory: {dir}"))?;
        let path = entry.path();
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();

        let file_type = entry
            .file_type()
            .with_context(|| format!("stat directory entry: {}", path.display()))?;
        if file_type.is_dir() {
            if IGNORED_DIR_NAMES
                .iter()
                .any(|ignored| ignored == &name.as_ref())
            {
                continue;
            }
            if let Some(utf8) = Utf8PathBuf::from_path_buf(path).ok() {
                collect_watched_files(root, &utf8, snapshot)?;
            }
            continue;
        }

        if !file_type.is_file() {
            continue;
        }
        if !WATCHED_FILE_NAMES
            .iter()
            .any(|watched| watched == &name.as_ref())
        {
            continue;
        }
        if let Some(utf8_path) = Utf8PathBuf::from_path_buf(path).ok()
            && let Some(stamp) = stat_file(&utf8_path)?
        {
            let normalized = if utf8_path.starts_with(root) {
                root.join(utf8_path.strip_prefix(root).unwrap_or(&utf8_path))
            } else {
                utf8_path
            };
            snapshot.insert(normalized, stamp);
        }
    }
    Ok(())
}

fn stat_file(path: &Utf8Path) -> Result<Option<FileStamp>> {
    if !path.exists() {
        return Ok(None);
    }
    let meta = fs::metadata(path).with_context(|| format!("metadata for {path}"))?;
    if !meta.is_file() {
        return Ok(None);
    }
    let modified_ms = meta
        .modified()
        .ok()
        .and_then(|mtime| mtime.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis())
        .unwrap_or(0);

    Ok(Some(FileStamp {
        modified_ms,
        len: meta.len(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_workspace() -> (TempDir, Utf8PathBuf) {
        let temp = TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
        std::fs::create_dir_all(root.join("crates/a/src")).unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            r#"[workspace]
resolver = "2"
members = ["crates/a"]
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("crates/a/Cargo.toml"),
            r#"[package]
name = "a"
version = "0.1.0"
edition = "2021"
"#,
        )
        .unwrap();
        std::fs::write(root.join("crates/a/src/lib.rs"), "pub fn a() {}\n").unwrap();
        (temp, root)
    }

    #[test]
    fn take_snapshot_tracks_default_files() {
        let (_temp, root) = create_workspace();
        std::fs::write(
            root.join("rust-toolchain.toml"),
            "[toolchain]\nchannel = \"1.75.0\"\n",
        )
        .unwrap();

        let opts = WatchOptions::for_root(&root);
        let snapshot = take_snapshot(&opts).unwrap();

        assert!(
            snapshot.keys().any(|p| p.ends_with("Cargo.toml")),
            "expected Cargo.toml in snapshot"
        );
        assert!(
            snapshot.keys().any(|p| p.ends_with("rust-toolchain.toml")),
            "expected rust-toolchain.toml in snapshot"
        );
    }

    #[test]
    fn watch_loop_runs_again_after_change() {
        let (_temp, root) = create_workspace();
        let manifest = root.join("Cargo.toml");
        let mut opts = WatchOptions::for_root(&root);
        opts.poll_interval = Duration::from_millis(20);
        opts.debounce = Duration::from_millis(50);
        opts.clear_screen = false;
        opts.max_runs = Some(2);

        let manifest_for_thread = manifest.clone();
        let handle = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(140));
            std::fs::write(
                &manifest_for_thread,
                r#"[workspace]
resolver = "2"
members = ["crates/a"]
# changed
"#,
            )
            .unwrap();
        });

        let mut runs = 0usize;
        let exit = run_watch_loop(&opts, || {
            runs += 1;
            Ok(runs as i32)
        })
        .unwrap();

        handle.join().unwrap();
        assert_eq!(exit, 2);
        assert_eq!(runs, 2);
    }

    #[test]
    fn watch_loop_tracks_extra_files() {
        let (_temp, root) = create_workspace();
        let config_rel = Utf8PathBuf::from("builddiag.toml");
        let config_abs = root.join(&config_rel);
        std::fs::write(&config_abs, "[defaults]\nfail_on = \"error\"\n").unwrap();

        let mut opts = WatchOptions::for_root(&root);
        opts.poll_interval = Duration::from_millis(20);
        opts.debounce = Duration::from_millis(50);
        opts.clear_screen = false;
        opts.max_runs = Some(2);
        opts.extra_files.insert(config_rel);

        let config_for_thread = config_abs.clone();
        let handle = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(140));
            std::fs::write(&config_for_thread, "[defaults]\nfail_on = \"never\"\n").unwrap();
        });

        let mut runs = 0usize;
        let exit = run_watch_loop(&opts, || {
            runs += 1;
            Ok(runs as i32)
        })
        .unwrap();

        handle.join().unwrap();
        assert_eq!(exit, 2);
        assert_eq!(runs, 2);
    }
}
