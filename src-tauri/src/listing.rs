//! Builds the file list: scans folders with the Filters options
//! (masks, regex, files/folders/hidden, subfolder levels, name and path lengths and a
//! JavaScript condition) and turns dropped or pasted paths into list entries.

use crate::engine::text;
use crate::meta::FileInfo;
use crate::script::QuickJs;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Filters {
    pub mask: String,
    pub match_case: bool,
    pub regex: bool,
    pub files: bool,
    pub folders: bool,
    pub hidden: bool,
    pub subfolders: bool,
    /// Level limit for subfolders: "3", "<=3", "<3", ">0", ">=5", "=2". Empty = no limit.
    pub level: String,
    pub name_min: usize,
    pub name_max: usize,
    pub path_min: usize,
    pub path_max: usize,
    /// JavaScript condition; the item is listed when it evaluates to true.
    pub condition: String,
}

impl Default for Filters {
    fn default() -> Self {
        Filters {
            mask: "*".into(),
            match_case: false,
            regex: false,
            files: true,
            folders: false,
            hidden: false,
            subfolders: false,
            level: String::new(),
            name_min: 0,
            name_max: 0,
            path_min: 0,
            path_max: 0,
            condition: String::new(),
        }
    }
}

/// One row of the file list.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub created: Option<String>,
    pub modified: Option<String>,
    pub accessed: Option<String>,
    pub attrs: String,
    /// Folder the item was listed from.
    pub root: String,
    /// Path of the item's folder relative to `root` ("" for the root itself).
    pub subdir: String,
    pub depth: usize,
}

pub fn entry_from(info: &FileInfo, root: &Path, depth: usize) -> Entry {
    let f = |d: Option<chrono::NaiveDateTime>| d.map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string());
    let parent = info.path.parent().unwrap_or(Path::new(""));
    Entry {
        path: info.path.to_string_lossy().into_owned(),
        name: info
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        is_dir: info.is_dir,
        size: info.size,
        created: f(info.created),
        modified: f(info.modified),
        accessed: f(info.accessed),
        attrs: info.attrs.clone(),
        root: root.to_string_lossy().into_owned(),
        subdir: parent
            .strip_prefix(root)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default(),
        depth,
    }
}

/// Splits a mask field into (include, exclude) masks. Quotes keep spaces; \" is a quote.
fn parse_masks(mask: &str) -> (Vec<String>, Vec<String>) {
    let mut tokens = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = mask.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => {
                if !cur.is_empty() {
                    tokens.push(std::mem::take(&mut cur));
                }
            }
            _ => cur.push(c),
        }
    }
    if !cur.is_empty() {
        tokens.push(cur);
    }
    let (mut inc, mut exc) = (Vec::new(), Vec::new());
    for t in tokens {
        match t.strip_prefix('!') {
            Some(rest) if !rest.is_empty() => exc.push(rest.to_string()),
            _ => inc.push(t),
        }
    }
    (inc, exc)
}

/// Checks a name against the mask field (wildcards or a regular expression).
pub fn mask_ok(f: &Filters, name: &str) -> Result<bool, String> {
    let mask = f.mask.trim();
    if mask.is_empty() || mask == "*" || mask == "*.*" {
        return Ok(true);
    }
    if f.regex {
        let re = fancy_regex::RegexBuilder::new(mask)
            .case_insensitive(!f.match_case)
            .build()
            .map_err(|e| format!("Invalid filter expression: {e}"))?;
        return Ok(re.is_match(name).unwrap_or(false));
    }
    let (inc, exc) = parse_masks(mask);
    let m = |p: &String| p == "*.*" || text::mask_matches(p, name, f.match_case);
    Ok((inc.is_empty() || inc.iter().any(m)) && !exc.iter().any(m))
}

/// Parses a level limit into (min, max) depth, where depth 0 is the folder itself.
fn level_bounds(level: &str, subfolders: bool) -> (usize, usize) {
    if !subfolders {
        return (0, 0);
    }
    let l = level.trim().replace(' ', "");
    let num = |s: &str| s.parse::<usize>().ok();
    if l.is_empty() {
        return (0, usize::MAX);
    }
    if let Some(n) = l.strip_prefix("<=").and_then(num) {
        (0, n)
    } else if let Some(n) = l.strip_prefix(">=").and_then(num) {
        (n, usize::MAX)
    } else if let Some(n) = l.strip_prefix('<').and_then(num) {
        (0, n.saturating_sub(1))
    } else if let Some(n) = l.strip_prefix('>').and_then(num) {
        (n + 1, usize::MAX)
    } else if let Some(n) = l.strip_prefix('=').and_then(num) {
        (n, n)
    } else if let Some(n) = num(&l) {
        (0, n)
    } else {
        (0, usize::MAX)
    }
}

