"""End-to-end check of an Alterix build: self-test, rename with Replace, Extension, Change case and Script steps,
a filter mask, a case-only rename, a name swap, non-ASCII names, the logs and undo, on made-up files.

It runs the app's own check modes (--self-test, --check-rename, --check-undo), which use the same rename and
undo code as the window, and checks each step's effect on disk, not only the reply.
It only touches the made-up files it creates in the work folder. Standard library only, so it runs on any test machine.

Usage:
  python scripts/check_e2e.py --app <Alterix executable> --work <new empty folder>
    macOS:   --app Alterix.app/Contents/MacOS/alterix
    Windows: --app Alterix.exe
    Linux:   --app Alterix.AppImage, or the built binary
  Leave out --work to use a new temporary folder.
"""
# Template from the tariq-project-standards skill. Keep the helpers; replace the marked section with the app's checks.
import argparse, hashlib, json, os, subprocess, sys, tempfile
from pathlib import Path

# --- Fill in for this app ----------------------------------------------------------------------------
APP_NAME = 'Alterix'
# -----------------------------------------------------------------------------------------------------

failures = []


def check(name, ok, detail=''):
    """Print PASS or FAIL for one step and remember failures."""
    print(('PASS ' if ok else 'FAIL ') + name + (f' -- {detail}' if detail and not ok else ''))
    if not ok:
        failures.append(name)


def run(*app_args, timeout=120):
    """Run the app with arguments. Returns (exit code, stdout)."""
    r = subprocess.run([args.app, *app_args], capture_output=True, text=True, encoding='utf-8',
                       errors='replace', timeout=timeout)
    return r.returncode, r.stdout


def sha(p):
    return hashlib.sha256(Path(p).read_bytes()).hexdigest()


def make_work_folder(path):
    """Use a new temporary folder, or the given one only if it's new or empty, so real files are never touched."""
    if path is None:
        return Path(tempfile.mkdtemp(prefix='check_e2e_'))
    work = Path(path)
    if work.exists() and any(work.iterdir()):
        sys.exit(f'Work folder must be new or empty: {work}')
    work.mkdir(parents=True, exist_ok=True)
    return work


parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
parser.add_argument('--app', required=True, help='Path to the Alterix executable')
parser.add_argument('--work', help='New or empty work folder; default is a new temporary folder')
args = parser.parse_args()

work = make_work_folder(args.work)
print(f'Work folder: {work}')
# Where a build would put its data folder: beside the executable, or beside the .app on macOS.
exe_dir = Path(args.app).resolve().parent
data_beside_app = [d / 'Alterix-data' for d in (exe_dir, exe_dir.parent.parent.parent)]
absent_before = [d for d in data_beside_app if not d.exists()]

# --- App checks --------------------------------------------------------------------------------------


def names(folder):
    """Exact names on disk (os.listdir keeps the stored case, unlike a case-insensitive lookup)."""
    return sorted(os.listdir(folder))


def by_content(folder):
    """Map content hash -> name, so a check can follow each file through a rename."""
    return {sha(folder / n): n for n in os.listdir(folder)}


def make_files(folder, files):
    folder.mkdir()
    for name, text in files.items():
        (folder / name).write_text(text, encoding='utf-8')


