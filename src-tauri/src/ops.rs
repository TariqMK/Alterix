//! Carries out a confirmed rename plan: renames, moves or copies items, sets attributes
//! and timestamps, and records everything so it can be undone. Items are processed
//! deepest first, so folder contents are renamed before their folder. Swaps and chains
//! (a→b, b→a) go through a temporary name. Existing files are never deleted: with
//! "Overwrite" they are moved to the system trash.

use crate::engine::{Attributes, PreviewItem, Status, Tri};
use crate::meta::{self, FileInfo};
use crate::store::{Batch, UndoOp};
use chrono::NaiveDateTime;
use serde::Serialize;
use std::collections::HashSet;
use std::fs::FileTimes;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpResult {
    pub path: String,
    pub new_path: String,
    pub ok: bool,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub results: Vec<OpResult>,
    pub renamed: usize,
    pub failed: usize,
    pub batch_id: Option<String>,
}

/// What to do with each item, taken from a fresh preview.
pub struct Plan<'a> {
    pub items: &'a [PreviewItem],
    pub copy: bool,
    pub overwrite: bool,
    pub reverse: bool,
    pub attributes: &'a Attributes,
    pub set_times: bool,
}

fn parse_time(s: &Option<String>) -> Option<NaiveDateTime> {
    s.as_deref().and_then(meta::parse_date)
}

fn to_system(t: NaiveDateTime) -> SystemTime {
    use chrono::TimeZone;
    chrono::Local
        .from_local_datetime(&t)
        .earliest()
        .map(SystemTime::from)
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

fn fmt(t: Option<NaiveDateTime>) -> Option<String> {
    t.map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
}

/// Sets created / modified / accessed. Created can only be set on Windows and macOS.
pub fn set_times(
    path: &Path,
    created: Option<NaiveDateTime>,
    modified: Option<NaiveDateTime>,
    accessed: Option<NaiveDateTime>,
) -> Result<(), String> {
    if created.is_none() && modified.is_none() && accessed.is_none() {
        return Ok(());
    }
    let mut times = FileTimes::new();
    if let Some(m) = modified {
        times = times.set_modified(to_system(m));
    }
    if let Some(a) = accessed {
        times = times.set_accessed(to_system(a));
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::FileTimesExt;
        if let Some(c) = created {
            times = times.set_created(to_system(c));
        }
    }
    #[cfg(target_os = "macos")]
    {
        use std::os::macos::fs::FileTimesExt;
        if let Some(c) = created {
            times = times.set_created(to_system(c));
        }
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    let _ = created;
    let file = open_for_times(path).map_err(|e| format!("Cannot change timestamps: {e}"))?;
    file.set_times(times)
        .map_err(|e| format!("Cannot change timestamps: {e}"))
}

fn open_for_times(path: &Path) -> std::io::Result<std::fs::File> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // FILE_WRITE_ATTRIBUTES, FILE_FLAG_BACKUP_SEMANTICS (needed for folders).
        std::fs::OpenOptions::new()
            .access_mode(0x100)
            .custom_flags(0x0200_0000)
            .open(path)
    }
    #[cfg(not(windows))]
    {
        if path.is_dir() {
            std::fs::File::open(path)
        } else {
            std::fs::OpenOptions::new()
                .write(true)
                .open(path)
                .or_else(|_| std::fs::File::open(path))
        }
    }
}

