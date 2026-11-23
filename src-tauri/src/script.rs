//! JavaScript renaming and filter conditions, run in an embedded QuickJS engine.
//! Scripts see variables and helper functions for renaming (newName, origName, object(), exif(), fileProperty(), regEx(),
//! formatDateTime(), askOnce() …). Scripts have no file or network access beyond the
//! helpers, and each run stops after a time limit.

use crate::engine::{self, regexes, tags, ScriptCtx, ScriptHost, ScriptOut};
use crate::meta::{self, FileInfo};
use rquickjs::{Context, Ctx, Function, Runtime};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const TIME_LIMIT: Duration = Duration::from_secs(3);

/// Helpers written in JavaScript on top of the native functions.
const PRELUDE: &str = r#"
var __d = function (s) { return s ? new Date(s) : null; };
function object(key) {
  var k = String(key).toLowerCase();
  var m = /^container\[(\d)\]$/.exec(k);
  if (m) { return __containers[Number(m[1]) - 1] || ""; }
  switch (k) {
    case "folder": return __obj.folder;
    case "container": return __containers[0] || "";
    case "containers": return __obj.containers;
    case "autonumber": return __obj.autonumber;
    case "autodate": return __obj.autodate;
    case "isdir": return __obj.isdir;
    case "size": return __obj.size;
    case "attr": return __obj.attr || "-";
    case "modified": return __d(__obj.modified);
    case "created": return __d(__obj.created);
    case "accessed": return __d(__obj.accessed);
  }
  return null;
}
function exif(code) {
  var map = { "%d": "$taken", "taken": "$taken", "%a": "exif:FNumber", "%c": "exif:UserComment",
    "%e": "exif:ExposureTime", "%f": "exif:FocalLength", "%xb": "exif:ExposureBiasValue",
    "%ma": "exif:Make", "%mo": "exif:Model" };
  var p = map[code] || code;
  if (p === "$taken") { return filePropertyDate("exif:DateTimeOriginal"); }
  return fileProperty(p);
}
function filePropertyDate(n) { var v = __propDate(String(n)); return v ? new Date(v) : null; }
function filePropertyNum(n) { return __propNum(String(n)); }
function removeExt(t) { t = String(t); var i = t.lastIndexOf("."); return i > 0 ? t.substring(0, i) : t; }
function padNum(n, width, pat) {
  var s = String(n); pat = pat === undefined ? "0" : String(pat); width = Math.min(Number(width) || 0, 1000);
  if (s.length >= width || pat.length === 0) { return s; }
  var need = width - s.length, pad = "";
  while (pad.length < need) { pad += pat; }
  return pad.substring(0, need) + s;
}
function formatDateTime(d, fmt, locale) {
  var ms = (d instanceof Date) ? d.getTime() : Number(d);
  return __fmt(ms, String(fmt === undefined ? "YYYY-MM-DD HH:mm:ss" : fmt));
}
function askOnce(prompt, def) {
  var a = __answers[String(prompt)];
  return (a === undefined || a === "") ? (def === undefined ? "" : String(def)) : a;
}
function clipboard() { return __clip; }
var getenv = getEnv, setenv = setEnv;
"#;

/// Shared state that lives as long as the app: setEnv / getEnv values.
#[derive(Default)]
pub struct ScriptState {
    env: Arc<Mutex<HashMap<String, String>>>,
    /// Folder that include() / require() read from.
    pub scripts_dir: Mutex<Option<PathBuf>>,
}

/// A QuickJS host for one preview or listing. Not shared between threads.
pub struct QuickJs<'a> {
    rt: Runtime,
    state: &'a ScriptState,
}

impl<'a> QuickJs<'a> {
    pub fn new(state: &'a ScriptState) -> Result<Self, String> {
        let rt = Runtime::new().map_err(|e| e.to_string())?;
        rt.set_memory_limit(128 << 20);
        rt.set_max_stack_size(1 << 20);
        Ok(QuickJs { rt, state })
    }

    fn arm_timer(&self) {
        let deadline = Instant::now() + TIME_LIMIT;
        self.rt
            .set_interrupt_handler(Some(Box::new(move || Instant::now() > deadline)));
    }

