mod compile;
mod files;
mod library;
mod synctex;
mod watch;

use library::{ProjectInfo, Settings};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::ipc::{InvokeBody, Request, Response};
use tauri::{AppHandle, Manager, State};

struct AppState {
    settings: Mutex<Settings>,
    config_path: PathBuf,
    cache_dir: PathBuf,
    synctex: Mutex<synctex::Cache>,
    watcher: Mutex<Option<watch::Handle>>,
    lib_watcher: Mutex<Option<watch::Handle>>,
}

impl AppState {
    fn settings(&self) -> Settings {
        self.settings.lock().unwrap().clone()
    }
    fn update(&self, f: impl FnOnce(&mut Settings)) {
        let mut s = self.settings.lock().unwrap();
        f(&mut s);
        s.save(&self.config_path);
    }
    fn info(&self, path: &Path) -> ProjectInfo {
        let s = self.settings();
        let external = !path.starts_with(s.root_dir());
        library::project_info(path, &s, &self.cache_dir, external)
    }
}

type Res<T> = Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppInfo {
    root: String,
    tectonic: Option<String>,
}

#[tauri::command]
fn app_info(state: State<AppState>) -> AppInfo {
    AppInfo {
        root: state.settings().root_dir().to_string_lossy().to_string(),
        tectonic: compile::find_tectonic().map(|p| p.to_string_lossy().to_string()),
    }
}

#[tauri::command]
fn set_root(app: AppHandle, state: State<AppState>, path: String) -> Res<()> {
    std::fs::create_dir_all(&path).map_err(err)?;
    state.update(|s| s.root = Some(path));
    watch_library(app, state);
    Ok(())
}

/// (Re)start watching the library folder so new papers show up without a manual refresh.
#[tauri::command]
fn watch_library(app: AppHandle, state: State<AppState>) {
    let root = state.settings().root_dir();
    let _ = std::fs::create_dir_all(&root);
    *state.lib_watcher.lock().unwrap() = watch::watch_library(app, root);
}

#[tauri::command]
async fn list_library(state: State<'_, AppState>) -> Res<library::Library> {
    let s = state.settings();
    let cache = state.cache_dir.clone();
    tauri::async_runtime::spawn_blocking(move || library::list_library(&s, &cache)).await.map_err(err)
}

/// Resolve a folder path relative to the library root, refusing escapes.
fn folder_dir(state: &AppState, folder: &str) -> Res<PathBuf> {
    let root = state.settings().root_dir();
    if folder.is_empty() {
        return Ok(root);
    }
    files::resolve(&root, folder)
}

#[tauri::command]
fn create_folder(state: State<AppState>, parent: String, name: String) -> Res<String> {
    let base = folder_dir(&state, &parent)?;
    let dir = library::unique_dir(&base, &library::sanitize_name(&name));
    std::fs::create_dir_all(&dir).map_err(err)?;
    let root = state.settings().root_dir();
    Ok(dir.strip_prefix(&root).map(|r| r.to_string_lossy().to_string()).unwrap_or_default())
}

#[tauri::command]
fn rename_folder(state: State<AppState>, folder: String, name: String) -> Res<String> {
    let from = folder_dir(&state, &folder)?;
    if folder.is_empty() {
        return Err("Can't rename the library itself".into());
    }
    let parent = from.parent().ok_or("Bad folder")?.to_path_buf();
    let to = library::unique_dir(&parent, &library::sanitize_name(&name));
    std::fs::rename(&from, &to).map_err(err)?;
    let old_prefix = from.to_string_lossy().to_string();
    let new_prefix = to.to_string_lossy().to_string();
    rekey(&state, |k| k.strip_prefix(&old_prefix).map(|rest| format!("{new_prefix}{rest}")));
    let root = state.settings().root_dir();
    Ok(to.strip_prefix(&root).map(|r| r.to_string_lossy().to_string()).unwrap_or_default())
}

