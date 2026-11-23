//! Tauri commands the front end calls. Heavy work runs on a blocking thread so the
//! window stays responsive. Renames always re-run the preview here, so the UI can never
//! rename something the engine has not checked.

use crate::applog;
use crate::engine::{self, Env, InfoCache, Item, Options, Preview, ReplacePair, Rule, Status};
use crate::listing::{self, Entry, Filters};
use crate::meta::FileInfo;
use crate::ops::{self, Report};
use crate::script::{QuickJs, ScriptState};
use crate::shell::{self, ShellStatus};
use crate::store::{Batch, DataStatus, Store};
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager, State};

/// Shared app state.
pub struct AppState {
    pub store: Store,
    pub cache: InfoCache,
    pub scripts: ScriptState,
    /// Paths or a preset passed on the command line, waiting for the UI.
    pub launch_args: Mutex<Vec<String>>,
}

type Res<T> = Result<T, String>;

async fn blocking<T: Send + 'static>(
    app: AppHandle,
    f: impl FnOnce(&AppState) -> Res<T> + Send + 'static,
) -> Res<T> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        f(&state)
    })
    .await
    .map_err(|e| e.to_string())?
}

// ------------------------------------------------------------------ data and settings

#[tauri::command]
pub fn data_status(state: State<AppState>) -> DataStatus {
    state.store.status()
}

#[tauri::command]
pub fn use_data_folder(state: State<AppState>, path: String) -> DataStatus {
    let dir = PathBuf::from(path).join(crate::store::DATA_DIR_NAME);
    applog::info(
        "settings",
        &format!("Data folder changed to {}", dir.display()),
    );
    state.store.set_dir(dir.clone());
    applog::relocate(&dir);
    *state.scripts.scripts_dir.lock().unwrap() = Some(state.store.dir().join("scripts"));
    state.store.status()
}

#[tauri::command]
pub fn load_settings(state: State<AppState>) -> serde_json::Value {
    state.store.load_settings()
}

#[tauri::command]
pub fn save_settings(state: State<AppState>, settings: serde_json::Value) -> Res<()> {
    state.store.save_settings(&settings)
}

#[tauri::command]
pub fn default_preset(state: State<AppState>) -> Option<String> {
    state.store.default_preset()
}

#[tauri::command]
pub fn save_default_preset(state: State<AppState>, text: String) -> Res<()> {
    state.store.save_default_preset(&text)
}

#[tauri::command]
pub fn launch_args(state: State<AppState>) -> Vec<String> {
    std::mem::take(&mut *state.launch_args.lock().unwrap())
}

// ------------------------------------------------------------------ listing

#[tauri::command]
pub async fn scan(app: AppHandle, roots: Vec<String>, filters: Filters) -> Res<Vec<Entry>> {
    blocking(app, move |s| {
        let js = if filters.condition.trim().is_empty() {
            None
        } else {
            Some(QuickJs::new(&s.scripts)?)
        };
        let t = std::time::Instant::now();
        let r = listing::scan(&roots, &filters, js.as_ref());
        match &r {
            Ok(list) => applog::info(
                "list",
                &format!(
                    "Listed {} item(s) from {} in {} ms (mask {}, files {}, folders {}, hidden {}, subfolders {}, condition {})",
                    list.len(),
                    roots.join(", "),
                    t.elapsed().as_millis(),
                    if filters.mask.trim() == "*" { "all" } else { "set" },
                    filters.files,
                    filters.folders,
                    filters.hidden,
                    filters.subfolders,
                    if filters.condition.trim().is_empty() { "none" } else { "set" },
                ),
            ),
            Err(e) => applog::warn("list", &format!("Listing {} failed: {e}", roots.join(", "))),
        }
        r
    })
    .await
}

#[tauri::command]
pub async fn stat_paths(app: AppHandle, paths: Vec<String>) -> Res<Vec<Entry>> {
    blocking(app, move |_| Ok(listing::stat_paths(&paths))).await
}