fn is_hidden(info: &FileInfo) -> bool {
    info.attrs.contains('H') || info.attrs.contains('S')
}

/// Lists the contents of `roots` that pass the filters, in name order per folder.
pub fn scan(roots: &[String], f: &Filters, js: Option<&QuickJs>) -> Result<Vec<Entry>, String> {
    let (min_d, max_d) = level_bounds(&f.level, f.subfolders);
    let mut out = Vec::new();
    for root in roots {
        let root = PathBuf::from(root);
        if !root.is_dir() {
            return Err(format!("Folder not found: {}", root.display()));
        }
        walk(&root, &root, 0, min_d, max_d, f, js, &mut out)?;
    }
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
fn walk(
    root: &Path,
    dir: &Path,
    depth: usize,
    min_d: usize,
    max_d: usize,
    f: &Filters,
    js: Option<&QuickJs>,
    out: &mut Vec<Entry>,
) -> Result<(), String> {
    let rd = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) if depth == 0 => return Err(format!("Cannot open {}: {e}", dir.display())),
        Err(_) => return Ok(()),
    };
    let mut infos: Vec<FileInfo> = rd
        .filter_map(Result::ok)
        .filter_map(|e| {
            let p = e.path();
            let md = e.metadata().ok()?;
            Some(FileInfo::from_metadata(&p, &md))
        })
        .collect();
    infos.sort_by(|a, b| {
        natural_cmp(
            &a.path.to_string_lossy().to_lowercase(),
            &b.path.to_string_lossy().to_lowercase(),
        )
    });
    let mut subdirs = Vec::new();
    for info in infos {
        if !f.hidden && is_hidden(&info) {
            continue;
        }
        let name = info
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if info.is_dir && depth < max_d {
            subdirs.push(info.path.clone());
        }
        if depth < min_d {
            continue;
        }
        let wanted = if info.is_dir { f.folders } else { f.files };
        if !wanted {
            continue;
        }
        let name_len = name.chars().count();
        let path_len = info.path.to_string_lossy().chars().count();
        if (f.name_min > 0 && name_len < f.name_min)
            || (f.name_max > 0 && name_len > f.name_max)
            || (f.path_min > 0 && path_len < f.path_min)
            || (f.path_max > 0 && path_len > f.path_max)
        {
            continue;
        }
        if (!info.is_dir || f.folders) && !mask_ok(f, &name)? {
            continue;
        }
        let info = Arc::new(info);
        if !f.condition.trim().is_empty() {
            if let Some(js) = js {
                if !js.condition(&f.condition, info.clone(), out.len() + 1)? {
                    continue;
                }
            }
        }
        out.push(entry_from(&info, root, depth));
    }
    for d in subdirs {
        walk(root, &d, depth + 1, min_d, max_d, f, js, out)?;
    }
    Ok(())
}

/// Turns dropped or pasted paths into entries. Missing paths are skipped.
pub fn stat_paths(paths: &[String]) -> Vec<Entry> {
    paths
        .iter()
        .map(|p| p.trim().trim_matches('"'))
        .filter(|p| !p.is_empty())
        .filter_map(|p| {
            let path = PathBuf::from(p);
            let info = FileInfo::load(&path).ok()?;
            let root = path.parent().unwrap_or(Path::new("")).to_path_buf();
            Some(entry_from(&info, &root, 0))
        })
        .collect()
}

