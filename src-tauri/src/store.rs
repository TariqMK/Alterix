//! Portable data storage. Everything Alterix keeps lives in one folder beside the app
//! (`Alterix-data/`, beside the .app on macOS): settings, undo history, the rename
//! activity log and the default preset. Nothing is written anywhere else.

use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const DATA_DIR_NAME: &str = "Alterix-data";
const LOG_LIMIT: u64 = 1024 * 1024;
const HISTORY_LIMIT: usize = 50;

/// The folder the data folder is created in: beside the executable, or beside the
/// .app bundle on macOS. When run from source, the project root.
pub fn default_base() -> PathBuf {
    if cfg!(debug_assertions) {
        if let Some(root) = Path::new(env!("CARGO_MANIFEST_DIR")).parent() {
            return root.to_path_buf();
        }
    }
    let exe = std::env::current_exe().unwrap_or_default();
    let exe = exe.canonicalize().unwrap_or(exe);
    let dir = exe.parent().map(Path::to_path_buf).unwrap_or_default();
    // …/Alterix.app/Contents/MacOS/alterix → …/
    if dir.ends_with("Contents/MacOS") {
        if let Some(app) = dir.parent().and_then(Path::parent) {
            if app.extension().map(|e| e == "app").unwrap_or(false) {
                return app.parent().map(Path::to_path_buf).unwrap_or(dir);
            }
        }
    }
    // An AppImage runs from a temporary mount; use the AppImage file's folder.
    if let Ok(appimage) = std::env::var("APPIMAGE") {
        if let Some(p) = Path::new(&appimage).parent() {
            return p.to_path_buf();
        }
    }
    dir
}

/// Owns the data folder location. The location can change for the session when the
/// default one is not writable and the user picks another.
pub struct Store {
    dir: Mutex<PathBuf>,
    lock: Mutex<()>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataStatus {
    pub path: String,
    pub writable: bool,
}

impl Store {
    pub fn new() -> Self {
        Store {
            dir: Mutex::new(default_base().join(DATA_DIR_NAME)),
            lock: Mutex::new(()),
        }
    }

    pub fn at(dir: PathBuf) -> Self {
        Store {
            dir: Mutex::new(dir),
            lock: Mutex::new(()),
        }
    }

    pub fn dir(&self) -> PathBuf {
        self.dir.lock().unwrap().clone()
    }

    pub fn set_dir(&self, dir: PathBuf) {
        *self.dir.lock().unwrap() = dir;
    }

    /// Creates the folder if needed and checks that it can be written.
    pub fn status(&self) -> DataStatus {
        let dir = self.dir();
        let writable = std::fs::create_dir_all(&dir).is_ok() && {
            let probe = dir.join(".write-test");
            let ok = std::fs::write(&probe, b"ok").is_ok();
            let _ = std::fs::remove_file(&probe);
            ok
        };
        DataStatus {
            path: dir.to_string_lossy().into_owned(),
            writable,
        }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir().join(name)
    }

    fn read(&self, name: &str) -> Option<String> {
        std::fs::read_to_string(self.path(name)).ok()
    }

    /// Writes atomically: a temp file, then rename over the old one.
    fn write(&self, name: &str, text: &str) -> Result<(), String> {
        let _g = self.lock.lock().unwrap();
        let dir = self.dir();
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
        let tmp = dir.join(format!(".{name}.tmp"));
        std::fs::write(&tmp, text).map_err(|e| format!("Cannot save {name}: {e}"))?;
        std::fs::rename(&tmp, dir.join(name)).map_err(|e| format!("Cannot save {name}: {e}"))
    }

    // ------------------------------------------------------------ settings (opaque JSON)

    pub fn load_settings(&self) -> serde_json::Value {
        self.read("settings.json")
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or(serde_json::Value::Null)
    }

    pub fn save_settings(&self, v: &serde_json::Value) -> Result<(), String> {
        self.write(
            "settings.json",
            &serde_json::to_string_pretty(v).map_err(|e| e.to_string())?,
        )
    }

    /// Reads one boolean setting before the UI starts (for single-instance).
    pub fn setting_bool(&self, key: &str) -> Option<bool> {
        self.load_settings()
            .get(key)
            .and_then(serde_json::Value::as_bool)
    }

    pub fn default_preset(&self) -> Option<String> {
        self.read("default.alterix")
    }

    pub fn save_default_preset(&self, text: &str) -> Result<(), String> {
        self.write("default.alterix", text)
    }

    // ------------------------------------------------------------ undo history

    pub fn history(&self) -> Vec<Batch> {
        self.read("history.json")
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn push_history(&self, batch: Batch) -> Result<(), String> {
        let mut h = self.history();
        h.insert(0, batch);
        h.truncate(HISTORY_LIMIT);
        self.write(
            "history.json",
            &serde_json::to_string(&h).map_err(|e| e.to_string())?,
        )
    }

    pub fn replace_history(&self, h: &[Batch]) -> Result<(), String> {
        self.write(
            "history.json",
            &serde_json::to_string(h).map_err(|e| e.to_string())?,
        )
    }

    // ------------------------------------------------------------ rename activity log

    pub fn log_path(&self) -> PathBuf {
        self.path("rename-activity.log")
    }

    /// Appends lines to the activity log, keeping it under 1 MB by dropping the oldest half.
    pub fn log(&self, lines: &[String]) {
        let _g = self.lock.lock().unwrap();
        let p = self.log_path();
        if let Some(parent) = p.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if std::fs::metadata(&p)
            .map(|m| m.len() > LOG_LIMIT)
            .unwrap_or(false)
        {
            if let Ok(text) = std::fs::read_to_string(&p) {
                let keep = &text[text.len() / 2..];
                let keep = keep.find('\n').map(|i| &keep[i + 1..]).unwrap_or(keep);
                let _ = std::fs::write(&p, keep);
            }
        }
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&p)
        {
            let stamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
            for l in lines {
                let _ = writeln!(f, "[{stamp}] {l}");
            }
        }
    }
}

impl Default for Store {
    fn default() -> Self {
        Self::new()
    }
}

/// One undoable operation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UndoOp {
    pub from: String,
    pub to: String,
    pub is_dir: bool,
    /// True when the item was copied (undo moves the copy to the trash).
    pub copied: bool,
    /// Created / modified / accessed before the change, when they were changed.
    pub old_times: Option<[Option<String>; 3]>,
    /// Attribute letters before the change, when they were changed.
    pub old_attrs: Option<String>,
}

/// One rename run, as shown in the undo history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Batch {
    pub id: String,
    pub when: String,
    pub summary: String,
    pub ops: Vec<UndoOp>,
    #[serde(default)]
    pub undone: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_history_and_log_round_trip() {
        let dir = std::env::temp_dir().join(format!("alterix-store-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let s = Store::at(dir.clone());
        assert!(s.status().writable);
        s.save_settings(&serde_json::json!({"theme": "dark", "singleInstance": false}))
            .unwrap();
        assert_eq!(s.load_settings()["theme"], "dark");
        assert_eq!(s.setting_bool("singleInstance"), Some(false));
        let batch = Batch {
            id: "1".into(),
            when: "now".into(),
            summary: "x".into(),
            ops: vec![],
            undone: false,
        };
        for _ in 0..60 {
            s.push_history(batch.clone()).unwrap();
        }
        assert_eq!(s.history().len(), HISTORY_LIMIT);
        s.log(&["one".into()]);
        assert!(std::fs::read_to_string(s.log_path())
            .unwrap()
            .contains("one"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