/// Extra column values: taken, dimensions, duration, or a custom tag template.
#[tauri::command]
pub async fn columns(
    app: AppHandle,
    paths: Vec<String>,
    kinds: Vec<String>,
    custom: String,
) -> Res<Vec<HashMap<String, String>>> {
    blocking(app, move |s| {
        Ok(paths
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let path = Path::new(p);
                let info = s.cache.get(path, path.is_dir());
                let mut row = HashMap::new();
                for k in &kinds {
                    let v = match k.as_str() {
                        "taken" => info
                            .date_taken("original")
                            .map(|d| d.format("%Y-%m-%d %H:%M:%S").to_string()),
                        "itemDate" => info
                            .item_date()
                            .map(|d| d.format("%Y-%m-%d %H:%M:%S").to_string()),
                        "dimensions" => info.property("dimensions"),
                        "duration" => info.property("duration"),
                        "custom" if !custom.is_empty() => {
                            let name = path
                                .file_name()
                                .map(|n| n.to_string_lossy().into_owned())
                                .unwrap_or_default();
                            let (stem, ext) = engine::split_name(&name);
                            let tags = engine::ItemTags {
                                info: info.clone(),
                                stem: stem.into(),
                                ext: ext.into(),
                                counter: i + 1,
                                removed: vec![],
                                clipboard: String::new(),
                                date_format: "%Y-%m-%d".into(),
                                numfiles: 0,
                            };
                            Some(engine::tags::expand(&custom, &tags))
                        }
                        _ => None,
                    };
                    if let Some(v) = v {
                        row.insert(k.clone(), v);
                    }
                }
                row
            })
            .collect())
    })
    .await
}

#[tauri::command]
pub async fn properties(app: AppHandle, path: String) -> Res<BTreeMap<String, String>> {
    blocking(app, move |s| {
        let p = Path::new(&path);
        Ok(s.cache.get(p, p.is_dir()).all_properties())
    })
    .await
}

