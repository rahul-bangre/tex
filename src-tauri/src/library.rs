//! The paper library: settings, discovery of projects, creation and import.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct Settings {
    pub root: Option<String>,
    #[serde(default)]
    pub extra: Vec<String>,
    #[serde(default)]
    pub mains: HashMap<String, String>,
    #[serde(default)]
    pub opened: HashMap<String, u64>,
}

impl Settings {
    pub fn load(path: &Path) -> Settings {
        fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) {
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_vec_pretty(self) {
            let tmp = path.with_extension("json.tmp");
            if fs::write(&tmp, json).is_ok() {
                let _ = fs::rename(&tmp, path);
            }
        }
    }

    pub fn root_dir(&self) -> PathBuf {
        match &self.root {
            Some(r) => PathBuf::from(r),
            None => default_root(),
        }
    }
}

pub fn default_root() -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    home.join("Documents").join("Papers")
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInfo {
    pub name: String,
    pub path: String,
    pub main: Option<String>,
    pub title: Option<String>,
    pub modified: u64,
    pub opened: u64,
    pub pdf: Option<String>,
    pub pdf_mtime: u64,
    pub thumb: Option<String>,
    pub thumb_mtime: u64,
    pub external: bool,
    /// Folder inside the library ("" for the top level, "a/b" when nested).
    pub folder: String,
}

pub fn mtime_ms(p: &Path) -> u64 {
    fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Stable 64-bit FNV-1a hash, used to name per-project cache folders.
pub fn fnv(s: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}", h)
}

pub fn build_dir(cache: &Path, project: &str) -> PathBuf {
    cache.join("builds").join(fnv(project))
}

pub fn thumb_path(cache: &Path, project: &str) -> PathBuf {
    cache.join("thumbs").join(format!("{}.png", fnv(project)))
}

pub fn pdf_path(cache: &Path, project: &str, main: &str) -> PathBuf {
    let stem = Path::new(main)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "main".into());
    build_dir(cache, project).join(format!("{stem}.pdf"))
}

fn is_hidden(name: &str) -> bool {
    name.starts_with('.') || name == "node_modules" || name == "__MACOSX"
}

fn read_head(p: &Path, max: usize) -> String {
    let mut buf = Vec::with_capacity(max.min(64 * 1024));
    if let Ok(f) = fs::File::open(p) {
        let _ = f.take(max as u64).read_to_end(&mut buf);
    }
    String::from_utf8_lossy(&buf).into_owned()
}

fn has_documentclass(src: &str) -> bool {
    src.lines().any(|l| {
        let t = l.trim_start();
        !t.starts_with('%') && (t.contains("\\documentclass") || t.contains("\\documentstyle"))
    })
}

/// Collect .tex files up to a small depth, relative to `root`.
fn tex_files(root: &Path, rel: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(root.join(rel)) else { return };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let name = e.file_name().to_string_lossy().to_string();
        if is_hidden(&name) {
            continue;
        }
        let ft = match e.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        if ft.is_dir() && depth > 0 {
            tex_files(root, &rel.join(&name), depth - 1, out);
        } else if ft.is_file() && name.ends_with(".tex") {
            out.push(rel.join(&name));
        }
    }
}

pub fn detect_main(project: &Path, settings: &Settings) -> Option<String> {
    let key = project.to_string_lossy().to_string();
    if let Some(m) = settings.mains.get(&key) {
        if project.join(m).is_file() {
            return Some(m.clone());
        }
    }
    let mut files = Vec::new();
    tex_files(project, Path::new(""), 2, &mut files);
    if files.is_empty() {
        return None;
    }
    // Prefer shallow files, and conventional names.
    files.sort_by_key(|p| {
        let n = p.file_name().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
        let named = matches!(n.as_str(), "main.tex" | "paper.tex" | "thesis.tex" | "document.tex");
        (p.components().count(), !named)
    });
    for f in &files {
        if has_documentclass(&read_head(&project.join(f), 32 * 1024)) {
            return Some(f.to_string_lossy().to_string());
        }
    }
    Some(files[0].to_string_lossy().to_string())
}

/// Pull a readable title out of `\title{...}`.
pub fn extract_title(src: &str) -> Option<String> {
    let mut search = 0;
    while let Some(i) = src[search..].find("\\title") {
        let start = search + i;
        search = start + 6;
        // Skip if commented out.
        let line_start = src[..start].rfind('\n').map(|x| x + 1).unwrap_or(0);
        if src[line_start..start].contains('%') {
            continue;
        }
        let rest = &src[start + 6..];
        let rest = rest.trim_start();
        // Optional short title.
        let rest = if rest.starts_with('[') {
            match rest.find(']') {
                Some(j) => rest[j + 1..].trim_start(),
                None => continue,
            }
        } else {
            rest
        };
        if !rest.starts_with('{') {
            continue;
        }
        let mut depth = 0;
        let mut end = None;
        for (j, c) in rest.char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(j);
                        break;
                    }
                }
                _ => {}
            }
        }
        let inner = &rest[1..end?];
        let cleaned = clean_tex(inner);
        if !cleaned.is_empty() {
            return Some(cleaned);
        }
    }
    None
}

