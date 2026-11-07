//! Project file tree and file operations, all scoped to a project folder.

use serde::Serialize;
use std::fs;
use std::path::{Component, Path, PathBuf};

#[derive(Serialize)]
pub struct Entry {
    pub path: String,
    pub dir: bool,
    pub size: u64,
}

const MAX_ENTRIES: usize = 4000;

pub fn list_tree(root: &Path) -> Vec<Entry> {
    let mut out = Vec::new();
    walk(root, Path::new(""), 0, &mut out);
    out
}

fn walk(root: &Path, rel: &Path, depth: usize, out: &mut Vec<Entry>) {
    if depth > 8 || out.len() >= MAX_ENTRIES {
        return;
    }
    let Ok(rd) = fs::read_dir(root.join(rel)) else { return };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || matches!(name.as_str(), "node_modules" | "__MACOSX" | "__pycache__") {
            continue;
        }
        let Ok(meta) = e.metadata() else { continue };
        // Python virtualenvs are never part of a paper.
        if meta.is_dir() && e.path().join("pyvenv.cfg").is_file() {
            continue;
        }
        let p = rel.join(&name);
        let dir = meta.is_dir();
        out.push(Entry { path: p.to_string_lossy().to_string(), dir, size: if dir { 0 } else { meta.len() } });
        if dir {
            walk(root, &p, depth + 1, out);
        }
        if out.len() >= MAX_ENTRIES {
            return;
        }
    }
}

/// Resolve `rel` inside `root`, refusing anything that escapes it.
pub fn resolve(root: &Path, rel: &str) -> Result<PathBuf, String> {
    let rel_path = Path::new(rel);
    if rel_path.is_absolute() {
        let canon_root = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
        if rel_path.starts_with(root) || rel_path.starts_with(&canon_root) {
            return Ok(rel_path.to_path_buf());
        }
        return Err("Path is outside the project".into());
    }
    let mut out = root.to_path_buf();
    for c in rel_path.components() {
        match c {
            Component::Normal(s) => out.push(s),
            Component::CurDir => {}
            _ => return Err("Invalid path".into()),
        }
    }
    Ok(out)
}