/// A data URL for the picture viewer (images up to 40 MB the web view can show).
#[tauri::command]
pub async fn image_preview(app: AppHandle, path: String) -> Res<Option<String>> {
    blocking(app, move |_| {
        use base64::Engine as _;
        let p = Path::new(&path);
        let ext = p
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let mime = match ext.as_str() {
            "jpg" | "jpeg" | "jpe" | "jfif" => "image/jpeg",
            "png" => "image/png",
            "gif" => "image/gif",
            "webp" => "image/webp",
            "bmp" => "image/bmp",
            "ico" => "image/x-icon",
            "svg" => "image/svg+xml",
            "avif" => "image/avif",
            "tif" | "tiff" => "image/tiff",
            "heic" | "heif" => "image/heic",
            _ => return Ok(None),
        };
        let md = std::fs::metadata(p).map_err(|e| e.to_string())?;
        if md.len() > 40 * 1024 * 1024 {
            return Ok(None);
        }
        let bytes = std::fs::read(p).map_err(|e| e.to_string())?;
        Ok(Some(format!(
            "data:{mime};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        )))
    })
    .await
}

// ------------------------------------------------------------------ preview and rename

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewReq {
    pub items: Vec<Item>,
    pub rules: Vec<Rule>,
    pub options: Options,
    #[serde(default)]
    pub trace: Vec<String>,
}

fn run_preview(s: &AppState, req: &PreviewReq) -> Res<Preview> {
    let needs_js = req.rules.iter().any(|r| matches!(r, Rule::Script { .. }));
    let js = if needs_js {
        Some(QuickJs::new(&s.scripts)?)
    } else {
        None
    };
    let trace: HashSet<String> = req.trace.iter().cloned().collect();
    let env = Env {
        opts: &req.options,
        cache: &s.cache,
        script: js.as_ref().map(|j| j as &dyn engine::ScriptHost),
        trace: &trace,
    };
    Ok(engine::preview(&req.items, &req.rules, &env))
}

#[tauri::command]
pub async fn preview(app: AppHandle, req: PreviewReq) -> Res<Preview> {
    blocking(app, move |s| run_preview(s, &req)).await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameReq {
    #[serde(flatten)]
    pub preview: PreviewReq,
    #[serde(default)]
    pub reverse: bool,
    #[serde(default)]
    pub log: bool,
}

#[tauri::command]
pub async fn rename(app: AppHandle, req: RenameReq) -> Res<Report> {
    blocking(app, move |s| {
        // File facts may have changed since the last preview.
        for it in &req.preview.items {
            s.cache.forget(Path::new(&it.path));
        }
        let p = run_preview(s, &req.preview)?;
        let o = &req.preview.options;
        let copy = o.copy && !o.new_location.trim().is_empty();
        let plan = ops::Plan {
            items: &p.items,
            copy,
            overwrite: o.overwrite,
            reverse: req.reverse,
            attributes: &o.attributes,
            set_times: o.timestamps != engine::Timestamps::default()
                || req
                    .preview
                    .rules
                    .iter()
                    .any(|r| matches!(r, Rule::Script { .. })),
        };
        let kinds: Vec<&str> = req.preview.rules.iter().map(|r| r.label()).collect();
        applog::info(
            "rename",
            &format!(
                "Rename started: {} item(s), steps [{}], copy {}, new location {}, overwrite {}, reverse {}",
                p.items.len(),
                kinds.join(", "),
                copy,
                if o.new_location.trim().is_empty() { "none" } else { "set" },
                o.overwrite,
                req.reverse
            ),
        );
        let t = std::time::Instant::now();
        let (report, batch) = ops::execute(&plan);
        applog::info("rename", &format!("Rename finished in {} ms: {} done, {} not done", t.elapsed().as_millis(), report.renamed, report.failed));
        for r in report.results.iter().filter(|r| !r.ok) {
            applog::warn("rename", &format!("Not renamed {}: {}", r.path, r.message.clone().unwrap_or_default()));
        }
        for r in &report.results {
            s.cache.forget(Path::new(&r.path));
            s.cache.forget(Path::new(&r.new_path));
        }
        if req.log {
            let mut lines: Vec<String> = report
                .results
                .iter()
                .filter(|r| r.path != r.new_path || !r.ok)
                .map(|r| match (&r.ok, &r.message) {
                    (true, _) => format!(
                        "{} {} -> {}",
                        if copy { "COPY" } else { "RENAME" },
                        r.path,
                        r.new_path
                    ),
                    (false, m) => format!("FAILED {}: {}", r.path, m.clone().unwrap_or_default()),
                })
                .collect();
            lines.push(format!(
                "Done: {} changed, {} failed.",
                report.renamed, report.failed
            ));
            s.store.log(&lines);
        }
        if let Some(b) = batch {
            s.store.push_history(b)?;
        }
        Ok(report)
    })
    .await
}

/// Renames one item by hand (F2), outside the pipeline. Recorded for undo.
#[tauri::command]
pub async fn manual_rename(
    app: AppHandle,
    path: String,
    new_name: String,
    log: bool,
) -> Res<Report> {
    blocking(app, move |s| {
        let from = PathBuf::from(&path);
        if let Some(msg) = engine::validate_name(&new_name, false) {
            return Err(msg);
        }
        let to = from.with_file_name(&new_name);
        let item = engine::PreviewItem {
            path: path.clone(),
            original: from
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            new_name: new_name.clone(),
            new_path: to.to_string_lossy().into_owned(),
            status: if from == to {
                Status::Unchanged
            } else {
                Status::Changed
            },
            message: None,
            new_created: None,
            new_modified: None,
            new_accessed: None,
            trace: None,
            alerts: vec![],
        };
        if item.status == Status::Changed
            && to.exists()
            && to.to_string_lossy().to_lowercase() != path.to_lowercase()
        {
            return Err("An item with this name already exists.".into());
        }
        let attrs = engine::Attributes::default();
        let items = [item];
        let (report, batch) = ops::execute(&ops::Plan {
            items: &items,
            copy: false,
            overwrite: false,
            reverse: false,
            attributes: &attrs,
            set_times: false,
        });
        s.cache.forget(&from);
        if log {
            s.store
                .log(&[format!("RENAME {} -> {}", path, to.display())]);
        }
        if let Some(b) = batch {
            s.store.push_history(b)?;
        }
        Ok(report)
    })
    .await
}

#[tauri::command]
pub fn history(state: State<AppState>) -> Vec<Batch> {
    state.store.history()
}

#[tauri::command]
pub async fn undo(app: AppHandle, id: String, log: bool) -> Res<Report> {
    blocking(app, move |s| {
        let mut h = s.store.history();
        let b = h
            .iter_mut()
            .find(|b| b.id == id)
            .ok_or("This rename is no longer in the history.")?;
        if b.undone {
            return Err("This rename has already been undone.".into());
        }
        let report = ops::undo(b);
        applog::info(
            "undo",
            &format!(
                "Undo of batch {} ({}): {} restored, {} failed",
                b.id, b.summary, report.renamed, report.failed
            ),
        );
        for r in report.results.iter().filter(|r| !r.ok) {
            applog::warn(
                "undo",
                &format!(
                    "Not restored {}: {}",
                    r.path,
                    r.message.clone().unwrap_or_default()
                ),
            );
        }
        b.undone = report.failed == 0 || report.renamed > 0;
        for r in &report.results {
            s.cache.forget(Path::new(&r.path));
            s.cache.forget(Path::new(&r.new_path));
        }
        if log {
            let mut lines: Vec<String> = report
                .results
                .iter()
                .map(|r| {
                    format!(
                        "UNDO {} -> {}{}",
                        r.path,
                        r.new_path,
                        if r.ok { "" } else { " (failed)" }
                    )
                })
                .collect();
            lines.push(format!(
                "Undo done: {} restored, {} failed.",
                report.renamed, report.failed
            ));
            s.store.log(&lines);
        }
        s.store.replace_history(&h)?;
        Ok(report)
    })
    .await
}

#[tauri::command]
pub fn clear_history(state: State<AppState>) -> Res<()> {
    state.store.replace_history(&[])
}

/// Writes an undo script for a batch to `path` (.bat on Windows, .sh elsewhere).
#[tauri::command]
pub fn save_undo_script(state: State<AppState>, id: String, path: String) -> Res<()> {
    let h = state.store.history();
    let b = h
        .iter()
        .find(|b| b.id == id)
        .ok_or("This rename is no longer in the history.")?;
    let text = ops::undo_script(b, cfg!(windows));
    std::fs::write(&path, text).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755));
    }
    Ok(())
}

#[tauri::command]
pub fn log_file(state: State<AppState>) -> String {
    state.store.log_path().to_string_lossy().into_owned()
}

// ------------------------------------------------------------------ presets and imports

#[tauri::command]
pub fn read_text_file(path: String) -> Res<String> {
    let bytes = std::fs::read(&path).map_err(|e| format!("Cannot read {path}: {e}"))?;
    Ok(decode_text(&bytes))
}

#[tauri::command]
pub fn write_text_file(path: String, text: String) -> Res<()> {
    std::fs::write(&path, text).map_err(|e| format!("Cannot save {path}: {e}"))
}

/// The `_autoload.alterix` preset in a folder, if any.
#[tauri::command]
pub fn find_autoload(dir: String) -> Option<String> {
    let p = Path::new(&dir).join("_autoload.alterix");
    std::fs::read(&p).ok().map(|b| decode_text(&b))
}

/// Decodes UTF-8 (with or without BOM), UTF-16 LE/BE or Latin-1 text.
pub fn decode_text(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
        let be = bytes[0] == 0xFE;
        let units: Vec<u16> = bytes[2..]
            .chunks(2)
            .filter(|c| c.len() == 2)
            .map(|c| {
                if be {
                    u16::from_be_bytes([c[0], c[1]])
                } else {
                    u16::from_le_bytes([c[0], c[1]])
                }
            })
            .collect();
        return String::from_utf16_lossy(&units);
    }
    let b = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    match std::str::from_utf8(b) {
        Ok(s) => s.to_string(),
        Err(_) => b.iter().map(|&c| c as char).collect(),
    }
}