/// Applies attribute changes. Returns a note when an attribute is not supported here.
pub fn set_attributes(path: &Path, a: &Attributes) -> Result<Option<String>, String> {
    let mut notes = Vec::new();
    if a.read_only != Tri::Keep {
        let md = std::fs::metadata(path).map_err(|e| e.to_string())?;
        let mut p = md.permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        p.set_readonly(a.read_only == Tri::Set);
        std::fs::set_permissions(path, p).map_err(|e| format!("Cannot change read-only: {e}"))?;
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use std::os::windows::fs::MetadataExt;
        let md = std::fs::metadata(path).map_err(|e| e.to_string())?;
        let mut bits = md.file_attributes();
        for (tri, bit) in [(a.hidden, 0x2u32), (a.system, 0x4), (a.archive, 0x20)] {
            match tri {
                Tri::Set => bits |= bit,
                Tri::Clear => bits &= !bit,
                Tri::Keep => {}
            }
        }
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let ok = unsafe {
            windows_sys::Win32::Storage::FileSystem::SetFileAttributesW(wide.as_ptr(), bits)
        };
        if ok == 0 {
            return Err(format!(
                "Cannot change attributes: {}",
                std::io::Error::last_os_error()
            ));
        }
    }
    #[cfg(target_os = "macos")]
    {
        use std::ffi::CString;
        use std::os::macos::fs::MetadataExt;
        use std::os::unix::ffi::OsStrExt;
        if a.hidden != Tri::Keep {
            const UF_HIDDEN: u32 = 0x8000;
            let md = std::fs::metadata(path).map_err(|e| e.to_string())?;
            let flags = if a.hidden == Tri::Set {
                md.st_flags() | UF_HIDDEN
            } else {
                md.st_flags() & !UF_HIDDEN
            };
            let c = CString::new(path.as_os_str().as_bytes()).map_err(|e| e.to_string())?;
            if unsafe { libc::chflags(c.as_ptr(), flags as _) } != 0 {
                return Err(format!(
                    "Cannot change hidden: {}",
                    std::io::Error::last_os_error()
                ));
            }
        }
        if a.system != Tri::Keep || a.archive != Tri::Keep {
            notes.push("System and Archive attributes exist only on Windows.");
        }
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        if a.hidden != Tri::Keep || a.system != Tri::Keep || a.archive != Tri::Keep {
            notes.push("On Linux only Read-only can be changed. A leading dot hides a file.");
        }
    }
    Ok(if notes.is_empty() {
        None
    } else {
        Some(notes.join(" "))
    })
}

fn attrs_to_tri(letters: &str) -> Attributes {
    let t = |c: char| {
        if letters.contains(c) {
            Tri::Set
        } else {
            Tri::Clear
        }
    };
    Attributes {
        read_only: t('R'),
        hidden: t('H'),
        system: t('S'),
        archive: t('A'),
    }
}

fn copy_recursive(from: &Path, to: &Path) -> std::io::Result<()> {
    if from.is_dir() {
        std::fs::create_dir_all(to)?;
        for e in std::fs::read_dir(from)? {
            let e = e?;
            copy_recursive(&e.path(), &to.join(e.file_name()))?;
        }
        Ok(())
    } else {
        std::fs::copy(from, to).map(|_| ())
    }
}

/// Renames, falling back to copy + remove when the target is on another drive.
fn move_path(from: &Path, to: &Path) -> std::io::Result<()> {
    match std::fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::CrossesDevices => {
            copy_recursive(from, to)?;
            if from.is_dir() {
                std::fs::remove_dir_all(from)
            } else {
                std::fs::remove_file(from)
            }
        }
        Err(e) => Err(e),
    }
}

fn temp_name(p: &Path, n: usize) -> PathBuf {
    let name = p
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    p.with_file_name(format!(".{name}.alterix-{}-{n}", std::process::id()))
}

fn same_path(a: &Path, b: &Path) -> bool {
    a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
}

struct Job {
    from: PathBuf,
    to: PathBuf,
    is_dir: bool,
    times: [Option<NaiveDateTime>; 3],
    index: usize,
}