    /// Evaluates a filter condition for one item. True keeps the item in the list.
    pub fn condition(
        &self,
        code: &str,
        info: Arc<FileInfo>,
        counter: usize,
    ) -> Result<bool, String> {
        let name_full = info
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let (stem, ext) = engine::split_name(&name_full);
        let ctx = ScriptCtx {
            info: info.clone(),
            name: stem.to_string(),
            ext: ext.trim_start_matches('.').to_string(),
            orig_name: stem.to_string(),
            orig_ext: ext.trim_start_matches('.').to_string(),
            location: String::new(),
            sub_dir: String::new(),
            counter,
            clipboard: String::new(),
            answers: HashMap::new(),
            autonumber: None,
            autodate: None,
            tags: engine::ItemTags {
                info: info.clone(),
                stem: stem.to_string(),
                ext: ext.to_string(),
                counter,
                removed: Vec::new(),
                clipboard: String::new(),
                date_format: "%Y-%m-%d".into(),
                numfiles: 0,
            },
        };
        let wrapped = format!(
            "!!(function(){{ return eval({}); }})()",
            serde_json::to_string(code).unwrap_or_default()
        );
        let mut result = false;
        self.with_ctx(ctx, false, |c| {
            result = c
                .eval::<bool, _>(wrapped.clone())
                .map_err(|e| js_error(c, e))?;
            Ok(())
        })?;
        Ok(result)
    }

    fn with_ctx(
        &self,
        sc: ScriptCtx,
        parse_tags: bool,
        body: impl FnOnce(&Ctx) -> Result<(), String>,
    ) -> Result<Vec<String>, String> {
        self.arm_timer();
        let context = Context::full(&self.rt).map_err(|e| e.to_string())?;
        let alerts: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
        let state = self.state;
        let out = context.with(|c| -> Result<(), String> {
            setup(&c, &sc, alerts.clone(), state, parse_tags).map_err(|e| js_error(&c, e))?;
            c.eval::<(), _>(PRELUDE).map_err(|e| js_error(&c, e))?;
            body(&c)
        });
        out?;
        let a = alerts.borrow().clone();
        Ok(a)
    }
}

fn js_error(c: &Ctx, e: rquickjs::Error) -> String {
    if let rquickjs::Error::Exception = e {
        let v = c.catch();
        if let Some(ex) = v.as_exception() {
            let msg = ex.message().unwrap_or_default();
            return format!("Script error: {msg}");
        }
        if let Some(s) = v.as_string().and_then(|s| s.to_string().ok()) {
            return format!("Script error: {s}");
        }
        return "Script error.".into();
    }
    if matches!(e, rquickjs::Error::Allocation) {
        return "Script stopped: it used too much memory.".into();
    }
    let s = e.to_string();
    if s.contains("interrupted") {
        return "Script stopped: it ran for more than 3 seconds.".into();
    }
    format!("Script error: {s}")
}

fn iso(d: Option<chrono::NaiveDateTime>) -> serde_json::Value {
    match d {
        Some(t) => serde_json::Value::String(t.format("%Y-%m-%dT%H:%M:%S").to_string()),
        None => serde_json::Value::Null,
    }
}

fn containers(path: &Path) -> Vec<String> {
    path.ancestors()
        .skip(1)
        .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .collect()
}

fn base_folder(info: &FileInfo) -> PathBuf {
    if info.is_dir {
        info.path.clone()
    } else {
        info.path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default()
    }
}

fn list_folder(dir: &Path, pattern: &str, hidden: bool, want_dirs: bool) -> Vec<String> {
    let pattern = if pattern.is_empty() { "*" } else { pattern };
    let mut out: Vec<String> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .filter(|e| {
                    e.file_type()
                        .map(|t| if want_dirs { t.is_dir() } else { t.is_file() })
                        .unwrap_or(false)
                })
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| hidden || !n.starts_with('.'))
                .filter(|n| engine::text::mask_matches(pattern, n, false) || pattern == "*.*")
                .collect()
        })
        .unwrap_or_default();
    out.sort_by_key(|n| n.to_lowercase());
    out
}

fn read_text(path: &Path, max_mb: f64) -> Option<String> {
    let md = std::fs::metadata(path).ok()?;
    if md.len() as f64 > max_mb * 1048576.0 {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
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
        return Some(String::from_utf16_lossy(&units));
    }
    Some(
        String::from_utf8_lossy(bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&bytes))
            .into_owned(),
    )
}

