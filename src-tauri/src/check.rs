//! Command-line checks of a built app, run without opening the window:
//! - `--self-test` loads what a build needs (web files, engine, scripts, data folder),
//!   prints "Alterix vX.Y.Z" and exits 0, or prints what is missing and exits 1.
//! - `--check-rename <preset> <folder> <data folder>` lists the folder with the preset's
//!   filters and renames it with the preset's steps, through the same code as the window.
//! - `--check-undo <data folder>` undoes the latest rename in that data folder's history.
//!
//! The check modes write their result to `<data folder>/check-result.json`, because a
//! windowed Windows build has no console. scripts/check_e2e.py drives them.

use crate::commands::{self, AppState, PreviewReq, RenameReq};
use crate::engine::{self, Env, InfoCache, Item, Options, Rule};
use crate::listing::{self, Filters};
use crate::script::{QuickJs, ScriptState};
use crate::{applog, store::Store};
use serde::Deserialize;
use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Runs a check mode if the arguments ask for one, and returns its exit code.
pub fn from_args<R: tauri::Runtime>(args: &[String], context: &tauri::Context<R>) -> Option<i32> {
    let mode = args.get(1)?.as_str();
    let rest = &args[2..];
    Some(match mode {
        "--self-test" => {
            let assets = context.assets();
            self_test(&|key: &str| assets.get(&key.into()).map(|b| b.into_owned()))
        }
        "--check-rename" if rest.len() == 3 => {
            finish(&rest[2], check_rename(&rest[0], &rest[1], &rest[2]))
        }
        "--check-undo" if rest.len() == 1 => finish(&rest[0], check_undo(&rest[0])),
        "--check-rename" | "--check-undo" => {
            say("Usage: --check-rename <preset> <folder> <data folder> | --check-undo <data folder>");
            2
        }
        _ => return None,
    })
}

/// Prints a line; never fails, because a windowed Windows build has no stdout.
fn say(line: &str) {
    let _ = writeln!(std::io::stdout(), "{line}");
}

fn state_at(dir: &Path) -> AppState {
    let scripts = ScriptState::default();
    *scripts.scripts_dir.lock().unwrap() = Some(dir.join("scripts"));
    AppState {
        store: Store::at(dir.to_path_buf()),
        cache: InfoCache::default(),
        scripts,
        launch_args: Mutex::new(vec![]),
    }
}

// ---------------------------------------------------------------------------- self-test

/// Looks up a bundled web file by its path, such as "index.html".
type AssetFn<'a> = &'a dyn Fn(&str) -> Option<Vec<u8>>;

fn self_test(asset: AssetFn) -> i32 {
    let dir = std::env::temp_dir().join(format!("alterix-self-test-{}", std::process::id()));
    let problems = self_test_in(&dir, asset);
    let _ = std::fs::remove_dir_all(&dir);
    for p in &problems {
        say(&format!("Missing: {p}"));
    }
    say(&format!("Alterix {}", applog::version()));
    i32::from(!problems.is_empty())
}

fn self_test_in(dir: &Path, asset: AssetFn) -> Vec<String> {
    let mut problems = vec![];
    // Web files: index.html and every script and stylesheet it loads.
    match asset("index.html") {
        None => problems.push("bundled web page index.html".into()),
        Some(html) => {
            let html = String::from_utf8_lossy(&html).into_owned();
            let refs = asset_refs(&html);
            if refs.is_empty() {
                problems.push("scripts in index.html".into());
            }
            for r in refs {
                if asset(&r).is_none() {
                    problems.push(format!("bundled web file {r}"));
                }
            }
        }
    }
    // Data folder: a temporary one, so nothing is written beside the build.
    let state = state_at(dir);
    if !state.store.status().writable {
        problems.push(format!("writable data folder ({})", dir.display()));
        return problems;
    }
    // Engine and JavaScript host on two made-up files.
    let files = ["holiday 01.jpg", "holiday 02.jpg"];
    for f in files {
        if std::fs::write(dir.join(f), b"made-up").is_err() {
            problems.push(format!("sample file {f}"));
        }
    }
    let items: Vec<Item> = files
        .iter()
        .map(|f| Item {
            path: dir.join(f).to_string_lossy().into_owned(),
            is_dir: false,
            root: dir.to_string_lossy().into_owned(),
        })
        .collect();
    let rules = vec![
        Rule::Script {
            code: "newName = name.toUpperCase();".into(),
        },
        Rule::Extension {
            mode: engine::ExtMode::Upper,
            value: String::new(),
        },
    ];
    let js = match QuickJs::new(&state.scripts) {
        Ok(js) => Some(js),
        Err(e) => {
            problems.push(format!("JavaScript host: {e}"));
            None
        }
    };
    let opts = Options::default();
    let trace = HashSet::new();
    let env = Env {
        opts: &opts,
        cache: &state.cache,
        script: js.as_ref().map(|j| j as &dyn engine::ScriptHost),
        trace: &trace,
    };
    let names: Vec<String> = engine::preview(&items, &rules, &env)
        .items
        .into_iter()
        .map(|i| i.new_name)
        .collect();
    if names != ["HOLIDAY 01.JPG", "HOLIDAY 02.JPG"] {
        problems.push(format!("rename engine (got {names:?})"));
    }
    problems
}