#[tauri::command]
fn trash_folder(state: State<AppState>, folder: String) -> Res<()> {
    if folder.is_empty() {
        return Err("Can't remove the library itself".into());
    }
    let dir = folder_dir(&state, &folder)?;
    trash::delete(&dir).map_err(err)
}

#[tauri::command]
fn move_project(state: State<AppState>, path: String, folder: String) -> Res<ProjectInfo> {
    let from = PathBuf::from(&path);
    let root = state.settings().root_dir();
    if !from.starts_with(&root) {
        return Err("Linked papers live outside the library and can't be moved".into());
    }
    let base = folder_dir(&state, &folder)?;
    if base.starts_with(&from) {
        return Err("Can't move a paper into itself".into());
    }
    if from.parent() == Some(base.as_path()) {
        return Ok(state.info(&from));
    }
    let name = from.file_name().ok_or("Bad path")?.to_string_lossy().to_string();
    let to = library::unique_dir(&base, &name);
    std::fs::rename(&from, &to).map_err(err)?;
    // Carry the cached build and thumbnail along so the card doesn't go blank.
    let new = to.to_string_lossy().to_string();
    let cache = &state.cache_dir;
    let _ = std::fs::rename(library::build_dir(cache, &path), library::build_dir(cache, &new));
    let _ = std::fs::rename(library::thumb_path(cache, &path), library::thumb_path(cache, &new));
    rekey(&state, |k| (k == path).then(|| new.clone()));
    Ok(state.info(&to))
}

/// Rewrite per-paper settings keys after a move or rename.
fn rekey(state: &AppState, map: impl Fn(&str) -> Option<String>) {
    state.update(|s| {
        for table in [&mut s.mains, &mut s.opened] as [&mut dyn RekeyMap; 2] {
            table.rekey(&map);
        }
    });
}

trait RekeyMap {
    fn rekey(&mut self, f: &dyn Fn(&str) -> Option<String>);
}
impl<V> RekeyMap for std::collections::HashMap<String, V> {
    fn rekey(&mut self, f: &dyn Fn(&str) -> Option<String>) {
        let keys: Vec<String> = self.keys().cloned().collect();
        for k in keys {
            if let Some(n) = f(&k) {
                if let Some(v) = self.remove(&k) {
                    self.insert(n, v);
                }
            }
        }
    }
}

fn pbpaste() -> String {
    std::process::Command::new("pbpaste")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

#[tauri::command]
fn clipboard_text() -> String {
    pbpaste()
}

#[tauri::command]
fn create_project(state: State<AppState>, name: String, template: String, folder: Option<String>) -> Res<ProjectInfo> {
    let root = folder_dir(&state, folder.as_deref().unwrap_or(""))?;
    std::fs::create_dir_all(&root).map_err(err)?;
    let (body, guessed) = match template.as_str() {
        "clipboard" => {
            let text = pbpaste();
            if !text.contains("\\begin{document}") {
                return Err("The clipboard doesn't contain a LaTeX document (no \\begin{document}).".into());
            }
            let title = library::extract_title(&text);
            (text, title)
        }
        "blank" => (library::BLANK.to_string(), None),
        _ => {
            let t = if name.trim().is_empty() { "Untitled".to_string() } else { name.trim().to_string() };
            (library::ARTICLE.replace("%TITLE%", &t), None)
        }
    };
    let name = if name.trim().is_empty() { guessed.unwrap_or_else(|| "Untitled".into()) } else { name };
    let dir = library::unique_dir(&root, &library::sanitize_name(&name));
    std::fs::create_dir_all(&dir).map_err(err)?;
    std::fs::write(dir.join("main.tex"), body).map_err(err)?;
    Ok(state.info(&dir))
}

#[tauri::command]
async fn import_zip(state: State<'_, AppState>, path: String, folder: Option<String>) -> Res<ProjectInfo> {
    let root = folder_dir(&state, folder.as_deref().unwrap_or(""))?;
    std::fs::create_dir_all(&root).map_err(err)?;
    let zip = PathBuf::from(&path);
    let stem = zip.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Imported".into());
    // Extract to a staging folder first so the final name can come from the archive itself.
    let staging = library::unique_dir(&root, &format!(".import-{}", library::fnv(&path)));
    let st2 = staging.clone();
    let res = tauri::async_runtime::spawn_blocking(move || library::extract_zip(&zip, &st2)).await.map_err(err)?;
    let wrapper = match res {
        Ok(w) => w,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(e);
        }
    };
    let name = wrapper.filter(|w| !w.trim().is_empty()).unwrap_or(stem);
    let dir = library::unique_dir(&root, &library::sanitize_name(&name));
    std::fs::rename(&staging, &dir).map_err(err)?;
    Ok(state.info(&dir))
}