def scenario(label, files, preset, expected):
    """Rename `files` with `preset`, check the result is `expected` (old name -> new name), then undo."""
    folder = work / label
    data = work / f'{label}-data'
    make_files(folder, files)
    before = by_content(folder)
    stats = {n: (folder / n).stat().st_mtime_ns for n in files}
    preset_path = work / f'{label}.alterix'
    preset_path.write_text(json.dumps({'app': APP_NAME, 'version': 1, **preset}), encoding='utf-8')

    code, _ = run('--check-rename', str(preset_path), str(folder), str(data))
    result_file = data / 'check-result.json'
    result = json.loads(result_file.read_text(encoding='utf-8')) if result_file.exists() else {}
    check(f'{label}: rename exits 0 and reports ok', code == 0 and result.get('ok') is True,
          f'exit {code}, {result.get("error") or result.get("report", {}).get("results")}')
    want = sorted(expected.get(n, n) for n in files)
    check(f'{label}: names on disk are the new names', names(folder) == want, f'{names(folder)} != {want}')
    after = by_content(folder)
    check(f'{label}: every file kept its content', set(after) == set(before))
    moved = {before[h]: after.get(h) for h in before}
    check(f'{label}: each file got its own new name', all(moved[o] == expected.get(o, o) for o in files), moved)
    untouched = [n for n in files if n not in expected]
    check(f'{label}: files outside the selection did not change',
          all((folder / n).exists() and (folder / n).stat().st_mtime_ns == stats[n] for n in untouched), untouched)
    log = data / 'rename-activity.log'
    check(f'{label}: rename activity log lists the renames',
          log.exists() and all(expected[o] in log.read_text(encoding='utf-8') for o in expected))
    app_log = data / 'Alterix.log'
    check(f'{label}: Alterix.log records the rename',
          app_log.exists() and 'Rename finished' in app_log.read_text(encoding='utf-8'))

    code, _ = run('--check-undo', str(data))
    result = json.loads(result_file.read_text(encoding='utf-8')) if result_file.exists() else {}
    check(f'{label}: undo exits 0 and reports ok', code == 0 and result.get('ok') is True,
          f'exit {code}, {result.get("error") or result.get("report", {}).get("results")}')
    check(f'{label}: undo restores the original names', names(folder) == sorted(files), names(folder))
    check(f'{label}: undo restores the original bytes', by_content(folder) == before)
    code, _ = run('--check-undo', str(data))
    check(f'{label}: a second undo finds nothing to undo', code != 0)


# Self-test: the build loads its web files, engine, scripts and a data folder.
code, out = run('--self-test')
lines = out.strip().splitlines()
check('Self-test exits 0', code == 0, f'exit {code}: {out.strip()}')
if lines:  # A windowed Windows build prints nothing; its exit code is the check.
    check('Self-test prints the version last', lines[-1].startswith(f'{APP_NAME} v'), lines[-1])

# 1. Replace text and lower-case extensions, with a mask that leaves the notes file out.
scenario('replace', {
    'IMG_0001.JPG': 'made-up photo 1', 'IMG_0002.JPG': 'made-up photo 2', 'IMG_0003.jpg': 'made-up photo 3',
    'notes.txt': 'made-up notes',
}, {
    'steps': [
        {'enabled': True, 'rule': {'kind': 'replace', 'pairs': [{'find': 'IMG_', 'with': 'Holiday '}]}},
        {'enabled': True, 'rule': {'kind': 'extension', 'mode': 'lower'}},
    ],
    'filters': {'mask': '*.jpg'},
}, {'IMG_0001.JPG': 'Holiday 0001.jpg', 'IMG_0002.JPG': 'Holiday 0002.jpg', 'IMG_0003.jpg': 'Holiday 0003.jpg'})

# 2. Case-only renames (a trap on case-insensitive disks) and non-ASCII names.
scenario('case', {
    'readme.md': 'made-up readme', 'café menu.md': 'made-up menu', 'Ünïcode ñame.md': 'made-up unicode',
}, {
    'steps': [{'enabled': True, 'rule': {'kind': 'case', 'mode': 'upper'}}],
}, {'readme.md': 'README.md', 'café menu.md': 'CAFÉ MENU.md', 'Ünïcode ñame.md': 'ÜNÏCODE ÑAME.md'})

# 3. Swap two names with a Script step: needs a temporary name, and runs the JavaScript host.
scenario('swap', {'first.txt': 'made-up first', 'second.txt': 'made-up second'}, {
    'steps': [{'enabled': True, 'rule': {'kind': 'script',
                                         'code': "newName = name == 'first' ? 'second' : 'first';"}}],
}, {'first.txt': 'second.txt', 'second.txt': 'first.txt'})

# The checks wrote only to the work folder: no data folder appeared beside the app.
check('No data folder was created beside the app', not any(d.exists() for d in absent_before),
      [str(d) for d in absent_before if d.exists()])
# -----------------------------------------------------------------------------------------------------

print(f'RESULT: {"FAIL" if failures else "PASS"} ({len(failures)} failed)')
sys.exit(1 if failures else 0)