/// Parses rename pairs: one "old|new", "old,new" or "old<TAB>new" per line. A field
/// that contains a comma is wrapped in double quotes.
#[tauri::command]
pub fn parse_pairs(text: String) -> Vec<ReplacePair> {
    parse_pair_lines(&text)
}

pub fn parse_pair_lines(text: &str) -> Vec<ReplacePair> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim_end_matches('\r');
            if line.trim().is_empty() {
                return None;
            }
            let mut fields = Vec::new();
            let mut cur = String::new();
            let mut quoted = false;
            let mut chars = line.chars().peekable();
            while let Some(c) = chars.next() {
                match c {
                    '"' if quoted && chars.peek() == Some(&'"') => {
                        cur.push('"');
                        chars.next();
                    }
                    '"' => quoted = !quoted,
                    '|' | ',' | '\t' if !quoted && fields.is_empty() => {
                        fields.push(std::mem::take(&mut cur))
                    }
                    _ => cur.push(c),
                }
            }
            fields.push(cur);
            if fields.len() < 2 {
                return None;
            }
            Some(ReplacePair {
                find: fields[0].trim().to_string(),
                with: fields[1].trim().to_string(),
            })
        })
        .collect()
}

// ------------------------------------------------------------------ shell integration

#[tauri::command]
pub fn shell_status() -> ShellStatus {
    shell::status()
}