fn clean_tex(s: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    let mut skip_depth: Option<i32> = None;
    let mut depth = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' {
            let mut j = i + 1;
            while j < chars.len() && chars[j].is_ascii_alphabetic() {
                j += 1;
            }
            let name: String = chars[i + 1..j].iter().collect();
            if name.is_empty() {
                // Escaped char or line break (\\).
                if j < chars.len() {
                    let n = chars[j];
                    if n == '\\' {
                        // A forced line break: the title proper ends here.
                        if skip_depth.is_none() && !out.trim().is_empty() && depth <= 0 {
                            break;
                        }
                        out.push(' ');
                    } else if skip_depth.is_none() {
                        out.push(n);
                    }
                    j += 1;
                }
            } else if matches!(name.as_str(), "thanks" | "footnote" | "label" | "vspace" | "hspace") {
                if skip_depth.is_none() {
                    skip_depth = Some(depth);
                }
            } else if name == "LaTeX" {
                out.push_str("LaTeX");
            } else if name == "TeX" {
                out.push_str("TeX");
            }
            i = j;
            continue;
        }
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if skip_depth == Some(depth) {
                    skip_depth = None;
                }
            }
            '~' | '\n' => {
                if skip_depth.is_none() {
                    out.push(' ')
                }
            }
            '$' | '%' => {}
            _ => {
                if skip_depth.is_none() {
                    out.push(c)
                }
            }
        }
        i += 1;
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn latest_mtime(dir: &Path, depth: usize) -> u64 {
    let mut best = 0;
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if is_hidden(&name) {
                continue;
            }
            let p = e.path();
            match e.file_type() {
                Ok(t) if t.is_dir() && depth > 0 => best = best.max(latest_mtime(&p, depth - 1)),
                Ok(t) if t.is_file() => best = best.max(mtime_ms(&p)),
                _ => {}
            }
        }
    }
    best
}

pub fn project_info(path: &Path, settings: &Settings, cache: &Path, external: bool) -> ProjectInfo {
    let key = path.to_string_lossy().to_string();
    let main = detect_main(path, settings);
    let title = main
        .as_ref()
        .and_then(|m| extract_title(&read_head(&path.join(m), 64 * 1024)));
    let pdf = main.as_ref().map(|m| pdf_path(cache, &key, m)).filter(|p| p.is_file());
    let thumb = thumb_path(cache, &key);
    let thumb_ok = thumb.is_file();
    ProjectInfo {
        name: path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| key.clone()),
        path: key.clone(),
        main,
        title,
        modified: latest_mtime(path, 2),
        opened: *settings.opened.get(&key).unwrap_or(&0),
        pdf_mtime: pdf.as_ref().map(|p| mtime_ms(p)).unwrap_or(0),
        pdf: pdf.map(|p| p.to_string_lossy().to_string()),
        thumb_mtime: if thumb_ok { mtime_ms(&thumb) } else { 0 },
        thumb: thumb_ok.then(|| thumb.to_string_lossy().to_string()),
        external,
        folder: if external {
            String::new()
        } else {
            path.parent()
                .and_then(|p| p.strip_prefix(settings.root_dir()).ok())
                .map(|r| r.to_string_lossy().to_string())
                .unwrap_or_default()
        },
    }
}

/// A directory is a paper when it has a .tex file at its top level. Otherwise it is a
/// folder that groups papers (empty directories count as folders).
pub fn is_project(dir: &Path) -> bool {
    let Ok(rd) = fs::read_dir(dir) else { return false };
    let mut has_subdir = false;
    let mut has_files = false;
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if is_hidden(&name) {
            continue;
        }
        match e.file_type() {
            Ok(t) if t.is_file() => {
                if name.to_lowercase().ends_with(".tex") {
                    return true;
                }
                has_files = true;
            }
            Ok(t) if t.is_dir() => has_subdir = true,
            _ => {}
        }
    }
    has_files && !has_subdir
}

#[derive(Serialize)]
pub struct Library {
    pub projects: Vec<ProjectInfo>,
    /// Every folder, relative to the library root, parents before children.
    pub folders: Vec<String>,
}