fn setup<'js>(
    c: &Ctx<'js>,
    sc: &ScriptCtx,
    alerts: Rc<RefCell<Vec<String>>>,
    state: &ScriptState,
    parse_tags: bool,
) -> rquickjs::Result<()> {
    let g = c.globals();
    let info = sc.info.clone();
    let obj = serde_json::json!({
        "folder": info.path.parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default(),
        "containers": info.path.ancestors().count().saturating_sub(1),
        "autonumber": sc.autonumber.clone().unwrap_or_default(),
        "autodate": sc.autodate.clone().unwrap_or_default(),
        "isdir": info.is_dir,
        "size": info.size,
        "attr": info.attrs,
        "modified": iso(info.modified),
        "created": iso(info.created),
        "accessed": iso(info.accessed),
    });
    let data = format!(
        "var __obj = {}; var __containers = {}; var __answers = {}; var __clip = {};",
        obj,
        serde_json::to_string(&containers(&info.path)).unwrap_or_default(),
        serde_json::to_string(&sc.answers).unwrap_or_default(),
        serde_json::to_string(&sc.clipboard).unwrap_or_default(),
    );
    c.eval::<(), _>(data)?;
    g.set("name", sc.name.clone())?;
    g.set("ext", sc.ext.clone())?;
    g.set("origName", sc.orig_name.clone())?;
    g.set("origExt", sc.orig_ext.clone())?;
    g.set("newName", sc.name.clone())?;
    g.set("newExt", sc.ext.clone())?;
    g.set("location", sc.location.clone())?;
    g.set("newLocation", sc.location.clone())?;
    g.set("subDir", sc.sub_dir.clone())?;
    g.set("counter", sc.counter as f64)?;
    c.eval::<(), _>("var newModified = null; var newCreated = null;")?;

    let i = info.clone();
    g.set(
        "fileProperty",
        Function::new(c.clone(), move |n: String| i.property(&n))?,
    )?;
    let i = info.clone();
    g.set(
        "__propDate",
        Function::new(c.clone(), move |n: String| {
            let lower = n.to_ascii_lowercase();
            let d = if lower == "system.itemdate" {
                i.item_date()
            } else {
                i.property(&n).and_then(|v| meta::parse_date(&v))
            };
            d.map(|t| t.format("%Y-%m-%dT%H:%M:%S").to_string())
        })?,
    )?;
    let i = info.clone();
    g.set(
        "__propNum",
        Function::new(c.clone(), move |n: String| {
            i.property(&n).and_then(|v| meta::as_number(&v))
        })?,
    )?;
    g.set(
        "__fmt",
        Function::new(c.clone(), |ms: f64, f: String| {
            chrono::DateTime::from_timestamp_millis(ms as i64)
                .map(|t| {
                    engine::dates::js_format(&t.with_timezone(&chrono::Local).naive_local(), &f)
                })
                .unwrap_or_default()
        })?,
    )?;
    let a = alerts.clone();
    g.set(
        "alert",
        Function::new(c.clone(), move |t: rquickjs::Coerced<String>| {
            a.borrow_mut().push(t.0)
        })?,
    )?;
    g.set(
        "regEx",
        Function::new(c.clone(), |t: String, m: String, r: String| {
            regexes::apply(&t, &m, &r, regexes::RegexMode::Standard).unwrap_or(t)
        })?,
    )?;
    let base = base_folder(&info);
    let b = base.clone();
    g.set(
        "countFilesInFolder",
        Function::new(
            c.clone(),
            move |p: rquickjs::function::Opt<String>,
                  h: rquickjs::function::Opt<bool>,
                  _s: rquickjs::function::Opt<bool>| {
                list_folder(
                    &b,
                    p.0.as_deref().unwrap_or(""),
                    h.0.unwrap_or(false),
                    false,
                )
                .len() as f64
            },
        )?,
    )?;
    let b = base.clone();
    g.set(
        "listFilesInFolder",
        Function::new(
            c.clone(),
            move |p: rquickjs::function::Opt<String>,
                  h: rquickjs::function::Opt<bool>,
                  _s: rquickjs::function::Opt<bool>| {
                list_folder(
                    &b,
                    p.0.as_deref().unwrap_or(""),
                    h.0.unwrap_or(false),
                    false,
                )
            },
        )?,
    )?;
    let b = base.clone();
    g.set(
        "listFoldersInFolder",
        Function::new(
            c.clone(),
            move |p: rquickjs::function::Opt<String>,
                  h: rquickjs::function::Opt<bool>,
                  _s: rquickjs::function::Opt<bool>| {
                list_folder(&b, p.0.as_deref().unwrap_or(""), h.0.unwrap_or(false), true)
            },
        )?,
    )?;
    let i = info.clone();
    g.set(
        "fileMatch",
        Function::new(
            c.clone(),
            move |p: String,
                  flags: rquickjs::function::Opt<String>,
                  occ: rquickjs::function::Opt<f64>,
                  max: rquickjs::function::Opt<f64>|
                  -> Option<String> {
                if i.is_dir {
                    return None;
                }
                let text = read_text(&i.path, max.0.unwrap_or(30.0))?;
                let ci = flags.0.unwrap_or_default().contains('i');
                let re = fancy_regex::RegexBuilder::new(&p)
                    .case_insensitive(ci)
                    .build()
                    .ok()?;
                let want = occ.0.unwrap_or(1.0).max(1.0) as usize;
                let mut n = 0;
                for line in text.lines() {
                    for m in re.find_iter(line).filter_map(Result::ok) {
                        n += 1;
                        if n == want {
                            return Some(m.as_str().to_string());
                        }
                    }
                }
                None
            },
        )?,
    )?;
    let shared = state.env.clone();
    let s1 = shared.clone();
    g.set(
        "setEnv",
        Function::new(c.clone(), move |k: String, v: rquickjs::Coerced<String>| {
            if let Ok(mut m) = s1.lock() {
                m.insert(k, v.0);
            }
        })?,
    )?;
    g.set(
        "getEnv",
        Function::new(c.clone(), move |k: String| {
            shared
                .lock()
                .ok()
                .and_then(|m| m.get(&k).cloned())
                .or_else(|| std::env::var(&k).ok())
        })?,
    )?;
    if parse_tags {
        let tag_src = Rc::new(engine::ItemTags {
            info: sc.tags.info.clone(),
            stem: sc.tags.stem.clone(),
            ext: sc.tags.ext.clone(),
            counter: sc.tags.counter,
            removed: sc.tags.removed.clone(),
            clipboard: sc.tags.clipboard.clone(),
            date_format: sc.tags.date_format.clone(),
            numfiles: sc.tags.numfiles,
        });
        g.set(
            "parseTags",
            Function::new(c.clone(), move |t: String| {
                tags::expand(&t, tag_src.as_ref())
            })?,
        )?;
    }
    let dir = state.scripts_dir.lock().ok().and_then(|d| d.clone());
    for (fname, strict) in [("include", false), ("require", true)] {
        let dir = dir.clone();
        g.set(
            fname,
            Function::new(
                c.clone(),
                move |cx: Ctx<'js>, file: String| -> rquickjs::Result<()> {
                    let path = match &dir {
                        Some(d) => d.join(&file),
                        None => PathBuf::from(&file),
                    };
                    match std::fs::read_to_string(&path) {
                        Ok(src) => cx.eval::<(), _>(src),
                        Err(_) if !strict => Ok(()),
                        Err(_) => Err(rquickjs::Exception::throw_message(
                            &cx,
                            &format!("require: cannot read {file}"),
                        )),
                    }
                },
            )?,
        )?;
    }
    Ok(())
}

