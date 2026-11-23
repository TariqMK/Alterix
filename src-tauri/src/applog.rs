//! The app log: one always-on, self-trimming file, Alterix.log, in the data folder.
//! Each session starts with a header (version, build kind, system). The home folder is
//! written as ~. Logging never fails the app: write errors are ignored.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const LOG_NAME: &str = "Alterix.log";
const MAX_BYTES: u64 = 1_000_000;
const KEEP_BYTES: usize = 500_000;
const TRIM_MARK: &str = "--- older entries removed ---";
const END_MARK: &str = "Session ended";
const START_MARK: &str = "==========";

static LOG: Mutex<Option<PathBuf>> = Mutex::new(None);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Info,
    Warning,
    Error,
}

impl Level {
    fn label(self) -> &'static str {
        match self {
            Level::Info => "INFO   ",
            Level::Warning => "WARNING",
            Level::Error => "ERROR  ",
        }
    }

    pub fn parse(s: &str) -> Level {
        match s.to_ascii_lowercase().as_str() {
            "error" => Level::Error,
            "warning" | "warn" => Level::Warning,
            _ => Level::Info,
        }
    }
}

/// The running version, from Cargo.toml (set from the release tag at build time).
pub fn version() -> String {
    format!("v{}", env!("CARGO_PKG_VERSION"))
}

/// "release", "development" or "branch build", from the version text.
pub fn build_kind(version: &str) -> &'static str {
    let v = version.trim_start_matches('v');
    if v.ends_with("-dev") || v.starts_with("0.0.0") {
        "development"
    } else if v.contains('-') {
        "branch build"
    } else {
        "release"
    }
}

/// "macOS 26.1", "Windows 11", "Ubuntu 24.04" and the CPU architecture.
pub fn os_text() -> String {
    let info = os_info::get();
    let name = match info.os_type() {
        os_info::Type::Macos => "macOS".to_string(),
        os_info::Type::Windows => "Windows".to_string(),
        t => t.to_string(),
    };
    let ver = info.version().to_string();
    if ver.is_empty() || ver == "Unknown" {
        name
    } else {
        format!("{name} {ver}")
    }
}

fn home() -> Option<String> {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok()
        .filter(|h| h.len() > 1)
}

/// Replaces the home folder with ~ (both slash directions and any case on Windows).
pub fn scrub(text: &str) -> String {
    let Some(h) = home() else {
        return text.to_string();
    };
    if cfg!(windows) {
        let mut out = text.to_string();
        for variant in [h.clone(), h.replace('\\', "/")] {
            let re = fancy_regex::RegexBuilder::new(&fancy_regex::escape(&variant))
                .case_insensitive(true)
                .build();
            if let Ok(re) = re {
                out = re.replace_all(&out, "~").into_owned();
            }
        }
        out
    } else {
        text.replace(&h, "~")
    }
}

/// Keeps the newest ~500 KB when the file passes 1 MB, cut at a line boundary.
pub fn trim(path: &Path) {
    let Ok(md) = std::fs::metadata(path) else {
        return;
    };
    if md.len() <= MAX_BYTES {
        return;
    }
    if let Ok(bytes) = std::fs::read(path) {
        let tail = &bytes[bytes.len().saturating_sub(KEEP_BYTES)..];
        let start = tail
            .iter()
            .position(|&b| b == b'\n')
            .map(|i| i + 1)
            .unwrap_or(0);
        let mut out = format!("{TRIM_MARK}\n").into_bytes();
        out.extend_from_slice(&tail[start..]);
        let _ = std::fs::write(path, out);
    }
}

fn stamp() -> String {
    chrono::Local::now()
        .format("%Y-%m-%d %H:%M:%S%.3f %:z")
        .to_string()
}

fn append(path: &Path, text: &str) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = f.write_all(text.as_bytes());
    }
    if std::fs::metadata(path)
        .map(|m| m.len() > MAX_BYTES)
        .unwrap_or(false)
    {
        trim(path);
    }
}

/// Writes one entry. `area` is the part of the app, e.g. "rename".
pub fn write(level: Level, area: &str, message: &str) {
    let path = LOG.lock().ok().and_then(|g| g.clone());
    let Some(path) = path else {
        return;
    };
    let line = format!(
        "{} {} [{}] {}\n",
        stamp(),
        level.label(),
        area,
        scrub(message).replace('\n', "\n    ")
    );
    append(&path, &line);
}