fn walk_library(root: &Path, rel: &Path, depth: usize, settings: &Settings, cache: &Path, lib: &mut Library) {
    let Ok(rd) = fs::read_dir(root.join(rel)) else { return };
    let mut dirs: Vec<_> = rd
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter(|e| !is_hidden(&e.file_name().to_string_lossy()))
        .collect();
    dirs.sort_by_key(|e| e.file_name().to_string_lossy().to_lowercase());
    for e in dirs {
        let child = rel.join(e.file_name());
        if is_project(&e.path()) {
            lib.projects.push(project_info(&e.path(), settings, cache, false));
        } else if depth < 4 {
            lib.folders.push(child.to_string_lossy().to_string());
            walk_library(root, &child, depth + 1, settings, cache, lib);
        }
    }
}

pub fn list_library(settings: &Settings, cache: &Path) -> Library {
    let root = settings.root_dir();
    let _ = fs::create_dir_all(&root);
    let mut lib = Library { projects: Vec::new(), folders: Vec::new() };
    walk_library(&root, Path::new(""), 0, settings, cache, &mut lib);
    for x in &settings.extra {
        let p = PathBuf::from(x);
        if p.is_dir() && !p.starts_with(&root) {
            lib.projects.push(project_info(&p, settings, cache, true));
        }
    }
    lib
}

pub fn sanitize_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if matches!(c, '/' | '\\' | ':' | '\0') { '-' } else { c })
        .collect();
    let t = cleaned.trim().trim_start_matches('.').trim();
    if t.is_empty() {
        "Untitled".into()
    } else {
        t.chars().take(80).collect()
    }
}

pub fn unique_dir(root: &Path, name: &str) -> PathBuf {
    let mut p = root.join(name);
    let mut n = 2;
    while p.exists() {
        p = root.join(format!("{name} {n}"));
        n += 1;
    }
    p
}

pub const ARTICLE: &str = r"\documentclass[11pt]{article}
\usepackage[margin=1in]{geometry}
\usepackage{amsmath, amssymb, amsthm}
\usepackage{graphicx}
\usepackage[hidelinks]{hyperref}

\title{%TITLE%}
\author{}
\date{\today}

\begin{document}
\maketitle

\begin{abstract}
\end{abstract}

\section{Introduction}


\end{document}
";

pub const BLANK: &str = r"\documentclass{article}

\begin{document}


\end{document}
";

/// Extract an Overleaf-style zip into `dest`, stripping a single wrapping folder.
/// Returns the name of the wrapping folder, when the archive had one.
pub fn extract_zip(zip_path: &Path, dest: &Path) -> Result<Option<String>, String> {
    let f = fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(f).map_err(|_| "That file isn't a .zip archive tex can read.".to_string())?;

    // Find a common top-level folder shared by every entry.
    let mut prefix: Option<PathBuf> = None;
    let mut single_root = true;
    for i in 0..archive.len() {
        let entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let Some(name) = entry.enclosed_name() else { continue };
        if name.starts_with("__MACOSX") {
            continue;
        }
        let first = name.components().next().map(|c| PathBuf::from(c.as_os_str()));
        let is_top_file = name.components().count() == 1 && !entry.is_dir();
        if is_top_file {
            single_root = false;
            break;
        }
        match (&prefix, first) {
            (None, Some(f)) => prefix = Some(f),
            (Some(p), Some(f)) if *p != f => {
                single_root = false;
                break;
            }
            _ => {}
        }
    }
    let strip = if single_root { prefix } else { None };
    let wrapper = strip.as_ref().map(|p| p.to_string_lossy().to_string());

    fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let Some(name) = entry.enclosed_name() else { continue };
        if name.starts_with("__MACOSX") || name.file_name().map(|n| n == ".DS_Store").unwrap_or(false) {
            continue;
        }
        let rel = match &strip {
            Some(s) => match name.strip_prefix(s) {
                Ok(r) => r.to_path_buf(),
                Err(_) => continue,
            },
            None => name.clone(),
        };
        if rel.as_os_str().is_empty() {
            continue;
        }
        let out = dest.join(&rel);
        if entry.is_dir() {
            fs::create_dir_all(&out).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut w = fs::File::create(&out).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut w).map_err(|e| e.to_string())?;
        }
    }
    Ok(wrapper)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles() {
        assert_eq!(extract_title("\\title{A \\textbf{Bold} Idea}").as_deref(), Some("A Bold Idea"));
        assert_eq!(
            extract_title("% \\title{no}\n\\title[s]{Real\\thanks{x}\\\\ Title}").as_deref(),
            Some("Real")
        );
        assert_eq!(
            extract_title("\\title{\\textbf{Predict It}\\\\[4pt]\\large Subtitle}").as_deref(),
            Some("Predict It")
        );
        assert_eq!(extract_title("\\title{}"), None);
    }
}