#[tauri::command]
fn add_folder(state: State<AppState>, path: String) -> Res<ProjectInfo> {
    let p = PathBuf::from(&path);
    if !p.is_dir() {
        return Err("Not a folder".into());
    }
    if !p.starts_with(state.settings().root_dir()) {
        state.update(|s| {
            if !s.extra.contains(&path) {
                s.extra.push(path.clone());
            }
        });
    }
    Ok(state.info(&p))
}

#[tauri::command]
fn forget_project(state: State<AppState>, path: String) {
    state.update(|s| {
        s.extra.retain(|x| *x != path);
        s.opened.remove(&path);
        s.mains.remove(&path);
    });
}

#[tauri::command]
fn trash_project(state: State<AppState>, path: String) -> Res<()> {
    let p = PathBuf::from(&path);
    if !p.starts_with(state.settings().root_dir()) {
        return Err("Only papers inside the library folder can be moved to the Trash".into());
    }
    trash::delete(&p).map_err(err)?;
    forget_project(state, path);
    Ok(())
}

#[derive(Serialize)]
struct Opened {
    info: ProjectInfo,
    tree: Vec<files::Entry>,
}

#[tauri::command]
fn open_project(app: AppHandle, state: State<AppState>, path: String) -> Res<Opened> {
    let p = PathBuf::from(&path);
    if !p.is_dir() {
        return Err("This paper's folder no longer exists".into());
    }
    state.update(|s| {
        s.opened.insert(path.clone(), now_ms());
    });
    *state.watcher.lock().unwrap() = watch::watch(app, p.clone());
    Ok(Opened { info: state.info(&p), tree: files::list_tree(&p) })
}

#[tauri::command]
fn close_project(state: State<AppState>) {
    *state.watcher.lock().unwrap() = None;
}

#[tauri::command]
fn list_tree(project: String) -> Vec<files::Entry> {
    files::list_tree(Path::new(&project))
}

#[tauri::command]
fn read_text(project: String, rel: String) -> Res<String> {
    let p = files::resolve(Path::new(&project), &rel)?;
    let bytes = std::fs::read(&p).map_err(err)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

#[tauri::command]
fn write_text(project: String, rel: String, content: String) -> Res<u64> {
    let p = files::resolve(Path::new(&project), &rel)?;
    std::fs::write(&p, content).map_err(err)?;
    Ok(library::mtime_ms(&p))
}

#[tauri::command]
fn read_bytes(project: String, rel: String) -> Res<Response> {
    let p = files::resolve(Path::new(&project), &rel)?;
    Ok(Response::new(std::fs::read(&p).map_err(err)?))
}

#[tauri::command]
fn create_entry(project: String, rel: String, dir: bool) -> Res<()> {
    let p = files::resolve(Path::new(&project), &rel)?;
    if p.exists() {
        return Err("Something with that name already exists".into());
    }
    if dir {
        std::fs::create_dir_all(&p).map_err(err)
    } else {
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).map_err(err)?;
        }
        std::fs::write(&p, "").map_err(err)
    }
}