impl ScriptHost for QuickJs<'_> {
    fn run(&self, code: &str, sc: ScriptCtx) -> Result<ScriptOut, String> {
        let name0 = sc.name.clone();
        let ext0 = sc.ext.clone();
        let loc0 = sc.location.clone();
        let mut raw = String::new();
        let alerts = self.with_ctx(sc, true, |c| {
            c.eval::<(), _>(code).map_err(|e| js_error(c, e))?;
            raw = c
                .eval::<String, _>(
                    "JSON.stringify({ name: newName == null ? null : String(newName), ext: newExt == null ? null : String(newExt), \
                     loc: newLocation == null ? null : String(newLocation), \
                     created: (newCreated instanceof Date) ? newCreated.getTime() : null, \
                     modified: (newModified instanceof Date) ? newModified.getTime() : null })",
                )
                .map_err(|e| js_error(c, e))?;
            Ok(())
        })?;
        let v: serde_json::Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
        let s = |k: &str| v.get(k).and_then(|x| x.as_str()).map(str::to_string);
        let t = |k: &str| {
            v.get(k)
                .and_then(serde_json::Value::as_f64)
                .and_then(|ms| chrono::DateTime::from_timestamp_millis(ms as i64))
                .map(|d| d.with_timezone(&chrono::Local).naive_local())
        };
        Ok(ScriptOut {
            name: s("name").filter(|n| *n != name0),
            ext: s("ext").filter(|e| *e != ext0),
            location: s("loc").filter(|l| *l != loc0),
            created: t("created"),
            modified: t("modified"),
            alerts,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{Env, InfoCache, Item, Options, Rule};
    use std::collections::HashSet;

    fn run_script(code: &str, file: &str, opts: Options) -> Result<String, String> {
        let state = ScriptState::default();
        let host = QuickJs::new(&state).unwrap();
        let cache = InfoCache::default();
        let trace = HashSet::new();
        let env = Env {
            opts: &opts,
            cache: &cache,
            script: Some(&host),
            trace: &trace,
        };
        let items = vec![Item {
            path: format!("/nonexistent/Folder/{file}"),
            is_dir: false,
            root: String::new(),
        }];
        let p = engine::preview(&items, &[Rule::Script { code: code.into() }], &env);
        let item = &p.items[0];
        match &item.message {
            Some(m) if item.status == engine::Status::Invalid => Err(m.clone()),
            _ => Ok(item.new_name.clone()),
        }
    }

    #[test]
    fn script_sets_new_name_and_extension() {
        assert_eq!(
            run_script(
                "newName = name.toUpperCase()",
                "a b.txt",
                Options::default()
            )
            .unwrap(),
            "A B.txt"
        );
        assert_eq!(
            run_script("newExt = 'md'", "a.txt", Options::default()).unwrap(),
            "a.md"
        );
        assert_eq!(
            run_script(
                "newName = object('container') + '_' + counter",
                "x.txt",
                Options::default()
            )
            .unwrap(),
            "Folder_1.txt"
        );
        assert_eq!(
            run_script(
                "newName = regEx(name, '(.*)-(.*)', '\\\\2\\\\1')",
                "ab-cd.txt",
                Options::default()
            )
            .unwrap(),
            "cdab.txt"
        );
        assert_eq!(
            run_script(
                "newName = padNum(42, 5) + removeExt('f.txt')",
                "x.txt",
                Options::default()
            )
            .unwrap(),
            "00042f.txt"
        );
        assert_eq!(
            run_script(
                "newName = formatDateTime(new Date(2025, 6, 9), 'YYYY-MM-DD')",
                "x.txt",
                Options::default()
            )
            .unwrap(),
            "2025-07-09.txt"
        );
        assert_eq!(
            run_script(
                "newName = parseTags('<name{upper}>')",
                "abc.txt",
                Options::default()
            )
            .unwrap(),
            "ABC.txt"
        );
    }

    #[test]
    fn ask_once_uses_answers_or_default() {
        let mut opts = Options::default();
        assert_eq!(
            run_script(
                "newName = askOnce('Prefix?', 'IMG_') + name",
                "a.txt",
                opts.clone()
            )
            .unwrap(),
            "IMG_a.txt"
        );
        opts.script_answers.insert("Prefix?".into(), "P_".into());
        assert_eq!(
            run_script("newName = askOnce('Prefix?', 'IMG_') + name", "a.txt", opts).unwrap(),
            "P_a.txt"
        );
    }

    #[test]
    fn errors_and_runaway_scripts_are_reported() {
        let e = run_script("throw new Error('boom')", "a.txt", Options::default()).unwrap_err();
        assert!(e.contains("boom"), "{e}");
        let e = run_script("while(true){}", "a.txt", Options::default()).unwrap_err();
        assert!(e.contains("3 seconds") || e.contains("interrupted"), "{e}");
    }

    #[test]
    fn env_values_persist_between_runs() {
        let state = ScriptState::default();
        let info = Arc::new(FileInfo::virtual_file(Path::new("/x/a.txt"), false));
        QuickJs::new(&state)
            .unwrap()
            .condition("setEnv('K', 'v'); true", info.clone(), 1)
            .unwrap();
        assert!(QuickJs::new(&state)
            .unwrap()
            .condition("getEnv('K') == 'v'", info, 1)
            .unwrap());
    }

    #[test]
    fn conditions_filter() {
        let state = ScriptState::default();
        let host = QuickJs::new(&state).unwrap();
        let info = Arc::new(FileInfo::virtual_file(Path::new("/x/report.pdf"), false));
        assert!(host.condition("ext == 'pdf'", info.clone(), 1).unwrap());
        assert!(!host
            .condition("var s = object('size'); s != 0;", info, 1)
            .unwrap());
    }
}