pub fn info(area: &str, message: &str) {
    write(Level::Info, area, message);
}

pub fn warn(area: &str, message: &str) {
    write(Level::Warning, area, message);
}

pub fn error(area: &str, message: &str) {
    write(Level::Error, area, message);
}

/// True when the last session in `text` has no "Session ended" line.
pub fn previous_session_open(text: &str) -> bool {
    let last_start = text.rfind(START_MARK);
    match last_start {
        None => false,
        Some(i) => !text[i..].contains(END_MARK),
    }
}

/// Starts logging to `dir/Alterix.log`: trims the file, warns about an unclean previous
/// exit, writes the session header and installs a panic hook.
pub fn start(dir: &Path, data_writable: bool) {
    let path = dir.join(LOG_NAME);
    *LOG.lock().unwrap() = Some(path.clone());
    trim(&path);
    let previous_open = std::fs::read_to_string(&path)
        .map(|t| previous_session_open(&t))
        .unwrap_or(false);
    let v = version();
    let header = format!(
        "{START_MARK} {START_MARK}\n{} INFO    [app] Alterix {v} ({}, {})\n{} INFO    [app] {} {}, Tauri {}\n{} INFO    [app] Data folder: {}{}\n",
        stamp(),
        build_kind(&v),
        if cfg!(debug_assertions) { "run from source" } else { "built app" },
        stamp(),
        os_text(),
        std::env::consts::ARCH,
        tauri::VERSION,
        stamp(),
        scrub(&dir.to_string_lossy()),
        if data_writable { "" } else { " (not writable)" },
    );
    append(&path, &header);
    if previous_open {
        warn(
            "app",
            "The previous session did not close normally (crash or force quit).",
        );
    }
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current()
            .name()
            .unwrap_or("unnamed")
            .to_string();
        let bt = std::backtrace::Backtrace::force_capture();
        error(
            "panic",
            &format!("Thread '{thread}' panicked: {info}\n{bt}"),
        );
        default_hook(info);
    }));
}

/// Moves logging to another data folder (when the user picks one).
pub fn relocate(dir: &Path) {
    *LOG.lock().unwrap() = Some(dir.join(LOG_NAME));
    info("app", &format!("Log moved to {}", dir.display()));
}

pub fn end() {
    info("app", END_MARK);
}

pub fn path() -> Option<PathBuf> {
    LOG.lock().ok().and_then(|g| g.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trimming_keeps_newest_whole_lines() {
        let dir = std::env::temp_dir().join(format!("alterix-log-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("t.log");
        let line = "x".repeat(99) + "\n";
        let mut text = String::new();
        for i in 0..12_000 {
            text.push_str(&format!("{i:06} {line}"));
        }
        std::fs::write(&p, &text).unwrap();
        trim(&p);
        let out = std::fs::read_to_string(&p).unwrap();
        assert!(out.starts_with(TRIM_MARK));
        assert!(out.len() <= KEEP_BYTES + 100);
        assert!(out.ends_with(&format!("011999 {line}")));
        let second = out.lines().nth(1).unwrap();
        assert_eq!(second.len(), 106, "cut at a whole line: {second}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn home_folder_becomes_tilde() {
        let h = home().unwrap();
        assert_eq!(
            scrub(&format!("open {h}/Pictures/a.jpg")),
            "open ~/Pictures/a.jpg"
        );
    }

    #[test]
    fn build_kinds_and_unclean_exit() {
        assert_eq!(build_kind("v0.0.0-dev"), "development");
        assert_eq!(build_kind("v1.5.0-new-sync.1"), "branch build");
        assert_eq!(build_kind("v1.5.0"), "release");
        assert!(previous_session_open("========== ==========\nx\n"));
        assert!(!previous_session_open(
            "========== ==========\nx Session ended\n"
        ));
        assert!(!previous_session_open(""));
    }

    #[test]
    fn header_contains_version_and_system() {
        let dir = std::env::temp_dir().join(format!("alterix-log-h-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        start(&dir, true);
        info("test", "hello");
        end();
        let text = std::fs::read_to_string(dir.join(LOG_NAME)).unwrap();
        assert!(
            text.contains(&version())
                && text.contains("development")
                && text.contains(std::env::consts::ARCH)
        );
        assert!(text.contains("[test] hello") && text.contains(END_MARK));
        *LOG.lock().unwrap() = None;
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
