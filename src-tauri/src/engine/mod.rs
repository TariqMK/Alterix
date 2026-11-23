//! Rename engine: runs the ordered pipeline of rename steps over a list of files and
//! checks every result for invalid names and collisions. It never changes the disk;
//! it only reads file facts and metadata.

pub mod dates;
pub mod regexes;
pub mod tags;
pub mod text;

use crate::meta::FileInfo;
use chrono::{Duration, Local, NaiveDateTime};
use dates::DatePreset;
use regexes::RegexMode;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};
use tags::{TagSource, Val};
use text::CaseMode;

// ======================================================================= rule types

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Position {
    #[default]
    Prefix,
    Suffix,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NameMode {
    Keep,
    Remove,
    Fixed,
    Reverse,
    PadNumbers,
    ReformatDate,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ReplacePair {
    pub find: String,
    pub with: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum CropMode {
    #[default]
    None,
    Before,
    After,
    Special,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum LeadDots {
    #[default]
    None,
    Single,
    Double,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MoveAction {
    CopyFirst,
    CopyLast,
    MoveFirst,
    MoveLast,
    CopyRange,
    MoveRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Target {
    Start,
    End,
    Position,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DateSource {
    Created,
    Modified,
    Accessed,
    Current,
    TakenOriginal,
    TakenDigitized,
    TakenModified,
    TakenRecent,
    ItemDate,
    CreatedNew,
    ModifiedNew,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NumPosition {
    Prefix,
    Suffix,
    Both,
    Insert,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NumType {
    Decimal,
    Base,
    LettersLower,
    LettersUpper,
    Roman,
    Hebrew,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum LetterCase {
    #[default]
    Default,
    Lower,
    Upper,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExtMode {
    Same,
    Lower,
    Upper,
    Title,
    Fixed,
    Extra,
    Remove,
}

fn default_true() -> bool {
    true
}

/// One step in the rename pipeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Rule {
    /// Regular expression
    #[serde(rename_all = "camelCase")]
    Regex {
        pattern: String,
        replace: String,
        #[serde(default)]
        include_ext: bool,
        mode: RegexMode,
    },
    /// Name
    Name {
        mode: NameMode,
        #[serde(default)]
        value: String,
    },
    /// Replace text
    #[serde(rename_all = "camelCase")]
    Replace {
        pairs: Vec<ReplacePair>,
        #[serde(default)]
        match_case: bool,
    },
    /// Change case
    Case {
        mode: CaseMode,
        #[serde(default)]
        exceptions: String,
    },
    /// Remove
    #[serde(rename_all = "camelCase")]
    Remove {
        #[serde(default)]
        first: usize,
        #[serde(default)]
        last: usize,
        #[serde(default)]
        from: i64,
        #[serde(default)]
        to: i64,
        #[serde(default)]
        chars: String,
        #[serde(default)]
        words: String,
        #[serde(default)]
        crop: CropMode,
        #[serde(default)]
        crop_text: String,
        #[serde(default)]
        digits: bool,
        #[serde(default)]
        high: bool,
        #[serde(default)]
        trim: bool,
        #[serde(default)]
        double_spaces: bool,
        #[serde(default)]
        accents: bool,
        #[serde(default)]
        all_chars: bool,
        #[serde(default)]
        symbols: bool,
        #[serde(default)]
        lead_dots: LeadDots,
    },
    /// Move or copy part
    #[serde(rename_all = "camelCase")]
    MoveCopy {
        action: MoveAction,
        #[serde(default)]
        count: usize,
        #[serde(default)]
        from: i64,
        #[serde(default)]
        to: i64,
        target: Target,
        #[serde(default)]
        position: i64,
        #[serde(default)]
        separator: String,
    },
    /// Add
    #[serde(rename_all = "camelCase")]
    Add {
        #[serde(default)]
        prefix: String,
        #[serde(default)]
        insert: String,
        #[serde(default)]
        at: i64,
        #[serde(default)]
        suffix: String,
        #[serde(default)]
        word_space: bool,
    },
    /// Date
    #[serde(rename_all = "camelCase")]
    AutoDate {
        position: Position,
        source: DateSource,
        preset: DatePreset,
        #[serde(default)]
        custom: String,
        #[serde(default)]
        separator: String,
        #[serde(default)]
        segment: String,
        #[serde(default = "default_true")]
        century: bool,
        #[serde(default)]
        offset_hours: f64,
    },
    /// Folder name
    #[serde(rename_all = "camelCase")]
    FolderName {
        position: Position,
        #[serde(default)]
        separator: String,
        levels: i64,
    },
    /// Numbers
    #[serde(rename_all = "camelCase")]
    Numbering {
        position: NumPosition,
        #[serde(default)]
        at: i64,
        start: i64,
        step: i64,
        #[serde(default)]
        pad: usize,
        #[serde(default)]
        separator: String,
        #[serde(default)]
        break_at: usize,
        #[serde(default)]
        per_folder: bool,
        num_type: NumType,
        #[serde(default)]
        base: u32,
        #[serde(default)]
        letter_case: LetterCase,
    },
    /// Extension
    Extension {
        mode: ExtMode,
        #[serde(default)]
        value: String,
    },
    /// Character translations
    Translate { rules: String },
    /// JavaScript renaming
    Script { code: String },
    /// Name segment: later steps only change this part of the name.
    Segment {
        #[serde(default)]
        from: String,
        #[serde(default)]
        to: String,
    },
}

impl Rule {
    pub fn label(&self) -> &'static str {
        match self {
            Rule::Regex { .. } => "Regular expression",
            Rule::Name { .. } => "Name",
            Rule::Replace { .. } => "Replace",
            Rule::Case { .. } => "Case",
            Rule::Remove { .. } => "Remove",
            Rule::MoveCopy { .. } => "Move or copy",
            Rule::Add { .. } => "Add",
            Rule::AutoDate { .. } => "Date",
            Rule::FolderName { .. } => "Folder name",
            Rule::Numbering { .. } => "Numbers",
            Rule::Extension { .. } => "Extension",
            Rule::Translate { .. } => "Character translations",
            Rule::Script { .. } => "Script",
            Rule::Segment { .. } => "Name segment",
        }
    }
}

// ======================================================================= options

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum StampMode {
    #[default]
    Keep,
    Set,
    Current,
    FromCreated,
    FromModified,
    FromTaken,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct StampSet {
    pub mode: StampMode,
    /// "YYYY-MM-DD HH:MM:SS" when mode is Set.
    pub value: String,
    /// "[+|-]days HH:MM:SS", added after the mode is applied.
    pub delta: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Timestamps {
    pub created: StampSet,
    pub modified: StampSet,
    pub accessed: StampSet,
    /// Seconds added per file, in list order.
    pub increment_seconds: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Tri {
    #[default]
    Keep,
    Set,
    Clear,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Attributes {
    pub read_only: Tri,
    pub hidden: Tri,
    pub system: Tri,
    pub archive: Tri,
}

/// Settings that apply to the whole batch rather than one step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Options {
    pub extensions_in_name: bool,
    pub folder_extensions_in_name: bool,
    pub prevent_duplicates: bool,
    pub duplicate_format: String,
    pub duplicate_start_one: bool,
    pub duplicate_across_folders: bool,
    pub link_master: String,
    pub link_linked: String,
    pub link_across_folders: bool,
    pub allow_folders_in_names: bool,
    pub relax_validation: bool,
    pub overwrite: bool,
    pub skip_existing: bool,
    pub new_location: String,
    pub copy: bool,
    pub keep_structure: bool,
    pub timestamps: Timestamps,
    pub attributes: Attributes,
    pub size_unit: String,
    /// Clipboard text, read by the UI, for <clip> and clipboard().
    pub clipboard: String,
    /// Imported rename pairs: old name or full path → new name.
    pub pairs: Vec<ReplacePair>,
    /// Names set directly (from the clipboard list), keyed by full path.
    pub overrides: HashMap<String, String>,
    /// Answers to askOnce() prompts in scripts.
    pub script_answers: HashMap<String, String>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            extensions_in_name: false,
            folder_extensions_in_name: false,
            prevent_duplicates: false,
            duplicate_format: "%name%_%count%".into(),
            duplicate_start_one: false,
            duplicate_across_folders: false,
            link_master: String::new(),
            link_linked: String::new(),
            link_across_folders: false,
            allow_folders_in_names: false,
            relax_validation: false,
            overwrite: false,
            skip_existing: false,
            new_location: String::new(),
            copy: true,
            keep_structure: false,
            timestamps: Timestamps::default(),
            attributes: Attributes::default(),
            size_unit: "auto".into(),
            clipboard: String::new(),
            pairs: Vec::new(),
            overrides: HashMap::new(),
            script_answers: HashMap::new(),
        }
    }
}

// ======================================================================= input / output

/// A file or folder in the list, in list order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub path: String,
    #[serde(default)]
    pub is_dir: bool,
    /// The folder the item was listed from, for "keep folder structure".
    #[serde(default)]
    pub root: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    Unchanged,
    Changed,
    Conflict,
    Invalid,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceStep {
    pub step: String,
    pub name: String,
}

/// The planned result for one item.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewItem {
    pub path: String,
    pub original: String,
    pub new_name: String,
    /// Full destination path (differs from the folder when a new location is set).
    pub new_path: String,
    pub status: Status,
    pub message: Option<String>,
    pub new_created: Option<String>,
    pub new_modified: Option<String>,
    pub new_accessed: Option<String>,
    pub trace: Option<Vec<TraceStep>>,
    pub alerts: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub items: Vec<PreviewItem>,
    /// Next start value for each Numbers step (index in the pipeline), for "Retain autonumber".
    pub next_numbers: Vec<(usize, i64)>,
}

// ======================================================================= shared caches

/// Caches file information between previews so metadata is read once.
#[derive(Default)]
pub struct InfoCache {
    files: Mutex<HashMap<PathBuf, Arc<FileInfo>>>,
    folder_counts: Mutex<HashMap<PathBuf, usize>>,
}

impl InfoCache {
    pub fn get(&self, path: &Path, is_dir: bool) -> Arc<FileInfo> {
        if let Some(i) = self.files.lock().unwrap().get(path) {
            return i.clone();
        }
        let info =
            Arc::new(FileInfo::load(path).unwrap_or_else(|_| FileInfo::virtual_file(path, is_dir)));
        self.files
            .lock()
            .unwrap()
            .insert(path.to_path_buf(), info.clone());
        info
    }

    pub fn forget(&self, path: &Path) {
        self.files.lock().unwrap().remove(path);
        if let Some(parent) = path.parent() {
            self.folder_counts.lock().unwrap().remove(parent);
        }
    }

    pub fn clear(&self) {
        self.files.lock().unwrap().clear();
        self.folder_counts.lock().unwrap().clear();
    }

    /// Number of visible files in `dir`.
    pub fn count_files(&self, dir: &Path) -> usize {
        if let Some(n) = self.folder_counts.lock().unwrap().get(dir) {
            return *n;
        }
        let n = std::fs::read_dir(dir)
            .map(|rd| {
                rd.filter_map(Result::ok)
                    .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
                    .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
                    .count()
            })
            .unwrap_or(0);
        self.folder_counts
            .lock()
            .unwrap()
            .insert(dir.to_path_buf(), n);
        n
    }
}

/// Runs a script step. Implemented by the script module so the engine stays engine-only.
pub trait ScriptHost {
    fn run(&self, code: &str, ctx: ScriptCtx) -> Result<ScriptOut, String>;
}

/// What a script can read about the current item.
pub struct ScriptCtx {
    pub info: Arc<FileInfo>,
    pub name: String,
    pub ext: String,
    pub orig_name: String,
    pub orig_ext: String,
    pub location: String,
    pub sub_dir: String,
    pub counter: usize,
    pub clipboard: String,
    pub answers: HashMap<String, String>,
    pub autonumber: Option<String>,
    pub autodate: Option<String>,
    pub tags: ItemTags,
}

#[derive(Debug, Default, Clone)]
pub struct ScriptOut {
    pub name: Option<String>,
    pub ext: Option<String>,
    pub location: Option<String>,
    pub created: Option<NaiveDateTime>,
    pub modified: Option<NaiveDateTime>,
    pub alerts: Vec<String>,
}

// ======================================================================= helpers

/// Splits a file name into (stem, extension including the dot).
/// A leading dot is part of the stem, so ".bashrc" has no extension.
pub fn split_name(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    }
}

/// Characters that are not allowed in file names on at least one supported system.
const FORBIDDEN_CHARS: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

/// Names Windows reserves for devices, with or without an extension.
const RESERVED_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Returns why `name` is not a safe file name, if it isn't. With `relaxed`, only the
/// rules of the current operating system apply.
pub fn validate_name(name: &str, relaxed: bool) -> Option<String> {
    if name.is_empty() {
        return Some("The new name is empty.".into());
    }
    if name == "." || name == ".." {
        return Some("\".\" and \"..\" cannot be used as names.".into());
    }
    let forbidden: &[char] = if relaxed && !cfg!(windows) {
        &['/']
    } else {
        FORBIDDEN_CHARS
    };
    if let Some(c) = name
        .chars()
        .find(|c| forbidden.contains(c) || *c == '\0' || (!relaxed && c.is_control()))
    {
        return Some(format!(
            "The name contains \"{}\", which is not allowed on all systems.",
            c.escape_default()
        ));
    }
    if relaxed && !cfg!(windows) {
        return None;
    }
    if name.ends_with(' ') || name.ends_with('.') {
        return Some("The name ends with a space or a dot, which Windows does not allow.".into());
    }
    let (stem, _) = split_name(name);
    if RESERVED_NAMES
        .iter()
        .any(|r| r.eq_ignore_ascii_case(stem.trim_end()))
    {
        return Some(format!("\"{stem}\" is a reserved name on Windows."));
    }
    if name.chars().count() > 255 {
        return Some("The name is longer than 255 characters.".into());
    }
    None
}

pub fn human_size(bytes: u64, unit: &str) -> String {
    let b = bytes as f64;
    let fmt = |v: f64, u: &str| {
        if v >= 100.0 || v.fract() == 0.0 {
            format!("{v:.0} {u}")
        } else {
            format!("{v:.1} {u}")
        }
    };
    match unit.to_ascii_lowercase().as_str() {
        "bytes" | "b" => format!("{bytes} bytes"),
        "kb" => fmt(b / 1024.0, "KB"),
        "mb" => fmt(b / 1048576.0, "MB"),
        "gb" => fmt(b / 1073741824.0, "GB"),
        _ => {
            if b < 1024.0 {
                format!("{bytes} bytes")
            } else if b < 1048576.0 {
                fmt(b / 1024.0, "KB")
            } else if b < 1073741824.0 {
                fmt(b / 1048576.0, "MB")
            } else {
                fmt(b / 1073741824.0, "GB")
            }
        }
    }
}

fn parse_stamp(s: &str) -> Option<NaiveDateTime> {
    crate::meta::parse_date(s)
}

/// Parses "[+|-]days HH:MM:SS" (each part optional) into a duration.
pub fn parse_delta(s: &str) -> Option<Duration> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let (neg, body) = match s.strip_prefix('-') {
        Some(b) => (true, b),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    let mut total = Duration::zero();
    for part in body.split_whitespace() {
        if part.contains(':') {
            let nums: Vec<i64> = part.split(':').filter_map(|p| p.parse().ok()).collect();
            let (h, m, sec) = match nums.as_slice() {
                [h, m, s] => (*h, *m, *s),
                [h, m] => (*h, *m, 0),
                _ => return None,
            };
            total += Duration::hours(h) + Duration::minutes(m) + Duration::seconds(sec);
        } else {
            total += Duration::days(part.parse().ok()?);
        }
    }
    Some(if neg { -total } else { total })
}

/// New created / modified / accessed times for the item at `index`, or None to keep.
pub fn new_timestamps(
    info: &FileInfo,
    index: usize,
    ts: &Timestamps,
) -> [Option<NaiveDateTime>; 3] {
    let now = Local::now().naive_local();
    let created_base = compute_stamp(&ts.created, info.created, info, now);
    let pick = |set: &StampSet, own: Option<NaiveDateTime>| match set.mode {
        StampMode::FromCreated => created_base.or(info.created),
        _ => compute_stamp(set, own, info, now),
    };
    let mut out = [
        created_base,
        pick(&ts.modified, info.modified),
        pick(&ts.accessed, info.accessed),
    ];
    let inc = Duration::seconds(ts.increment_seconds.saturating_mul(index as i64));
    for (slot, set) in out
        .iter_mut()
        .zip([&ts.created, &ts.modified, &ts.accessed])
    {
        if let Some(t) = slot {
            if let Some(d) = parse_delta(&set.delta) {
                *t += d;
            }
            if ts.increment_seconds != 0 && set.mode != StampMode::Keep {
                *t += inc;
            }
        }
    }
    out
}

fn compute_stamp(
    set: &StampSet,
    own: Option<NaiveDateTime>,
    info: &FileInfo,
    now: NaiveDateTime,
) -> Option<NaiveDateTime> {
    match set.mode {
        StampMode::Keep => {
            if parse_delta(&set.delta).is_some() {
                own
            } else {
                None
            }
        }
        StampMode::Set => parse_stamp(&set.value),
        StampMode::Current => Some(now),
        StampMode::FromCreated => info.created,
        StampMode::FromModified => info.modified,
        StampMode::FromTaken => info.date_taken("original").or_else(|| info.item_date()),
    }
}

// ======================================================================= pipeline work

/// The name as it moves through the pipeline.
struct Work {
    stem: String,
    ext: String,
    removed: Vec<String>,
    segment: Option<(String, String)>,
    location: Option<String>,
    created: Option<NaiveDateTime>,
    modified: Option<NaiveDateTime>,
    alerts: Vec<String>,
}

impl Work {
    /// Applies `f` to the stem, or only to the active name segment.
    fn edit(&mut self, f: impl FnOnce(&str) -> String) {
        match &self.segment {
            None => self.stem = f(&self.stem),
            Some((from, to)) => {
                let (a, b) = segment_range(&self.stem, from, to);
                let c: Vec<char> = self.stem.chars().collect();
                let pre: String = c[..a].iter().collect();
                let mid: String = c[a..b].iter().collect();
                let post: String = c[b..].iter().collect();
                self.stem = format!("{pre}{}{post}", f(&mid));
            }
        }
    }
}

/// Character range [a, b) selected by a Name Segment From / To pair. Each value is a
/// position (negative counts from the end) or `text[,n]` for the n-th occurrence of text.
/// Text boundaries are excluded from the segment.
fn segment_range(s: &str, from: &str, to: &str) -> (usize, usize) {
    let c: Vec<char> = s.chars().collect();
    let n = c.len();
    let find_text = |spec: &str, start: usize| -> Option<(usize, usize)> {
        let (txt, occ) = match spec.rsplit_once(',') {
            Some((t, o)) if !t.is_empty() && o.trim().parse::<usize>().is_ok() => {
                (t, o.trim().parse::<usize>().unwrap())
            }
            _ => (spec, 1),
        };
        let t: Vec<char> = txt.chars().collect();
        if t.is_empty() {
            return None;
        }
        let mut found = 0;
        let mut i = start;
        while i + t.len() <= n {
            if c[i..i + t.len()] == t[..] {
                found += 1;
                if found == occ {
                    return Some((i, i + t.len()));
                }
            }
            i += 1;
        }
        None
    };
    let as_pos = |spec: &str| spec.trim().parse::<i64>().ok();
    let a = if from.is_empty() {
        0
    } else if let Some(p) = as_pos(from) {
        text::pos_to_index(p, n)
    } else {
        match find_text(from, 0) {
            Some((_, end)) => end,
            None => return (n, n),
        }
    };
    let b = if to.is_empty() {
        n
    } else if let Some(p) = as_pos(to) {
        (text::pos_to_index(p, n) + 1).min(n)
    } else {
        let (txt, occ) = match to.rsplit_once(',') {
            Some((t, o)) if o.trim().parse::<usize>().is_ok() => {
                (t.to_string(), o.trim().parse::<usize>().unwrap())
            }
            _ => (to.to_string(), 1),
        };
        // Same delimiter on both sides: count occurrences from the start of the name.
        let same = !from.is_empty()
            && as_pos(from).is_none()
            && from.split(',').next() == Some(txt.as_str());
        let spec = format!("{txt},{}", if same && occ == 1 { 2 } else { occ });
        match find_text(&spec, 0) {
            Some((start, _)) => start,
            None => n,
        }
    };
    (a.min(n), b.clamp(a.min(n), n))
}

#[derive(Default)]
struct NumState {
    count: i64,
    folder: Option<PathBuf>,
    prev_char: Option<Option<char>>,
}

/// Tag values for one item at one point in the pipeline. Owns its data so scripts can keep it.
pub struct ItemTags {
    pub info: Arc<FileInfo>,
    pub stem: String,
    pub ext: String,
    pub counter: usize,
    pub removed: Vec<String>,
    pub clipboard: String,
    pub date_format: String,
    pub numfiles: usize,
}

fn random_string(len: usize, alphabet: &[u8]) -> String {
    (0..len)
        .map(|_| alphabet[(rand::random::<u32>() as usize) % alphabet.len()] as char)
        .collect()
}

impl TagSource for ItemTags {
    fn simple(&self, tag: &str, arg: Option<&str>) -> Option<Val> {
        let len = |d: usize| {
            arg.and_then(|a| a.trim().parse::<usize>().ok())
                .unwrap_or(d)
                .clamp(1, 1000)
        };
        Some(match tag {
            "name" => Val::Text(self.stem.clone()),
            "ext" => Val::Text(self.ext.trim_start_matches('.').to_string()),
            "counter" => Val::Num(self.counter as f64),
            "size" => Val::Num(self.info.size as f64),
            "folder" => Val::Text(
                self.info
                    .path
                    .parent()
                    .and_then(Path::file_name)
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            ),
            "numfiles" => Val::Num(self.numfiles as f64),
            "ulid" => Val::Text(ulid::Ulid::generate().to_string()),
            "uuid" => Val::Text(uuid::Uuid::new_v4().to_string()),
            "clip" | "clipboard" => {
                Val::Text(self.clipboard.lines().next().unwrap_or("").to_string())
            }
            "removed" => Val::Text(self.removed.concat()),
            "randnum" => Val::Text(random_string(len(6), b"0123456789")),
            "randchar" => Val::Text(random_string(
                len(6),
                b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ",
            )),
            "rand" => Val::Text(random_string(
                len(6),
                b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ",
            )),
            "randname" => {
                let (lo, hi) = match arg.and_then(|a| a.split_once('-')) {
                    Some((a, b)) => (
                        a.trim().parse().unwrap_or(10),
                        b.trim().parse().unwrap_or(10),
                    ),
                    None => {
                        let n = arg.and_then(|a| a.trim().parse().ok()).unwrap_or(10);
                        (n, n)
                    }
                };
                let (lo, hi): (usize, usize) =
                    (lo.clamp(1, 255), hi.clamp(1, 255).max(lo.clamp(1, 255)));
                let n = lo + (rand::random::<u32>() as usize) % (hi - lo + 1);
                let alphabet: Vec<u8> = (b'0'..=b'9')
                    .chain(b'A'..=b'Z')
                    .chain(b'a'..=b'z')
                    .chain(b"-_#@!~+=()[]{}".iter().copied())
                    .collect();
                Val::Text(random_string(n, &alphabet))
            }
            _ => return None,
        })
    }

    fn property(&self, name: &str) -> Option<String> {
        self.info.property(name)
    }

    fn date_format(&self) -> String {
        self.date_format.clone()
    }
}

/// Everything a preview needs besides the rules and items.
pub struct Env<'a> {
    pub opts: &'a Options,
    pub cache: &'a InfoCache,
    pub script: Option<&'a dyn ScriptHost>,
    /// Paths whose step-by-step trace is wanted ("Debug new name").
    pub trace: &'a HashSet<String>,
}

// ======================================================================= rule application

fn first_date_format(rules: &[Rule]) -> String {
    for r in rules {
        if let Rule::AutoDate {
            preset,
            custom,
            segment,
            century,
            ..
        } = r
        {
            return if *preset == DatePreset::Custom {
                custom.clone()
            } else {
                dates::preset_format(*preset, segment, *century)
            };
        }
    }
    "%Y-%m-%d".into()
}

fn item_tags(
    w: &Work,
    info: &Arc<FileInfo>,
    index: usize,
    env: &Env,
    date_fmt: &str,
    need_count: bool,
) -> ItemTags {
    ItemTags {
        info: info.clone(),
        stem: w.stem.clone(),
        ext: w.ext.clone(),
        counter: index + 1,
        removed: w.removed.clone(),
        clipboard: env.opts.clipboard.clone(),
        date_format: date_fmt.to_string(),
        numfiles: if need_count {
            info.path
                .parent()
                .map(|p| env.cache.count_files(p))
                .unwrap_or(0)
        } else {
            0
        },
    }
}

fn expand_text(
    s: &str,
    w: &Work,
    info: &Arc<FileInfo>,
    index: usize,
    env: &Env,
    date_fmt: &str,
    legacy: bool,
) -> String {
    if !s.contains('<') && !(legacy && s.contains('%')) {
        return s.to_string();
    }
    let src = item_tags(w, info, index, env, date_fmt, s.contains("numfiles"));
    let s = if legacy {
        tags::expand_legacy(s, &src, &human_size(info.size, &env.opts.size_unit))
    } else {
        s.to_string()
    };
    tags::expand(&s, &src)
}

/// Replace: one find/with pair with |, \first\, wildcard, quote and \regex\ syntax.
fn replace_pair(name: &str, find: &str, with: &str, match_case: bool) -> Result<String, String> {
    if find.is_empty() {
        return Ok(name.to_string());
    }
    if let Some(pattern) = find.strip_prefix("\\regex\\") {
        let re = fancy_regex::RegexBuilder::new(pattern)
            .case_insensitive(!match_case)
            .build()
            .map_err(|e| format!("Invalid regular expression: {e}"))?;
        return Ok(re.replace_all(name, with).into_owned());
    }
    // Position modifier.
    const POS: &[&str] = &[
        "first", "last", "start", "end", "second", "third", "fourth", "fifth", "sixth", "seventh",
        "eighth", "ninth",
    ];
    let mut which: Option<&str> = None;
    let mut body = find;
    for p in POS {
        let tag = format!("\\{p}\\");
        if let Some(rest) = find.strip_prefix(tag.as_str()) {
            which = Some(p);
            body = rest;
            break;
        }
    }
    let split = |s: &str| -> Vec<String> {
        let mut out = Vec::new();
        let mut cur = String::new();
        let mut chars = s.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\\' && chars.peek() == Some(&'|') {
                cur.push('|');
                chars.next();
            } else if c == '|' {
                out.push(std::mem::take(&mut cur));
            } else {
                cur.push(c);
            }
        }
        out.push(cur);
        out
    };
    let finds = split(body);
    let withs = split(with);
    let mut out = name.to_string();
    for (i, f) in finds.iter().enumerate() {
        if f.is_empty() {
            continue;
        }
        let w = withs
            .get(i)
            .or_else(|| withs.first())
            .cloned()
            .unwrap_or_default();
        out = replace_one(&out, f, &w, match_case, which)?;
    }
    Ok(out)
}

fn replace_one(
    name: &str,
    find: &str,
    with: &str,
    match_case: bool,
    which: Option<&str>,
) -> Result<String, String> {
    let (whole, find) = if find.len() >= 2 && find.starts_with('"') && find.ends_with('"') {
        (true, &find[1..find.len() - 1])
    } else {
        (false, find)
    };
    let mut body = if find.contains('*') || find.contains('?') {
        text::wildcard_to_regex(find)
    } else {
        fancy_regex::escape(find).into_owned()
    };
    match which {
        _ if whole => body = format!("^{body}$"),
        Some("start") => body = format!("^{body}"),
        Some("end") => body = format!("{body}$"),
        _ => {}
    }
    let re = fancy_regex::RegexBuilder::new(&body)
        .case_insensitive(!match_case)
        .build()
        .map_err(|e| format!("Invalid replace pattern: {e}"))?;
    let matches: Vec<fancy_regex::Captures<'_, str>> =
        re.captures_iter(name).filter_map(Result::ok).collect();
    if matches.is_empty() {
        return Ok(name.to_string());
    }
    let ordinal = |w: &str| match w {
        "first" | "start" => Some(0usize),
        "second" => Some(1),
        "third" => Some(2),
        "fourth" => Some(3),
        "fifth" => Some(4),
        "sixth" => Some(5),
        "seventh" => Some(6),
        "eighth" => Some(7),
        "ninth" => Some(8),
        _ => None,
    };
    let chosen: Vec<usize> = match which {
        None => (0..matches.len()).collect(),
        Some("last") | Some("end") => vec![matches.len() - 1],
        Some(w) => ordinal(w)
            .filter(|i| *i < matches.len())
            .into_iter()
            .collect(),
    };
    // Wildcards in `with` take the captured text in order.
    let fill = |caps: &fancy_regex::Captures<'_, str>| -> String {
        let mut out = String::new();
        let mut g = 1;
        for c in with.chars() {
            if (c == '*' || c == '?') && caps.get(g).is_some() {
                out.push_str(caps.get(g).map_or("", |m| m.as_str()));
                g += 1;
            } else {
                out.push(c);
            }
        }
        out
    };
    let mut out = String::new();
    let mut last = 0;
    for (i, caps) in matches.iter().enumerate() {
        if !chosen.contains(&i) {
            continue;
        }
        let m = caps.get(0).unwrap();
        out.push_str(&name[last..m.start()]);
        out.push_str(&fill(caps));
        last = m.end();
    }
    out.push_str(&name[last..]);
    Ok(out)
}

/// Crop (Remove 5): returns (new text, removed text).
fn crop(s: &str, mode: CropMode, t: &str) -> (String, String) {
    if t.is_empty() {
        return (s.to_string(), String::new());
    }
    match mode {
        CropMode::None => (s.to_string(), String::new()),
        CropMode::Before | CropMode::After => {
            let lower = s.to_lowercase();
            match lower.find(&t.to_lowercase()) {
                Some(i) if mode == CropMode::After => {
                    (s[..i + t.len()].to_string(), s[i + t.len()..].to_string())
                }
                Some(i) => (s[i..].to_string(), s[..i].to_string()),
                None => (s.to_string(), String::new()),
            }
        }
        CropMode::Special => {
            let body = text::wildcard_to_regex(t);
            let Ok(re) = fancy_regex::RegexBuilder::new(&body)
                .case_insensitive(true)
                .build()
            else {
                return (s.to_string(), String::new());
            };
            let mut removed = String::new();
            let out = if t.starts_with('*') {
                match re.find(s).ok().flatten() {
                    Some(m) => {
                        removed.push_str(m.as_str());
                        format!("{}{}", &s[..m.start()], &s[m.end()..])
                    }
                    None => s.to_string(),
                }
            } else {
                let mut out = String::new();
                let mut last = 0;
                for m in re.find_iter(s).filter_map(Result::ok) {
                    out.push_str(&s[last..m.start()]);
                    removed.push_str(m.as_str());
                    last = m.end();
                }
                out.push_str(&s[last..]);
                out
            };
            (out, removed)
        }
    }
}

fn number_text(n: i64, num_type: NumType, base: u32, pad: usize, case: LetterCase) -> String {
    let mut s = match num_type {
        NumType::Decimal => n.unsigned_abs().to_string(),
        NumType::Base => text::to_base(n.abs(), if base < 2 { 10 } else { base }),
        NumType::LettersLower => text::to_letters(n, false),
        NumType::LettersUpper => text::to_letters(n, true),
        NumType::Roman => text::to_roman(n),
        NumType::Hebrew => text::to_hebrew(n),
    };
    let fill = match num_type {
        NumType::LettersLower => 'a',
        NumType::LettersUpper => 'A',
        NumType::Roman | NumType::Hebrew => '\0',
        _ => '0',
    };
    if fill != '\0' {
        let len = s.chars().count();
        if len < pad {
            s = format!("{}{s}", fill.to_string().repeat(pad - len));
        }
    }
    if n < 0 && matches!(num_type, NumType::Decimal | NumType::Base) {
        s = format!("-{s}");
    }
    match case {
        LetterCase::Lower => s.to_lowercase(),
        LetterCase::Upper => s.to_uppercase(),
        LetterCase::Default => s,
    }
}

fn folder_names(path: &Path, levels: i64) -> String {
    let parents: Vec<String> = path
        .parent()
        .map(|p| {
            p.components()
                .filter_map(|c| match c {
                    Component::Normal(n) => Some(n.to_string_lossy().into_owned()),
                    Component::Prefix(pre) => Some(
                        pre.as_os_str()
                            .to_string_lossy()
                            .trim_end_matches([':', '\\'])
                            .to_string(),
                    ),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();
    if parents.is_empty() || levels == 0 {
        return String::new();
    }
    if levels < 0 {
        let k = levels.unsigned_abs() as usize;
        return parents
            .len()
            .checked_sub(k)
            .map(|i| parents[i].clone())
            .unwrap_or_default();
    }
    let k = (levels as usize).min(parents.len());
    parents[parents.len() - k..].join("\u{0}")
}

fn translate(s: &str, rules: &str) -> String {
    let token = |t: &str| -> Option<String> {
        let t = t.trim_matches(|c| c == '\r');
        match t.chars().count() {
            1 => Some(t.to_string()),
            2 => u32::from_str_radix(t, 16)
                .ok()
                .and_then(char::from_u32)
                .map(String::from),
            3 => t
                .parse::<u32>()
                .ok()
                .and_then(char::from_u32)
                .map(String::from),
            0 => Some(String::new()),
            _ => Some(t.to_string()),
        }
    };
    let seq = |side: &str| -> Option<String> {
        side.split(',')
            .map(token)
            .collect::<Option<Vec<_>>>()
            .map(|v| v.concat())
    };
    let mut out = s.to_string();
    for line in rules.lines() {
        let Some((from, to)) = line.split_once('=') else {
            continue;
        };
        if let (Some(f), Some(t)) = (seq(from), seq(to)) {
            if !f.is_empty() {
                out = out.replace(&f, &t);
            }
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn apply_rule(
    rule: &Rule,
    step: usize,
    w: &mut Work,
    item: &Item,
    info: &Arc<FileInfo>,
    index: usize,
    env: &Env,
    num: &mut HashMap<usize, NumState>,
    date_fmt: &str,
    orig: (&str, &str),
    autonumber: &mut Option<String>,
    autodate: &mut Option<String>,
) -> Result<(), String> {
    match rule {
        Rule::Regex {
            pattern,
            replace,
            include_ext,
            mode,
        } => {
            if pattern.is_empty() {
                return Ok(());
            }
            let rep = expand_text(replace, w, info, index, env, date_fmt, false);
            if *include_ext {
                let full = format!("{}{}", w.stem, w.ext);
                let new = regexes::apply(&full, pattern, &rep, *mode)?;
                let (s, e) = split_name(&new);
                w.stem = s.to_string();
                w.ext = e.to_string();
            } else {
                let mut err = None;
                w.edit(|s| {
                    regexes::apply(s, pattern, &rep, *mode).unwrap_or_else(|e| {
                        err = Some(e);
                        s.to_string()
                    })
                });
                if let Some(e) = err {
                    return Err(e);
                }
            }
        }
        Rule::Name { mode, value } => match mode {
            NameMode::Keep => {}
            NameMode::Remove => w.edit(|_| String::new()),
            NameMode::Fixed => {
                let v = expand_text(value, w, info, index, env, date_fmt, false);
                w.edit(|_| v);
            }
            NameMode::Reverse => w.edit(|s| s.chars().rev().collect()),
            NameMode::PadNumbers => {
                let (width, fill) = match value.split_once('>') {
                    Some((a, b)) => (
                        a.trim().parse().unwrap_or(0),
                        b.chars().next().unwrap_or('0'),
                    ),
                    None => (value.trim().parse().unwrap_or(0), '0'),
                };
                w.edit(|s| {
                    tags::apply_formatters(
                        Val::Text(s.to_string()),
                        &[format!("padnum:{width}:{fill}")],
                    )
                });
            }
            NameMode::ReformatDate => w.edit(|s| dates::reformat_date(s, value)),
        },
        Rule::Replace { pairs, match_case } => {
            for p in pairs {
                let with = expand_text(&p.with, w, info, index, env, date_fmt, false);
                let find = p
                    .find
                    .replace("<clip>", env.opts.clipboard.lines().next().unwrap_or(""));
                let mut err = None;
                w.edit(|s| {
                    replace_pair(s, &find, &with, *match_case).unwrap_or_else(|e| {
                        err = Some(e);
                        s.to_string()
                    })
                });
                if let Some(e) = err {
                    return Err(e);
                }
            }
        }
        Rule::Case { mode, exceptions } => w.edit(|s| text::change_case(s, *mode, exceptions)),
        Rule::Remove {
            first,
            last,
            from,
            to,
            chars,
            words,
            crop: crop_mode,
            crop_text,
            digits,
            high,
            trim,
            double_spaces,
            accents,
            all_chars,
            symbols,
            lead_dots,
        } => {
            let mut removed: Vec<String> = Vec::new();
            w.edit(|s| {
                let mut c: Vec<char> = s.chars().collect();
                if *first > 0 {
                    let n = (*first).min(c.len());
                    removed.push(c.drain(..n).collect());
                }
                if *last > 0 {
                    let n = (*last).min(c.len());
                    removed.push(c.drain(c.len() - n..).collect());
                }
                if *from != 0 || *to != 0 {
                    let len = c.len();
                    let a = text::pos_to_index(if *from == 0 { 1 } else { *from }, len);
                    let b = if *to == 0 {
                        len
                    } else {
                        (text::pos_to_index(*to, len) + 1).min(len)
                    };
                    if a < b {
                        removed.push(c.drain(a..b).collect());
                    }
                }
                let mut s: String = c.into_iter().collect();
                s = text::remove_chars(&s, chars);
                s = text::remove_words(&s, words);
                let (cropped, gone) = crop(&s, *crop_mode, crop_text);
                if !gone.is_empty() {
                    removed.push(gone);
                }
                s = cropped;
                if *digits {
                    s.retain(|c| !c.is_ascii_digit());
                }
                if *high {
                    s.retain(|c| !(128..=255).contains(&(c as u32)));
                }
                if *accents {
                    s = text::remove_accents(&s);
                }
                if *all_chars {
                    s.retain(|c| !c.is_alphabetic());
                }
                if *symbols {
                    s.retain(|c| !text::is_symbol(c));
                }
                s = match lead_dots {
                    LeadDots::None => s,
                    LeadDots::Single => s.strip_prefix('.').map(str::to_string).unwrap_or(s),
                    LeadDots::Double => s.strip_prefix("..").map(str::to_string).unwrap_or(s),
                    LeadDots::All => s.trim_start_matches('.').to_string(),
                };
                // Spacing clean-up runs last so it also tidies gaps left by the removals above.
                if *double_spaces {
                    s = text::collapse_spaces(&s);
                }
                if *trim {
                    s = s.trim().to_string();
                }
                s
            });
            w.removed.extend(removed);
        }
        Rule::MoveCopy {
            action,
            count,
            from,
            to,
            target,
            position,
            separator,
        } => {
            w.edit(|s| {
                let c: Vec<char> = s.chars().collect();
                let n = c.len();
                let (a, b) = match action {
                    MoveAction::CopyFirst | MoveAction::MoveFirst => (0, (*count).min(n)),
                    MoveAction::CopyLast | MoveAction::MoveLast => (n - (*count).min(n), n),
                    MoveAction::CopyRange | MoveAction::MoveRange => {
                        let a = text::pos_to_index(if *from == 0 { 1 } else { *from }, n);
                        let b = if *to == 0 {
                            n
                        } else {
                            (text::pos_to_index(*to, n) + 1).min(n)
                        };
                        (a.min(b), b)
                    }
                };
                let piece: String = c[a..b].iter().collect();
                if piece.is_empty() {
                    return s.to_string();
                }
                let moving = matches!(
                    action,
                    MoveAction::MoveFirst | MoveAction::MoveLast | MoveAction::MoveRange
                );
                let rest: String = if moving {
                    c[..a].iter().chain(c[b..].iter()).collect()
                } else {
                    s.to_string()
                };
                match target {
                    Target::Start => format!("{piece}{separator}{rest}"),
                    Target::End => format!("{rest}{separator}{piece}"),
                    Target::Position => {
                        text::insert_at(&rest, *position, &format!("{separator}{piece}"))
                    }
                }
            });
        }
        Rule::Add {
            prefix,
            insert,
            at,
            suffix,
            word_space,
        } => {
            let p = expand_text(prefix, w, info, index, env, date_fmt, true);
            let i = expand_text(insert, w, info, index, env, date_fmt, true);
            let sfx = expand_text(suffix, w, info, index, env, date_fmt, true);
            w.edit(|s| {
                let mut s = s.to_string();
                if !i.is_empty() {
                    s = text::insert_at(&s, *at, &i);
                }
                if *word_space {
                    s = text::word_space(&s);
                }
                format!("{p}{s}{sfx}")
            });
        }
        Rule::AutoDate {
            position,
            source,
            preset,
            custom,
            separator,
            segment,
            century,
            offset_hours,
        } => {
            let stamps = || new_timestamps(info, index, &env.opts.timestamps);
            let dt = match source {
                DateSource::Created => info.created,
                DateSource::Modified => info.modified,
                DateSource::Accessed => info.accessed,
                DateSource::Current => Some(Local::now().naive_local()),
                DateSource::TakenOriginal => info.date_taken("original"),
                DateSource::TakenDigitized => info.date_taken("digitized"),
                DateSource::TakenModified => info.date_taken("modified"),
                DateSource::TakenRecent => info.date_taken("recent"),
                DateSource::ItemDate => info.item_date(),
                DateSource::CreatedNew => w.created.or(stamps()[0]).or(info.created),
                DateSource::ModifiedNew => w.modified.or(stamps()[1]).or(info.modified),
            };
            if let Some(mut dt) = dt {
                dt += Duration::seconds((*offset_hours * 3600.0) as i64);
                let fmt = if *preset == DatePreset::Custom {
                    custom.clone()
                } else {
                    dates::preset_format(*preset, segment, *century)
                };
                let d = dates::custom_format(&dt, &fmt);
                *autodate = Some(d.clone());
                w.edit(|s| match position {
                    Position::Prefix => format!("{d}{separator}{s}"),
                    Position::Suffix => format!("{s}{separator}{d}"),
                });
            }
        }
        Rule::FolderName {
            position,
            separator,
            levels,
        } => {
            let f = folder_names(&info.path, *levels).replace('\u{0}', separator);
            if !f.is_empty() {
                w.edit(|s| match position {
                    Position::Prefix => format!("{f}{separator}{s}"),
                    Position::Suffix => format!("{s}{separator}{f}"),
                });
            }
        }
        Rule::Numbering {
            position,
            at,
            start,
            step: inc,
            pad,
            separator,
            break_at,
            per_folder,
            num_type,
            base,
            letter_case,
        } => {
            let st = num.entry(step).or_default();
            if *per_folder {
                let folder = info.path.parent().map(Path::to_path_buf);
                if st.folder.is_some() && st.folder != folder {
                    st.count = 0;
                }
                st.folder = folder;
            }
            if *break_at > 0 {
                let ch = w.stem.chars().nth(break_at - 1);
                if let Some(prev) = st.prev_char {
                    if prev != ch {
                        st.count = 0;
                    }
                }
                st.prev_char = Some(ch);
            }
            let n = start.saturating_add(inc.saturating_mul(st.count));
            st.count += 1;
            let digits = number_text(n, *num_type, *base, *pad, *letter_case);
            *autonumber = Some(digits.clone());
            let (num_text, sep) = if separator.contains(':') {
                (separator.replace(':', &digits), String::new())
            } else {
                (digits, separator.clone())
            };
            w.edit(|s| match position {
                NumPosition::Prefix => format!("{num_text}{sep}{s}"),
                NumPosition::Suffix => format!("{s}{sep}{num_text}"),
                NumPosition::Both => format!("{num_text}{sep}{s}{sep}{num_text}"),
                NumPosition::Insert => text::insert_at(s, *at, &num_text),
            });
        }
        Rule::Extension { mode, value } => {
            let e = w.ext.trim_start_matches('.').to_string();
            let v = value.trim_start_matches('.');
            let new = match mode {
                ExtMode::Same => e,
                ExtMode::Lower => e.to_lowercase(),
                ExtMode::Upper => e.to_uppercase(),
                ExtMode::Title => text::change_case(&e, CaseMode::Title, ""),
                ExtMode::Fixed => v.to_string(),
                ExtMode::Extra => {
                    if e.is_empty() {
                        v.to_string()
                    } else {
                        format!("{e}.{v}")
                    }
                }
                ExtMode::Remove => String::new(),
            };
            w.ext = if new.is_empty() {
                String::new()
            } else {
                format!(".{new}")
            };
        }
        Rule::Translate { rules } => w.edit(|s| translate(s, rules)),
        Rule::Segment { from, to } => {
            w.segment = if from.is_empty() && to.is_empty() {
                None
            } else {
                Some((from.clone(), to.clone()))
            };
        }
        Rule::Script { code } => {
            let Some(host) = env.script else {
                return Err("Scripts are not available.".into());
            };
            if code.trim().is_empty() {
                return Ok(());
            }
            let tag_src = item_tags(w, info, index, env, date_fmt, true);
            let ctx = ScriptCtx {
                info: info.clone(),
                name: w.stem.clone(),
                ext: w.ext.trim_start_matches('.').to_string(),
                orig_name: orig.0.to_string(),
                orig_ext: orig.1.trim_start_matches('.').to_string(),
                location: env.opts.new_location.clone(),
                sub_dir: subdir_of(item),
                counter: index + 1,
                clipboard: env.opts.clipboard.clone(),
                answers: env.opts.script_answers.clone(),
                autonumber: autonumber.clone(),
                autodate: autodate.clone(),
                tags: tag_src,
            };
            let out = host.run(code, ctx)?;
            if let Some(n) = out.name {
                w.edit(|_| n);
            }
            if let Some(e) = out.ext {
                w.ext = if e.is_empty() {
                    String::new()
                } else {
                    format!(".{}", e.trim_start_matches('.'))
                };
            }
            if out.location.is_some() {
                w.location = out.location;
            }
            if out.created.is_some() {
                w.created = out.created;
            }
            if out.modified.is_some() {
                w.modified = out.modified;
            }
            w.alerts.extend(out.alerts);
        }
    }
    Ok(())
}

pub fn subdir_of(item: &Item) -> String {
    let p = Path::new(&item.path);
    let parent = p.parent().unwrap_or(Path::new(""));
    if item.root.is_empty() {
        return String::new();
    }
    parent
        .strip_prefix(&item.root)
        .map(|r| r.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn destination_dir(item: &Item, opts: &Options, script_loc: Option<&str>) -> PathBuf {
    let src = Path::new(&item.path);
    let parent = src.parent().unwrap_or(Path::new("")).to_path_buf();
    let loc = script_loc.unwrap_or(&opts.new_location).trim();
    if loc.is_empty() {
        return parent;
    }
    let loc_path = PathBuf::from(loc.replace('\\', std::path::MAIN_SEPARATOR_STR));
    let base = if loc_path.is_absolute() {
        loc_path
    } else {
        parent.join(loc_path)
    };
    if opts.keep_structure {
        base.join(subdir_of(item))
    } else {
        base
    }
}

fn lower_key(p: &Path) -> String {
    p.to_string_lossy().to_lowercase()
}

// ======================================================================= preview

/// Builds the preview for `items` (in list order) after running `rules` in order.
pub fn preview(items: &[Item], rules: &[Rule], env: &Env) -> Preview {
    let date_fmt = first_date_format(rules);
    let mut num: HashMap<usize, NumState> = HashMap::new();
    let mut out: Vec<PreviewItem> = Vec::with_capacity(items.len());
    let pairs: HashMap<String, String> = env
        .opts
        .pairs
        .iter()
        .map(|p| (p.find.trim().to_lowercase(), p.with.trim().to_string()))
        .collect();
    let master = env
        .opts
        .link_master
        .trim()
        .trim_start_matches('.')
        .to_lowercase();
    let linked: Vec<String> = env
        .opts
        .link_linked
        .split(',')
        .map(|s| s.trim().trim_start_matches('.').to_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    let linking = !master.is_empty() && !linked.is_empty();
    let mut processed = 0usize;

    for item in items {
        let path = Path::new(&item.path);
        let info = env.cache.get(path, item.is_dir);
        let original = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let whole = if item.is_dir {
            env.opts.folder_extensions_in_name
        } else {
            env.opts.extensions_in_name
        };
        let (os, oe) = if whole {
            (original.as_str(), "")
        } else {
            split_name(&original)
        };
        let ext_l = split_name(&original)
            .1
            .trim_start_matches('.')
            .to_lowercase();
        let is_linked = linking && !item.is_dir && linked.contains(&ext_l);

        let mut w = Work {
            stem: os.to_string(),
            ext: oe.to_string(),
            removed: Vec::new(),
            segment: None,
            location: None,
            created: None,
            modified: None,
            alerts: Vec::new(),
        };
        // Imported pairs and clipboard names set the starting name.
        let pair = pairs
            .get(&lower_key(path))
            .or_else(|| pairs.get(&original.to_lowercase()))
            .cloned()
            .or_else(|| env.opts.overrides.get(&item.path).cloned());
        if let Some(n) = &pair {
            let (s, e) = if whole {
                (n.as_str(), "")
            } else {
                split_name(n)
            };
            w.stem = s.to_string();
            w.ext = e.to_string();
        }
        let wants_trace = env.trace.contains(&item.path);
        let mut trace = wants_trace.then(Vec::new);
        let mut error: Option<String> = None;
        let mut autonumber = None;
        let mut autodate = None;
        let index = processed;
        if !is_linked {
            processed += 1;
            for (step, rule) in rules.iter().enumerate() {
                if let Err(e) = apply_rule(
                    rule,
                    step,
                    &mut w,
                    item,
                    &info,
                    index,
                    env,
                    &mut num,
                    &date_fmt,
                    (os, oe),
                    &mut autonumber,
                    &mut autodate,
                ) {
                    error = Some(format!("{}: {e}", rule.label()));
                    break;
                }
                if let Some(t) = trace.as_mut() {
                    t.push(TraceStep {
                        step: format!("{}. {}", step + 1, rule.label()),
                        name: format!("{}{}", w.stem, w.ext),
                    });
                }
            }
        }
        let new_name = format!("{}{}", w.stem, w.ext);
        let dir = destination_dir(item, env.opts, w.location.as_deref());
        let stamps = new_timestamps(&info, index, &env.opts.timestamps);
        let fmt = |d: Option<NaiveDateTime>| d.map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string());
        out.push(PreviewItem {
            path: item.path.clone(),
            original: original.clone(),
            new_path: dir.join(&new_name).to_string_lossy().into_owned(),
            new_name,
            status: if error.is_some() {
                Status::Invalid
            } else {
                Status::Changed
            },
            message: error,
            new_created: fmt(w.created.or(stamps[0])),
            new_modified: fmt(w.modified.or(stamps[1])),
            new_accessed: fmt(stamps[2]),
            trace,
            alerts: w.alerts,
        });
    }

    if linking {
        link_by_extension(items, &mut out, &master, &linked, env.opts);
    }
    finish(items, &mut out, env.opts);
    let next_numbers = num
        .iter()
        .filter_map(|(step, st)| match rules.get(*step) {
            Some(Rule::Numbering {
                start, step: inc, ..
            }) => Some((*step, start + inc * st.count)),
            _ => None,
        })
        .collect();
    Preview {
        items: out,
        next_numbers,
    }
}

/// Gives linked-extension files the new name of their master file.
fn link_by_extension(
    items: &[Item],
    out: &mut [PreviewItem],
    master: &str,
    linked: &[String],
    opts: &Options,
) {
    let key = |p: &Path, across: bool| {
        let stem = split_name(
            &p.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
        )
        .0
        .to_lowercase();
        if across {
            stem
        } else {
            format!(
                "{}\u{0}{stem}",
                lower_key(p.parent().unwrap_or(Path::new("")))
            )
        }
    };
    let mut masters: HashMap<String, (String, PathBuf)> = HashMap::new();
    for (item, p) in items.iter().zip(out.iter()) {
        let path = Path::new(&item.path);
        if split_name(&p.original)
            .1
            .trim_start_matches('.')
            .eq_ignore_ascii_case(master)
        {
            let new_stem = split_name(&p.new_name).0.to_string();
            let new_dir = Path::new(&p.new_path)
                .parent()
                .unwrap_or(Path::new(""))
                .to_path_buf();
            masters.insert(key(path, opts.link_across_folders), (new_stem, new_dir));
        }
    }
    for (item, p) in items.iter().zip(out.iter_mut()) {
        let ext = split_name(&p.original).1.to_string();
        if !linked.contains(&ext.trim_start_matches('.').to_lowercase()) {
            continue;
        }
        let path = Path::new(&item.path);
        if let Some((stem, dir)) = masters.get(&key(path, opts.link_across_folders)) {
            p.new_name = format!("{stem}{ext}");
            let dir = if opts.new_location.is_empty() && !opts.link_across_folders {
                path.parent().unwrap_or(Path::new("")).to_path_buf()
            } else {
                dir.clone()
            };
            p.new_path = dir.join(&p.new_name).to_string_lossy().into_owned();
        } else {
            p.new_name = p.original.clone();
            p.new_path = item.path.clone();
        }
    }
}

/// Sets final statuses: unchanged, invalid, duplicate handling and conflicts.
fn finish(items: &[Item], out: &mut [PreviewItem], opts: &Options) {
    // Validation.
    for p in out.iter_mut() {
        if p.status == Status::Invalid {
            continue;
        }
        if p.new_path == p.path {
            p.status = Status::Unchanged;
            continue;
        }
        let parts: Vec<&str> = if opts.allow_folders_in_names {
            p.new_name.split(['/', '\\']).collect()
        } else {
            vec![p.new_name.as_str()]
        };
        if let Some(msg) = parts
            .iter()
            .find_map(|part| validate_name(part, opts.relax_validation))
        {
            p.status = Status::Invalid;
            p.message = Some(msg);
        } else if opts.allow_folders_in_names && p.new_name.contains(['/', '\\']) {
            // new_path was built as dir + raw name; rebuild it with real separators.
            let rel: PathBuf = parts.iter().collect();
            let dir = p.new_path[..p.new_path.len() - p.new_name.len()].to_string();
            p.new_path = Path::new(&dir).join(rel).to_string_lossy().into_owned();
        }
    }

    // Collisions inside the batch, ignoring case.
    let dup_key = |p: &PreviewItem| {
        if opts.duplicate_across_folders {
            Path::new(&p.new_path)
                .file_name()
                .map(|n| n.to_string_lossy().to_lowercase())
                .unwrap_or_default()
        } else {
            p.new_path.to_lowercase()
        }
    };
    let sources: HashSet<String> = items.iter().map(|i| i.path.to_lowercase()).collect();
    let mut taken: HashSet<String> = HashSet::new();
    let mut groups: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, p) in out.iter().enumerate() {
        if p.status == Status::Invalid {
            continue;
        }
        groups.entry(dup_key(p)).or_default().push(i);
        taken.insert(p.new_path.to_lowercase());
    }
    let exists_outside = |path: &str, own: &str| -> bool {
        let lower = path.to_lowercase();
        lower != own.to_lowercase() && !sources.contains(&lower) && Path::new(path).exists()
    };

    if opts.prevent_duplicates {
        let mut keys: Vec<&String> = groups.keys().collect();
        keys.sort();
        let mut renames: Vec<(usize, String)> = Vec::new();
        for k in keys {
            let idxs = &groups[k];
            let clash_first = exists_outside(&out[idxs[0]].new_path, &out[idxs[0]].path)
                && out[idxs[0]].status == Status::Changed;
            let skip = if opts.duplicate_start_one || clash_first {
                0
            } else {
                1
            };
            if idxs.len() == 1 && !clash_first {
                continue;
            }
            let mut count = 1;
            for &i in idxs.iter().skip(skip) {
                if out[i].status == Status::Unchanged && i == idxs[0] {
                    continue;
                }
                let p = &out[i];
                let (stem, ext) = split_name(&p.new_name);
                let dir = Path::new(&p.new_path)
                    .parent()
                    .unwrap_or(Path::new(""))
                    .to_path_buf();
                loop {
                    let candidate = format!(
                        "{}{ext}",
                        duplicate_name(&opts.duplicate_format, stem, count)
                    );
                    count += 1;
                    let full = dir.join(&candidate).to_string_lossy().into_owned();
                    if !taken.contains(&full.to_lowercase()) && !exists_outside(&full, &p.path) {
                        taken.insert(full.to_lowercase());
                        renames.push((i, candidate));
                        break;
                    }
                    if count > 9999 {
                        break;
                    }
                }
            }
        }
        for (i, name) in renames {
            let dir = Path::new(&out[i].new_path)
                .parent()
                .unwrap_or(Path::new(""))
                .to_path_buf();
            out[i].new_path = dir.join(&name).to_string_lossy().into_owned();
            out[i].new_name = name;
            out[i].status = if out[i].new_path == out[i].path {
                Status::Unchanged
            } else {
                Status::Changed
            };
        }
        groups.clear();
        for (i, p) in out.iter().enumerate() {
            if p.status != Status::Invalid {
                groups.entry(dup_key(p)).or_default().push(i);
            }
        }
    }

    for p in out.iter_mut() {
        if p.status != Status::Changed {
            continue;
        }
        let k = dup_key(p);
        let count = groups.get(&k).map_or(0, Vec::len);
        if count > 1 {
            p.status = Status::Conflict;
            p.message = Some("Two or more items would get this name.".into());
        } else if exists_outside(&p.new_path, &p.path) {
            if opts.skip_existing {
                p.status = Status::Skipped;
                p.message = Some("Skipped: a file with this name already exists.".into());
            } else if opts.overwrite {
                p.message = Some("The existing file will be moved to the trash.".into());
            } else {
                p.status = Status::Conflict;
                p.message = Some("A file with this name already exists in the folder.".into());
            }
        }
    }
}

/// Builds a de-duplicated name from a format such as "%name%_%count%".
pub fn duplicate_name(format: &str, stem: &str, count: usize) -> String {
    let fmt = if format.trim().is_empty() {
        "%name%_%count%"
    } else {
        format
    };
    fmt.replace("%name%", stem)
        .replace("%count0000%", &format!("{count:04}"))
        .replace("%count000%", &format!("{count:03}"))
        .replace("%count00%", &format!("{count:02}"))
        .replace("%counta%", &text::to_letters(count as i64 - 1, false))
        .replace("%countA%", &text::to_letters(count as i64 - 1, true))
        .replace("%count%", &count.to_string())
}

#[cfg(test)]
mod tests;