/// Natural ("logical") order: runs of digits compare as numbers, so File2 < File10.
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let (mut x, mut y) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (x.peek().copied(), y.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(c), Some(d)) if c.is_ascii_digit() && d.is_ascii_digit() => {
                let mut n1 = String::new();
                while let Some(c) = x.peek().copied().filter(char::is_ascii_digit) {
                    n1.push(c);
                    x.next();
                }
                let mut n2 = String::new();
                while let Some(d) = y.peek().copied().filter(char::is_ascii_digit) {
                    n2.push(d);
                    y.next();
                }
                let t1 = n1.trim_start_matches('0');
                let t2 = n2.trim_start_matches('0');
                let o = t1
                    .len()
                    .cmp(&t2.len())
                    .then(t1.cmp(t2))
                    .then(n1.len().cmp(&n2.len()));
                if o != Ordering::Equal {
                    return o;
                }
            }
            (Some(c), Some(d)) => {
                if c != d {
                    return c.cmp(&d);
                }
                x.next();
                y.next();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(mask: &str) -> Filters {
        Filters {
            mask: mask.into(),
            ..Filters::default()
        }
    }

    #[test]
    fn masks_support_multiple_not_and_quotes() {
        assert!(mask_ok(&f("*.mp3 *.doc"), "a.MP3").unwrap());
        assert!(!mask_ok(&f("*.mp3 *.doc"), "a.xls").unwrap());
        assert!(mask_ok(&f("* !*.doc !*.mp3"), "a.txt").unwrap());
        assert!(!mask_ok(&f("* !*.doc !*.mp3"), "a.doc").unwrap());
        assert!(mask_ok(&f("\"file with spaces.*\""), "file with spaces.txt").unwrap());
        let re = Filters {
            mask: r"^\d+\.jpg$".into(),
            regex: true,
            ..Filters::default()
        };
        assert!(mask_ok(&re, "123.JPG").unwrap());
        assert!(!mask_ok(&re, "a123.jpg").unwrap());
    }

    #[test]
    fn level_expressions() {
        assert_eq!(level_bounds("3", true), (0, 3));
        assert_eq!(level_bounds("<=3", true), (0, 3));
        assert_eq!(level_bounds("<3", true), (0, 2));
        assert_eq!(level_bounds(">0", true), (1, usize::MAX));
        assert_eq!(level_bounds("=2", true), (2, 2));
        assert_eq!(level_bounds("", false), (0, 0));
    }

    #[test]
    fn natural_order() {
        let mut v = vec!["file10.txt", "file2.txt", "file1.txt", "file02.txt"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(v, ["file1.txt", "file2.txt", "file02.txt", "file10.txt"]);
    }

    #[test]
    fn scan_respects_filters_and_levels() {
        let root = std::env::temp_dir().join(format!("alterix-scan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("sub/deep")).unwrap();
        for p in [
            "a.jpg",
            "b.txt",
            ".hidden.jpg",
            "sub/c.jpg",
            "sub/deep/d.jpg",
        ] {
            std::fs::write(root.join(p), "x").unwrap();
        }
        // Windows hides files by attribute, not by a leading dot.
        #[cfg(windows)]
        crate::ops::set_attributes(
            &root.join(".hidden.jpg"),
            &crate::engine::Attributes {
                hidden: crate::engine::Tri::Set,
                ..Default::default()
            },
        )
        .unwrap();
        let r = vec![root.to_string_lossy().into_owned()];
        let names = |fl: &Filters| {
            scan(&r, fl, None)
                .unwrap()
                .into_iter()
                .map(|e| e.name)
                .collect::<Vec<_>>()
        };
        assert_eq!(names(&f("*.jpg")), ["a.jpg"]);
        assert_eq!(
            names(&Filters {
                hidden: true,
                ..f("*.jpg")
            }),
            [".hidden.jpg", "a.jpg"]
        );
        assert_eq!(
            names(&Filters {
                subfolders: true,
                ..f("*.jpg")
            }),
            ["a.jpg", "c.jpg", "d.jpg"]
        );
        assert_eq!(
            names(&Filters {
                subfolders: true,
                level: ">0".into(),
                ..f("*.jpg")
            }),
            ["c.jpg", "d.jpg"]
        );
        assert_eq!(
            names(&Filters {
                subfolders: true,
                level: "1".into(),
                ..f("*.jpg")
            }),
            ["a.jpg", "c.jpg"]
        );
        assert_eq!(
            names(&Filters {
                files: false,
                folders: true,
                ..f("*")
            }),
            ["sub"]
        );
        assert_eq!(
            names(&Filters {
                name_min: 6,
                ..f("*")
            }),
            Vec::<String>::new()
        );
        let state = crate::script::ScriptState::default();
        let js = QuickJs::new(&state).unwrap();
        let cond = Filters {
            condition: "ext == 'txt'".into(),
            ..f("*")
        };
        assert_eq!(
            scan(&r, &cond, Some(&js))
                .unwrap()
                .into_iter()
                .map(|e| e.name)
                .collect::<Vec<_>>(),
            ["b.txt"]
        );
        let entries = scan(
            &r,
            &Filters {
                subfolders: true,
                ..f("d.jpg")
            },
            None,
        )
        .unwrap();
        assert_eq!(Path::new(&entries[0].subdir), Path::new("sub/deep"));
        std::fs::remove_dir_all(&root).unwrap();
    }
}