#[tauri::command]
pub fn shell_install() -> Res<ShellStatus> {
    let r = shell::install();
    match &r {
        Ok(s) => applog::info(
            "shell",
            &format!("Open in Alterix added: {}", s.places.join(", ")),
        ),
        Err(e) => applog::warn("shell", &format!("Adding Open in Alterix failed: {e}")),
    }
    r
}

#[tauri::command]
pub fn shell_uninstall() -> Res<ShellStatus> {
    applog::info("shell", "Open in Alterix removed");
    shell::uninstall()
}

/// Reloads metadata for paths (after changes outside Alterix).
#[tauri::command]
pub fn refresh_paths(state: State<AppState>, paths: Vec<String>) {
    if paths.is_empty() {
        state.cache.clear();
    }
    for p in paths {
        state.cache.forget(Path::new(&p));
    }
}

#[allow(dead_code)]
fn _assert_send(_: Arc<FileInfo>) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairs_accept_pipes_commas_tabs_and_quotes() {
        let p = parse_pair_lines("Track001.mp3|Headlong.mp3\r\nTrack002.mp3,Rushes.mp3\na\tb\n\"x, y.txt\",\"z, w.txt\"\n\nnoseparator\n");
        let got: Vec<(&str, &str)> = p
            .iter()
            .map(|r| (r.find.as_str(), r.with.as_str()))
            .collect();
        assert_eq!(
            got,
            [
                ("Track001.mp3", "Headlong.mp3"),
                ("Track002.mp3", "Rushes.mp3"),
                ("a", "b"),
                ("x, y.txt", "z, w.txt")
            ]
        );
    }

    #[test]
    fn text_decoding() {
        assert_eq!(decode_text(&[0xEF, 0xBB, 0xBF, b'a']), "a");
        assert_eq!(decode_text(&[0xFF, 0xFE, b'a', 0]), "a");
        assert_eq!(decode_text(&[0xE9]), "é");
    }
}

// ------------------------------------------------------------------ app info, log, feedback

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub build_kind: String,
    pub os: String,
    pub arch: String,
    pub report_url: String,
}

#[tauri::command]
pub fn app_info() -> AppInfo {
    let version = applog::version();
    let os = applog::os_text();
    AppInfo {
        report_url: crate::feedback::report_url(&version, &os),
        build_kind: applog::build_kind(&version).into(),
        version,
        os,
        arch: std::env::consts::ARCH.into(),
    }
}

/// Writes a line from the UI (errors, settings changes, update checks).
#[tauri::command]
pub fn log_line(level: String, area: String, message: String) {
    applog::write(applog::Level::parse(&level), &area, &message);
}

/// The log file path, for View Logs. Creates the file if it does not exist yet.
#[tauri::command]
pub fn log_path() -> Res<String> {
    let p = applog::path().ok_or("Logging is not available.")?;
    if !p.exists() {
        applog::info("app", "Log file created for View Logs");
    }
    Ok(p.to_string_lossy().into_owned())
}