#[tauri::command]
fn rename_entry(project: String, from: String, to: String) -> Res<()> {
    let root = Path::new(&project);
    let a = files::resolve(root, &from)?;
    let b = files::resolve(root, &to)?;
    if b.exists() {
        return Err("Something with that name already exists".into());
    }
    if let Some(parent) = b.parent() {
        std::fs::create_dir_all(parent).map_err(err)?;
    }
    std::fs::rename(a, b).map_err(err)
}

#[tauri::command]
fn trash_entry(project: String, rel: String) -> Res<()> {
    let p = files::resolve(Path::new(&project), &rel)?;
    trash::delete(&p).map_err(err)
}

#[tauri::command]
fn import_files(project: String, dir: String, paths: Vec<String>) -> Res<Vec<String>> {
    let root = Path::new(&project);
    let target = files::resolve(root, &dir)?;
    let mut added = Vec::new();
    for src in paths {
        let src = PathBuf::from(src);
        let Some(name) = src.file_name() else { continue };
        let mut dest = target.join(name);
        let mut n = 2;
        while dest.exists() {
            let stem = src.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            let ext = src.extension().map(|s| format!(".{}", s.to_string_lossy())).unwrap_or_default();
            dest = target.join(format!("{stem}-{n}{ext}"));
            n += 1;
        }
        std::fs::copy(&src, &dest).map_err(err)?;
        if let Ok(r) = dest.strip_prefix(root) {
            added.push(r.to_string_lossy().to_string());
        }
    }
    Ok(added)
}

#[tauri::command]
fn set_main(state: State<AppState>, project: String, rel: String) {
    state.update(|s| {
        s.mains.insert(project, rel);
    });
}

#[tauri::command]
async fn compile(state: State<'_, AppState>, project: String, main: String, full: Option<bool>) -> Res<compile::CompileResult> {
    let tectonic = compile::find_tectonic()
        .ok_or("Tectonic isn't installed. Install it with `brew install tectonic`, then recompile.")?;
    let out = library::build_dir(&state.cache_dir, &project);
    tauri::async_runtime::spawn_blocking(move || compile::run(&tectonic, Path::new(&project), &main, &out, full.unwrap_or(false)))
        .await
        .map_err(err)
}

#[tauri::command]
fn read_pdf(state: State<AppState>, path: String) -> Res<Response> {
    let p = PathBuf::from(&path);
    if !p.starts_with(&state.cache_dir) {
        return Err("Refusing to read outside the build cache".into());
    }
    Ok(Response::new(std::fs::read(&p).map_err(err)?))
}

fn synctex_for(state: &AppState, project: &str, main: &str) -> Option<std::sync::Arc<synctex::Synctex>> {
    let pdf = library::pdf_path(&state.cache_dir, project, main);
    let st = pdf.with_extension("synctex.gz");
    state.synctex.lock().unwrap().get(&st, Path::new(project))
}

#[tauri::command]
fn synctex_forward(state: State<AppState>, project: String, main: String, rel: String, line: u32) -> Option<synctex::Rect> {
    let s = synctex_for(&state, &project, &main)?;
    let file = files::resolve(Path::new(&project), &rel).ok()?;
    s.forward(&file, line)
}

#[derive(Serialize)]
struct InverseHit {
    rel: String,
    line: u32,
}

#[tauri::command]
fn synctex_inverse(state: State<AppState>, project: String, main: String, page: u32, x: f64, y: f64) -> Option<InverseHit> {
    let s = synctex_for(&state, &project, &main)?;
    let loc = s.inverse(page, x, y)?;
    let root = PathBuf::from(&project);
    let canon_root = std::fs::canonicalize(&root).unwrap_or(root.clone());
    let f = PathBuf::from(&loc.file);
    let rel = f.strip_prefix(&canon_root).or_else(|_| f.strip_prefix(&root)).ok()?;
    Some(InverseHit { rel: rel.to_string_lossy().to_string(), line: loc.line })
}