/// The `src` and `href` values of script and stylesheet tags in the built index.html.
fn asset_refs(html: &str) -> Vec<String> {
    let mut out = vec![];
    for attr in ["src=\"", "href=\""] {
        for (i, _) in html.match_indices(attr) {
            let start = i + attr.len();
            if let Some(len) = html[start..].find('"') {
                let v = html[start..start + len].trim_start_matches("./");
                let v = v.trim_start_matches('/');
                if v.ends_with(".js") || v.ends_with(".css") {
                    out.push(v.to_string());
                }
            }
        }
    }
    out
}

// --------------------------------------------------------------------------- check modes

#[derive(Deserialize)]
struct Preset {
    #[serde(default)]
    steps: Vec<PresetStep>,
    #[serde(default)]
    options: Options,
    #[serde(default)]
    filters: Filters,
}

#[derive(Deserialize)]
struct PresetStep {
    #[serde(default = "yes")]
    enabled: bool,
    rule: Rule,
}

fn yes() -> bool {
    true
}

fn check_rename(preset: &str, folder: &str, data: &str) -> Result<serde_json::Value, String> {
    let state = start(data)?;
    let text = std::fs::read_to_string(preset).map_err(|e| format!("Cannot read {preset}: {e}"))?;
    let preset: Preset = serde_json::from_str(&commands::decode_text(text.as_bytes()))
        .map_err(|e| format!("Preset: {e}"))?;
    let js = if preset.filters.condition.trim().is_empty() {
        None
    } else {
        Some(QuickJs::new(&state.scripts)?)
    };
    let entries = listing::scan(&[folder.to_string()], &preset.filters, js.as_ref())?;
    drop(js);
    applog::info(
        "check",
        &format!("Check rename: {} item(s) listed", entries.len()),
    );
    let items = entries
        .iter()
        .map(|e| Item {
            path: e.path.clone(),
            is_dir: e.is_dir,
            root: e.root.clone(),
        })
        .collect();
    let rules = preset
        .steps
        .into_iter()
        .filter(|s| s.enabled)
        .map(|s| s.rule)
        .collect();
    let report = commands::rename_now(
        &state,
        RenameReq {
            preview: PreviewReq {
                items,
                rules,
                options: preset.options,
                trace: vec![],
            },
            reverse: false,
            log: true,
        },
    )?;
    Ok(serde_json::json!({ "listed": entries.len(), "report": report }))
}

fn check_undo(data: &str) -> Result<serde_json::Value, String> {
    let state = start(data)?;
    let id = state
        .store
        .history()
        .into_iter()
        .rev()
        .find(|b| !b.undone)
        .map(|b| b.id)
        .ok_or("No rename to undo in this data folder.")?;
    let report = commands::undo_now(&state, &id, true)?;
    Ok(serde_json::json!({ "report": report }))
}

fn start(data: &str) -> Result<AppState, String> {
    let state = state_at(&PathBuf::from(data));
    let status = state.store.status();
    if !status.writable {
        return Err(format!("Cannot write the data folder {data}"));
    }
    applog::start(&state.store.dir(), true);
    Ok(state)
}

/// Writes check-result.json and returns the exit code.
fn finish(data: &str, result: Result<serde_json::Value, String>) -> i32 {
    let (code, mut value) = match result {
        Ok(mut v) => {
            let failed = v["report"]["failed"].as_u64().unwrap_or(0);
            v["ok"] = (failed == 0).into();
            (i32::from(failed != 0), v)
        }
        Err(e) => (1, serde_json::json!({ "ok": false, "error": e })),
    };
    value["version"] = applog::version().into();
    let text = serde_json::to_string_pretty(&value).unwrap_or_default();
    let _ = std::fs::create_dir_all(data);
    let _ = std::fs::write(Path::new(data).join("check-result.json"), &text);
    say(&text);
    applog::end();
    code
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_script_and_style_refs() {
        let html = r#"<script type="module" crossorigin src="/assets/index-a.js"></script>
<link rel="stylesheet" crossorigin href="./assets/index-b.css"><link rel="icon" href="/x.svg">"#;
        assert_eq!(
            asset_refs(html),
            ["assets/index-a.js", "assets/index-b.css"]
        );
    }

    #[test]
    fn self_test_passes_from_source() {
        // A stand-in for the bundled web files; the real ones are checked by --self-test on each build.
        let files = |key: &str| match key {
            "index.html" => {
                Some(br#"<script type="module" src="/assets/app.js"></script>"#.to_vec())
            }
            "assets/app.js" => Some(b"made-up".to_vec()),
            _ => None,
        };
        let dir =
            std::env::temp_dir().join(format!("alterix-self-test-unit-{}", std::process::id()));
        let problems = self_test_in(&dir, &files);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(problems.is_empty(), "{problems:?}");
    }

    #[test]
    fn self_test_reports_a_missing_web_file() {
        let files = |key: &str| match key {
            "index.html" => {
                Some(br#"<script type="module" src="/assets/app.js"></script>"#.to_vec())
            }
            _ => None,
        };
        let dir =
            std::env::temp_dir().join(format!("alterix-self-test-miss-{}", std::process::id()));
        let problems = self_test_in(&dir, &files);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(problems, ["bundled web file assets/app.js"]);
    }
}
