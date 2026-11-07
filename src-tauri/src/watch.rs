//! Watches the open project and tells the UI when files change on disk.

use notify_debouncer_mini::{new_debouncer, notify::RecursiveMode, DebounceEventResult, Debouncer};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Emitter};

pub type Handle = Debouncer<notify_debouncer_mini::notify::RecommendedWatcher>;

pub fn watch(app: AppHandle, root: PathBuf) -> Option<Handle> {
    let canon = std::fs::canonicalize(&root).unwrap_or_else(|_| root.clone());
    let base = root.clone();
    let mut debouncer = new_debouncer(Duration::from_millis(200), move |res: DebounceEventResult| {
        let Ok(events) = res else { return };
        let mut paths: Vec<String> = events
            .into_iter()
            .filter_map(|e| rel(&e.path, &base, &canon))
            .filter(|p| !p.split('/').any(|seg| seg.starts_with('.')))
            .collect();
        paths.sort();
        paths.dedup();
        if !paths.is_empty() {
            let _ = app.emit("fs-change", paths);
        }
    })
    .ok()?;
    debouncer.watcher().watch(&root, RecursiveMode::Recursive).ok()?;
    Some(debouncer)
}

fn rel(p: &Path, base: &Path, canon: &Path) -> Option<String> {
    p.strip_prefix(base)
        .or_else(|_| p.strip_prefix(canon))
        .ok()
        .map(|r| r.to_string_lossy().to_string())
        .filter(|s| !s.is_empty())
}

/// Watches the library root and tells the UI when papers or folders appear, move or change.
pub fn watch_library(app: AppHandle, root: PathBuf) -> Option<Handle> {
    let canon = std::fs::canonicalize(&root).unwrap_or_else(|_| root.clone());
    let base = root.clone();
    let mut debouncer = new_debouncer(Duration::from_millis(400), move |res: DebounceEventResult| {
        let Ok(events) = res else { return };
        let relevant = events.iter().any(|e| {
            let Some(r) = rel(&e.path, &base, &canon) else { return false };
            let parts: Vec<&str> = r.split('/').collect();
            !parts.iter().any(|seg| seg.starts_with('.'))
                && (parts.len() <= 4 || r.ends_with(".tex"))
        });
        if relevant {
            let _ = app.emit("library-change", ());
        }
    })
    .ok()?;
    debouncer.watcher().watch(&root, RecursiveMode::Recursive).ok()?;
    Some(debouncer)
}