/// Carries out `plan`. Only items with status Changed (or with attribute / timestamp
/// changes) are touched.
pub fn execute(plan: &Plan) -> (Report, Option<Batch>) {
    let attrs_change = *plan.attributes != Attributes::default();
    let mut jobs: Vec<Job> = Vec::new();
    let mut results: Vec<OpResult> = plan
        .items
        .iter()
        .map(|p| OpResult {
            path: p.path.clone(),
            new_path: p.path.clone(),
            ok: true,
            message: None,
        })
        .collect();
    for (i, p) in plan.items.iter().enumerate() {
        let times = [
            parse_time(&p.new_created),
            parse_time(&p.new_modified),
            parse_time(&p.new_accessed),
        ];
        let has_times = plan.set_times && times.iter().any(Option::is_some);
        match p.status {
            Status::Changed => {}
            Status::Unchanged if has_times || attrs_change => {}
            Status::Unchanged => continue,
            _ => {
                results[i].ok = false;
                results[i].message =
                    Some(p.message.clone().unwrap_or_else(|| "Not renamed.".into()));
                continue;
            }
        }
        let from = PathBuf::from(&p.path);
        let to = if p.status == Status::Changed {
            PathBuf::from(&p.new_path)
        } else {
            from.clone()
        };
        jobs.push(Job {
            is_dir: from.is_dir(),
            from,
            to,
            times: if plan.set_times { times } else { [None; 3] },
            index: i,
        });
    }
    // Deepest first, so a folder's contents move before the folder itself.
    jobs.sort_by_key(|j| std::cmp::Reverse(j.from.components().count()));
    if plan.reverse {
        jobs.reverse();
        jobs.sort_by_key(|j| std::cmp::Reverse(j.from.components().count()));
    }

    let mut pending: HashSet<String> = jobs
        .iter()
        .map(|j| j.from.to_string_lossy().to_lowercase())
        .collect();
    let mut deferred: Vec<(PathBuf, usize)> = Vec::new();
    let mut ops: Vec<UndoOp> = Vec::new();
    let mut renamed = 0;
    let mut tmp_n = 0;

    let finish =
        |job: &Job, at: &Path, results: &mut Vec<OpResult>, ops: &mut Vec<UndoOp>, copied: bool| {
            let before = FileInfo::load(at).ok();
            let mut msgs = Vec::new();
            let old_attrs = before.as_ref().map(|b| b.attrs.clone());
            if attrs_change {
                match set_attributes(at, plan.attributes) {
                    Ok(Some(n)) => msgs.push(n),
                    Ok(None) => {}
                    Err(e) => msgs.push(e),
                }
            }
            let [c, m, a] = job.times;
            let old_times = before
                .as_ref()
                .map(|b| [fmt(b.created), fmt(b.modified), fmt(b.accessed)]);
            if let Err(e) = set_times(at, c, m, a) {
                msgs.push(e);
            }
            let r = &mut results[job.index];
            r.new_path = at.to_string_lossy().into_owned();
            if !msgs.is_empty() {
                r.message = Some(msgs.join(" "));
            }
            ops.push(UndoOp {
                from: job.from.to_string_lossy().into_owned(),
                to: at.to_string_lossy().into_owned(),
                is_dir: job.is_dir,
                copied,
                old_times: if job.times.iter().any(Option::is_some) {
                    old_times
                } else {
                    None
                },
                old_attrs: if attrs_change { old_attrs } else { None },
            });
        };

    for (ji, job) in jobs.iter().enumerate() {
        let i = job.index;
        if job.from == job.to {
            finish(job, &job.to, &mut results, &mut ops, false);
            continue;
        }
        if let Some(parent) = job.to.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                results[i].ok = false;
                results[i].message =
                    Some(format!("Cannot create folder {}: {e}", parent.display()));
                continue;
            }
        }
        let target_key = job.to.to_string_lossy().to_lowercase();
        let case_only = same_path(&job.from, &job.to);
        if job.to.exists() && !case_only {
            if pending.contains(&target_key) && !plan.copy {
                // Another item still has this name: park this one under a temporary name.
                tmp_n += 1;
                let tmp = temp_name(&job.from, tmp_n);
                match std::fs::rename(&job.from, &tmp) {
                    Ok(()) => {
                        pending.remove(&job.from.to_string_lossy().to_lowercase());
                        deferred.push((tmp, ji));
                    }
                    Err(e) => {
                        results[i].ok = false;
                        results[i].message = Some(e.to_string());
                    }
                }
                continue;
            } else if plan.overwrite {
                if let Err(e) = trash::delete(&job.to) {
                    results[i].ok = false;
                    results[i].message =
                        Some(format!("Cannot move the existing item to the trash: {e}"));
                    continue;
                }
            } else {
                results[i].ok = false;
                results[i].message = Some("An item with this name already exists.".into());
                continue;
            }
        }
        let res = if plan.copy {
            copy_recursive(&job.from, &job.to)
        } else {
            move_path(&job.from, &job.to)
        };
        match res {
            Ok(()) => {
                pending.remove(&job.from.to_string_lossy().to_lowercase());
                renamed += 1;
                finish(job, &job.to, &mut results, &mut ops, plan.copy);
            }
            Err(e) => {
                results[i].ok = false;
                results[i].message = Some(e.to_string());
            }
        }
    }
    for (tmp, ji) in deferred {
        let job = &jobs[ji];
        let i = job.index;
        if job.to.exists() {
            if plan.overwrite && trash::delete(&job.to).is_ok() {
                // fall through to the move below
            } else {
                // Put it back where it was.
                let _ = std::fs::rename(&tmp, &job.from);
                results[i].ok = false;
                results[i].message = Some("An item with this name already exists.".into());
                continue;
            }
        }
        match std::fs::rename(&tmp, &job.to) {
            Ok(()) => {
                renamed += 1;
                finish(job, &job.to, &mut results, &mut ops, false);
            }
            Err(e) => {
                let _ = std::fs::rename(&tmp, &job.from);
                results[i].ok = false;
                results[i].message = Some(e.to_string());
            }
        }
    }
    let failed = results.iter().filter(|r| !r.ok).count();
    let batch = if ops.is_empty() {
        None
    } else {
        let now = chrono::Local::now();
        Some(Batch {
            id: format!("{}", now.timestamp_millis()),
            when: now.format("%Y-%m-%d %H:%M:%S").to_string(),
            summary: format!(
                "{} item{} {}",
                ops.len(),
                if ops.len() == 1 { "" } else { "s" },
                if plan.copy { "copied" } else { "renamed" }
            ),
            ops,
            undone: false,
        })
    };
    let id = batch.as_ref().map(|b| b.id.clone());
    (
        Report {
            results,
            renamed,
            failed,
            batch_id: id,
        },
        batch,
    )
}