#[tauri::command]
fn export_pdf(src: String, dest: String) -> Res<()> {
    std::fs::copy(src, dest).map(|_| ()).map_err(err)
}

#[tauri::command]
fn reveal(path: String) -> Res<()> {
    std::process::Command::new("open").arg("-R").arg(path).spawn().map(|_| ()).map_err(err)
}

#[tauri::command]
fn open_external(path: String) -> Res<()> {
    std::process::Command::new("open").arg(path).spawn().map(|_| ()).map_err(err)
}

#[tauri::command]
fn save_thumb(state: State<AppState>, request: Request) -> Res<()> {
    let project = request
        .headers()
        .get("x-project")
        .and_then(|v| v.to_str().ok())
        .map(|s| decode_uri(s))
        .ok_or("missing project")?;
    let InvokeBody::Raw(bytes) = request.body() else { return Err("expected raw body".into()) };
    let p = library::thumb_path(&state.cache_dir, &project);
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d).map_err(err)?;
    }
    std::fs::write(p, bytes).map_err(err)
}

#[tauri::command]
fn read_thumb(state: State<AppState>, project: String) -> Res<Response> {
    let p = library::thumb_path(&state.cache_dir, &project);
    Ok(Response::new(std::fs::read(p).map_err(err)?))
}

fn decode_uri(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Test hook: only active when TEXAPP_AUTOMATION points at a directory.
#[tauri::command]
fn automation_report(text: String) {
    if let Ok(dir) = std::env::var("TEXAPP_AUTOMATION") {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(Path::new(&dir).join("out.log")) {
            let _ = writeln!(f, "{text}");
        }
    }
}

fn start_automation(app: &AppHandle) {
    let Ok(dir) = std::env::var("TEXAPP_AUTOMATION") else { return };
    #[cfg(target_os = "macos")]
    if let Some(w) = app.get_webview_window("main") {
        if let Ok(ns) = w.ns_window() {
            let num: isize = unsafe { objc2::msg_send![ns as *mut objc2::runtime::AnyObject, windowNumber] };
            let _ = std::fs::write(Path::new(&dir).join("winid"), num.to_string());
        }
    }
    let handle = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_millis(80));
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        let mut scripts: Vec<PathBuf> =
            rd.flatten().map(|e| e.path()).filter(|p| p.extension().map(|e| e == "js").unwrap_or(false)).collect();
        scripts.sort();
        for p in scripts {
            let js = std::fs::read_to_string(&p).unwrap_or_default();
            let _ = std::fs::remove_file(&p);
            if let Some(w) = handle.get_webview_window("main") {
                let _ = w.eval(&js);
            }
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            let cache_dir = app.path().app_cache_dir()?;
            std::fs::create_dir_all(&cache_dir)?;
            let config_path = config_dir.join("settings.json");
            let settings = Settings::load(&config_path);
            app.manage(AppState {
                settings: Mutex::new(settings),
                config_path,
                cache_dir,
                synctex: Mutex::new(synctex::Cache::default()),
                watcher: Mutex::new(None),
                lib_watcher: Mutex::new(None),
            });
            // Test hook for driving the UI from scripts; never compiled into release builds.
            if cfg!(debug_assertions) {
                start_automation(app.handle());
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            set_root,
            watch_library,
            list_library,
            create_folder,
            rename_folder,
            trash_folder,
            move_project,
            clipboard_text,
            create_project,
            import_zip,
            add_folder,
            forget_project,
            trash_project,
            open_project,
            close_project,
            list_tree,
            read_text,
            write_text,
            read_bytes,
            create_entry,
            rename_entry,
            trash_entry,
            import_files,
            set_main,
            compile,
            read_pdf,
            synctex_forward,
            synctex_inverse,
            export_pdf,
            reveal,
            open_external,
            save_thumb,
            read_thumb,
            automation_report,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tex");
}