/// Reverses a batch: restores names (parents before contents), attributes and timestamps.
/// Copies are moved to the trash.
pub fn undo(batch: &Batch) -> Report {
    let mut results = Vec::new();
    let mut ops: Vec<&UndoOp> = batch.ops.iter().collect();
    // Shallowest destination first, so folders get their old names before their contents.
    ops.sort_by_key(|o| Path::new(&o.to).components().count());
    let mut deferred: Vec<(PathBuf, &UndoOp)> = Vec::new();
    let mut tmp_n = 0;
    let pending: HashSet<String> = ops.iter().map(|o| o.to.to_lowercase()).collect();
    let restore_meta = |o: &UndoOp, at: &Path| {
        if let Some(t) = &o.old_times {
            let _ = set_times(at, parse_time(&t[0]), parse_time(&t[1]), parse_time(&t[2]));
        }
        if let Some(a) = &o.old_attrs {
            let _ = set_attributes(at, &attrs_to_tri(a));
        }
    };
    for o in &ops {
        let (to, from) = (PathBuf::from(&o.to), PathBuf::from(&o.from));
        if o.copied {
            let r = trash::delete(&to);
            results.push(OpResult {
                path: o.to.clone(),
                new_path: o.from.clone(),
                ok: r.is_ok(),
                message: r.err().map(|e| e.to_string()),
            });
            continue;
        }
        if to == from {
            restore_meta(o, &to);
            results.push(OpResult {
                path: o.to.clone(),
                new_path: o.from.clone(),
                ok: true,
                message: None,
            });
            continue;
        }
        if from.exists() && !same_path(&from, &to) {
            if pending.contains(&o.from.to_lowercase()) {
                tmp_n += 1;
                let tmp = temp_name(&to, tmp_n);
                if std::fs::rename(&to, &tmp).is_ok() {
                    deferred.push((tmp, o));
                    continue;
                }
            }
            results.push(OpResult {
                path: o.to.clone(),
                new_path: o.from.clone(),
                ok: false,
                message: Some("The original name is in use.".into()),
            });
            continue;
        }
        let r = move_path(&to, &from);
        if r.is_ok() {
            restore_meta(o, &from);
        }
        results.push(OpResult {
            path: o.to.clone(),
            new_path: o.from.clone(),
            ok: r.is_ok(),
            message: r.err().map(|e| e.to_string()),
        });
    }
    for (tmp, o) in deferred {
        let from = PathBuf::from(&o.from);
        let r = std::fs::rename(&tmp, &from);
        if r.is_ok() {
            restore_meta(o, &from);
        } else {
            let _ = std::fs::rename(&tmp, &o.to);
        }
        results.push(OpResult {
            path: o.to.clone(),
            new_path: o.from.clone(),
            ok: r.is_ok(),
            message: r.err().map(|e| e.to_string()),
        });
    }
    let failed = results.iter().filter(|r| !r.ok).count();
    Report {
        renamed: results.len() - failed,
        failed,
        results,
        batch_id: Some(batch.id.clone()),
    }
}

/// A script that reverses a batch: a .bat file on Windows, a shell script elsewhere.
pub fn undo_script(batch: &Batch, windows: bool) -> String {
    let mut ops: Vec<&UndoOp> = batch.ops.iter().filter(|o| o.from != o.to).collect();
    ops.sort_by_key(|o| Path::new(&o.to).components().count());
    let mut s = String::new();
    if windows {
        s.push_str("@echo off\r\nrem Alterix undo script: reverses the rename run of ");
        s.push_str(&batch.when);
        s.push_str("\r\nchcp 65001 >nul\r\n");
        for o in ops {
            let q = |p: &str| p.replace('%', "%%");
            if o.copied {
                s.push_str(&format!("del \"{}\"\r\n", q(&o.to)));
            } else {
                s.push_str(&format!("move \"{}\" \"{}\"\r\n", q(&o.to), q(&o.from)));
            }
        }
    } else {
        s.push_str("#!/bin/sh\n# Alterix undo script: reverses the rename run of ");
        s.push_str(&batch.when);
        s.push('\n');
        let q = |p: &str| format!("'{}'", p.replace('\'', "'\\''"));
        for o in ops {
            if o.copied {
                s.push_str(&format!("rm -r -- {}\n", q(&o.to)));
            } else {
                s.push_str(&format!("mv -n -- {} {}\n", q(&o.to), q(&o.from)));
            }
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(dir: &Path, from: &str, to: &str) -> PreviewItem {
        PreviewItem {
            path: dir.join(from).to_string_lossy().into_owned(),
            original: from.into(),
            new_name: to.into(),
            new_path: dir.join(to).to_string_lossy().into_owned(),
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
        }
    }

    fn plan<'a>(items: &'a [PreviewItem], attrs: &'a Attributes) -> Plan<'a> {
        Plan {
            items,
            copy: false,
            overwrite: false,
            reverse: false,
            attributes: attrs,
            set_times: true,
        }
    }

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("alterix-ops-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn read(p: &Path) -> String {
        std::fs::read_to_string(p).unwrap()
    }

    #[test]
    fn swap_and_chain_then_undo() {
        let d = tmpdir("swap");
        for (n, c) in [
            ("a.txt", "A"),
            ("b.txt", "B"),
            ("1.txt", "1"),
            ("2.txt", "2"),
        ] {
            std::fs::write(d.join(n), c).unwrap();
        }
        let items = vec![
            item(&d, "a.txt", "b.txt"),
            item(&d, "b.txt", "a.txt"),
            item(&d, "1.txt", "2.txt"),
            item(&d, "2.txt", "3.txt"),
        ];
        let attrs = Attributes::default();
        let (rep, batch) = execute(&plan(&items, &attrs));
        assert_eq!(rep.failed, 0, "{:?}", rep.results);
        assert_eq!(read(&d.join("a.txt")), "B");
        assert_eq!(read(&d.join("b.txt")), "A");
        assert_eq!(read(&d.join("2.txt")), "1");
        assert_eq!(read(&d.join("3.txt")), "2");
        let u = undo(&batch.unwrap());
        assert_eq!(u.failed, 0, "{:?}", u.results);
        assert_eq!(read(&d.join("a.txt")), "A");
        assert_eq!(read(&d.join("1.txt")), "1");
        assert_eq!(read(&d.join("2.txt")), "2");
        assert!(!d.join("3.txt").exists());
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn folder_and_contents_rename_together() {
        let d = tmpdir("nest");
        std::fs::create_dir_all(d.join("old")).unwrap();
        std::fs::write(d.join("old/x.txt"), "x").unwrap();
        let items = vec![
            item(&d, "old", "new"),
            item(&d.join("old"), "x.txt", "y.txt"),
        ];
        let attrs = Attributes::default();
        let (rep, batch) = execute(&plan(&items, &attrs));
        assert_eq!(rep.failed, 0, "{:?}", rep.results);
        assert!(d.join("new/y.txt").exists());
        let u = undo(&batch.unwrap());
        assert_eq!(u.failed, 0, "{:?}", u.results);
        assert!(d.join("old/x.txt").exists());
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn existing_target_is_not_overwritten_without_option() {
        let d = tmpdir("exists");
        std::fs::write(d.join("a.txt"), "a").unwrap();
        std::fs::write(d.join("b.txt"), "b").unwrap();
        let items = vec![item(&d, "a.txt", "b.txt")];
        let attrs = Attributes::default();
        let (rep, _) = execute(&plan(&items, &attrs));
        assert_eq!(rep.failed, 1);
        assert_eq!(read(&d.join("b.txt")), "b");
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn copy_to_new_folder_and_times_and_readonly() {
        let d = tmpdir("copy");
        std::fs::write(d.join("a.txt"), "a").unwrap();
        let mut it = item(&d, "a.txt", "out/b.txt");
        it.new_modified = Some("2020-01-02 03:04:05".into());
        let attrs = Attributes {
            read_only: Tri::Set,
            ..Attributes::default()
        };
        let items = vec![it];
        let p = Plan {
            copy: true,
            ..plan(&items, &attrs)
        };
        let (rep, batch) = execute(&p);
        assert_eq!(rep.failed, 0, "{:?}", rep.results);
        let dest = d.join("out/b.txt");
        assert!(d.join("a.txt").exists() && dest.exists());
        let info = FileInfo::load(&dest).unwrap();
        assert_eq!(fmt(info.modified).unwrap(), "2020-01-02 03:04:05");
        assert!(info.attrs.contains('R'));
        let script = undo_script(batch.as_ref().unwrap(), false);
        assert!(script.contains("rm -r --"));
        // Clear read-only so the folder can be removed.
        set_attributes(
            &dest,
            &Attributes {
                read_only: Tri::Clear,
                ..Attributes::default()
            },
        )
        .unwrap();
        std::fs::remove_dir_all(&d).unwrap();
    }
}
