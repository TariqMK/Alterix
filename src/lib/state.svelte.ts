// Central app state and actions: the file list, the pipeline, options, settings,
// preview, rename, undo, presets, imports and list editing.

import { invoke } from "@tauri-apps/api/core";
import { readText, writeText } from "@tauri-apps/plugin-clipboard-manager";
import { ask, message } from "@tauri-apps/plugin-dialog";
import { openUrl, revealItemInDir } from "@tauri-apps/plugin-opener";
import { SvelteSet } from "svelte/reactivity";
import { check, UpdateError } from "./updateCheck";
import {
  RULE_BY_KIND,
  type Rule,
  type RuleKind,
  type Step,
  usesClipboard,
} from "./catalog";
import {
  type Batch,
  type ColumnId,
  defaultFilters,
  defaultOptions,
  defaultSettings,
  type Entry,
  type Filters,
  type Options,
  type Pair,
  type Preview,
  type PreviewItem,
  type Report,
  type Row,
  type Settings,
} from "./types";

export const PRESET_EXT = "alterix";

interface PresetFile {
  app: "Alterix";
  version: 1;
  steps: { enabled: boolean; rule: Rule }[];
  options: Partial<Options>;
  filters: Filters;
  folders?: string[];
}

export type Dialog =
  | null
  | "settings"
  | "history"
  | "pairs"
  | "properties"
  | "debug"
  | "report"
  | "confirmEach"
  | "jump"
  | "manual"
  | "zoom"
  | "about"
  | "update"
  | "logs";

const collator = new Intl.Collator(undefined, {
  numeric: true,
  sensitivity: "base",
});
const plain = new Intl.Collator(undefined, { sensitivity: "base" });

function clone<T>(v: T): T {
  return JSON.parse(JSON.stringify(v));
}

class AppState {
  rows = $state<Row[]>([]);
  steps = $state<Step[]>([]);
  options = $state<Options>(defaultOptions());
  filters = $state<Filters>(defaultFilters());
  settings = $state<Settings>(defaultSettings());
  folders = $state<string[]>([]);
  preview = $state.raw<Map<string, PreviewItem>>(new Map());
  nextNumbers = $state.raw<[number, number][]>([]);
  selected = new SvelteSet<string>();
  anchor = $state<string | null>(null);
  sort = $state<{ col: ColumnId | null; dir: 1 | -1 }>({ col: null, dir: 1 });
  busy = $state(false);
  previewing = $state(false);
  error = $state<string | null>(null);
  notice = $state<string | null>(null);
  dialog = $state<Dialog>(null);
  dialogData = $state<unknown>(null);
  history = $state<Batch[]>([]);
  report = $state<Report | null>(null);
  dataStatus = $state<{ path: string; writable: boolean } | null>(null);
  presetPath = $state<string | null>(null);
  lastSaved = $state<string | null>(null);
  previewImage = $state<string | null>(null);
  trace = $state<string[]>([]);
  ready = $state(false);
  appInfo = $state<{
    version: string;
    buildKind: string;
    os: string;
    arch: string;
    reportUrl: string;
  } | null>(null);
  updateChecking = $state(false);
  newerVersion = $state<string | null>(null);
  private loggedSettings: Record<string, unknown> | null = null;
  private nextId = 1;
  private added = 0;

  // -------------------------------------------------------------- derived helpers

  get checkedRows(): Row[] {
    return this.rows.filter((r) => r.checked);
  }

  get counts() {
    let changed = 0,
      conflict = 0,
      invalid = 0,
      skipped = 0;
    for (const p of this.preview.values()) {
      if (p.status === "changed") changed++;
      else if (p.status === "conflict") conflict++;
      else if (p.status === "invalid") invalid++;
      else if (p.status === "skipped") skipped++;
    }
    return { changed, conflict, invalid, skipped };
  }

  newStep(kind: RuleKind): Step {
    return {
      id: this.nextId++,
      enabled: true,
      collapsed: false,
      rule: RULE_BY_KIND[kind].defaults(),
    };
  }

  setError(e: unknown) {
    this.error = e instanceof Error ? e.message : String(e);
    this.log("warning", "ui", this.error);
  }

  /** Writes a line to Alterix.log. Never logs typed text or file contents. */
  log(level: "info" | "warning" | "error", area: string, message: string) {
    invoke("log_line", { level, area, message }).catch(() => {});
  }

  private static SKIP_LOG = [
    "recent",
    "recentPresets",
    "lastFolders",
    "lastCriteria",
    "customColumn",
  ];

  /** Logs each changed setting as old → new (lists and saved criteria are skipped). */
  private logSettingChanges() {
    const now = $state.snapshot(this.settings) as unknown as Record<
      string,
      unknown
    >;
    if (this.loggedSettings) {
      for (const [k, v] of Object.entries(now)) {
        if (AppState.SKIP_LOG.includes(k)) continue;
        const old = this.loggedSettings[k];
        if (JSON.stringify(old) !== JSON.stringify(v))
          this.log(
            "info",
            "settings",
            `${k}: ${JSON.stringify(old)} → ${JSON.stringify(v)}`,
          );
      }
    }
    this.loggedSettings = now;
  }

  /** Logs settings that differ from their defaults (part of the session header). */
  logNonDefaultSettings() {
    const def = defaultSettings() as unknown as Record<string, unknown>;
    const now = $state.snapshot(this.settings) as unknown as Record<
      string,
      unknown
    >;
    const diffs = Object.entries(now)
      .filter(
        ([k, v]) =>
          !AppState.SKIP_LOG.includes(k) &&
          JSON.stringify(def[k]) !== JSON.stringify(v),
      )
      .map(([k, v]) => `${k}=${JSON.stringify(v)}`);
    this.log(
      "info",
      "settings",
      diffs.length
        ? `Changed from default: ${diffs.join(", ")}`
        : "All settings at default values",
    );
    this.loggedSettings = now;
  }

  say(text: string) {
    this.notice = text;
    setTimeout(() => {
      if (this.notice === text) this.notice = null;
    }, 4000);
  }

  // -------------------------------------------------------------- settings

  async loadSettings() {
    try {
      this.dataStatus = await invoke("data_status");
      const raw = (await invoke("load_settings")) as Partial<Settings> | null;
      if (raw && typeof raw === "object")
        this.settings = { ...defaultSettings(), ...raw };
    } catch (e) {
      this.setError(e);
    }
  }

  private saveTimer: ReturnType<typeof setTimeout> | undefined;

  saveSettings() {
    clearTimeout(this.saveTimer);
    this.saveTimer = setTimeout(() => {
      this.logSettingChanges();
      if (!this.dataStatus?.writable) return;
      invoke("save_settings", {
        settings: $state.snapshot(this.settings),
      }).catch((e) => this.setError(e));
    }, 400);
  }

  rememberCriteria() {
    if (!this.settings.recallLastCriteria) return;
    this.settings.lastCriteria = this.snapshot();
    this.saveSettings();
  }

  addRecent(key: string, value: string) {
    if (!value || !this.settings.saveRecentEntries) return;
    const list = (this.settings.recent[key] ?? []).filter((v) => v !== value);
    list.unshift(value);
    this.settings.recent[key] = list.slice(0, 15);
  }

  // -------------------------------------------------------------- listing

  private toRows(
    entries: Entry[],
    checked: boolean,
    keep?: Map<string, Row>,
  ): Row[] {
    return entries.map((e) => {
      const old = keep?.get(e.path);
      return {
        ...e,
        checked: old ? old.checked : checked,
        added: old ? old.added : this.added++,
        extra: old?.extra,
      };
    });
  }

  /** Lists a folder, replacing the list. */
  async openFolder(path: string) {
    this.folders = [path];
    this.selected.clear();
    await this.rescan(true);
    this.settings.lastFolders = [
      path,
      ...this.settings.lastFolders.filter((f) => f !== path),
    ].slice(0, 10);
    this.saveSettings();
    await this.maybeAutoload(path);
  }

  async rescan(fresh = false) {
    if (!this.folders.length) return;
    this.busy = true;
    try {
      const entries: Entry[] = await invoke("scan", {
        roots: $state.snapshot(this.folders),
        filters: $state.snapshot(this.filters),
      });
      const keep = fresh
        ? undefined
        : new Map(this.rows.map((r) => [r.path, r]));
      const extras = fresh
        ? []
        : this.rows.filter((r) => !this.folders.some((f) => r.root === f));
      const listed = this.toRows(
        entries,
        this.settings.autoCheckAfterListing,
        keep,
      );
      const seen = new Set(listed.map((r) => r.path));
      this.rows = [...listed, ...extras.filter((r) => !seen.has(r.path))];
      this.applySort();
      this.loadExtraColumns();
    } catch (e) {
      this.setError(e);
    } finally {
      this.busy = false;
    }
  }

  /** Adds dropped, pasted or picked paths. `replace` clears the list first. */
  async addPaths(paths: string[], replace = false) {
    if (!paths.length) return;
    try {
      const entries: Entry[] = await invoke("stat_paths", { paths });
      if (replace) {
        this.rows = [];
        this.folders = [];
        this.selected.clear();
      }
      const have = new Set(this.rows.map((r) => r.path));
      this.rows = [
        ...this.rows,
        ...this.toRows(
          entries.filter((e) => !have.has(e.path)),
          this.settings.autoCheckAfterDrop,
        ),
      ];
      this.applySort();
      this.loadExtraColumns();
      const dir =
        entries.length === 1 && entries[0].isDir
          ? entries[0].path
          : entries[0]?.root;
      if (dir) await this.maybeAutoload(dir);
    } catch (e) {
      this.setError(e);
    }
  }

  async refresh() {
    await invoke("refresh_paths", { paths: [] });
    if (this.folders.length) {
      await this.rescan();
    } else if (this.rows.length) {
      const keep = new Map(this.rows.map((r) => [r.path, r]));
      const entries: Entry[] = await invoke("stat_paths", {
        paths: this.rows.map((r) => r.path),
      });
      this.rows = this.toRows(entries, true, keep);
      this.applySort();
      this.loadExtraColumns();
    }
  }

  async maybeAutoload(dir: string) {
    if (!this.settings.autoload) return;
    const text: string | null = await invoke("find_autoload", { dir });
    if (text) {
      this.applyPresetText(text, null);
      this.say("Loaded _autoload.alterix from this folder.");
    }
  }

  async loadExtraColumns() {
    const kinds = this.settings.columns.filter((c) =>
      ["taken", "dimensions", "duration", "custom"].includes(c),
    );
    if (!kinds.length || !this.rows.length) return;
    const paths = this.rows.map((r) => r.path);
    try {
      const vals: Record<string, string>[] = await invoke("columns", {
        paths,
        kinds,
        custom: this.settings.customColumn,
      });
      const byPath = new Map(paths.map((p, i) => [p, vals[i]]));
      for (const r of this.rows) r.extra = byPath.get(r.path) ?? {};
    } catch (e) {
      this.setError(e);
    }
  }

  // -------------------------------------------------------------- sorting and list editing

  value(r: Row, col: ColumnId): string | number {
    switch (col) {
      case "name":
        return r.name;
      case "newName":
        return this.preview.get(r.path)?.newName ?? "";
      case "ext":
        return r.isDir
          ? ""
          : r.name.includes(".")
            ? r.name.slice(r.name.lastIndexOf(".") + 1)
            : "";
      case "size":
        return r.size;
      case "created":
        return r.created ?? "";
      case "modified":
        return r.modified ?? "";
      case "accessed":
        return r.accessed ?? "";
      case "attrs":
        return r.attrs;
      case "subdir":
        return r.subdir;
      case "pathLength":
        return r.path.length;
      default:
        return r.extra?.[col] ?? "";
    }
  }

  sortBy(col: ColumnId) {
    if (this.sort.col !== col) this.sort = { col, dir: 1 };
    else if (this.sort.dir === 1) this.sort = { col, dir: -1 };
    else this.sort = { col: null, dir: 1 };
    this.applySort();
  }

  applySort() {
    const { col, dir } = this.sort;
    const cmp = this.settings.naturalSort ? collator : plain;
    const affected = (r: Row) =>
      this.preview.get(r.path)?.status === "changed" ? 0 : 1;
    const rows = [...this.rows];
    rows.sort((a, b) => {
      if (!this.settings.foldersWithFiles && a.isDir !== b.isDir)
        return a.isDir ? -1 : 1;
      if (
        this.settings.groupAffected &&
        col &&
        !["size", "created", "modified", "accessed", "attrs"].includes(col)
      ) {
        const g = affected(a) - affected(b);
        if (g) return g;
      }
      if (!col) return a.added - b.added;
      const x = this.value(a, col);
      const y = this.value(b, col);
      const c =
        typeof x === "number" && typeof y === "number"
          ? x - y
          : cmp.compare(String(x), String(y));
      return c * dir || a.added - b.added;
    });
    this.rows = rows;
  }

  /** Freezes the current order as the "no sort" order (after moving rows by hand). */
  private freezeOrder() {
    this.sort = { col: null, dir: 1 };
    this.rows.forEach((r, i) => (r.added = i));
    this.added = this.rows.length;
  }

  move(where: "up" | "down" | "top" | "bottom") {
    const sel = this.rows.filter((r) => this.selected.has(r.path));
    if (!sel.length) return;
    const rest = this.rows.filter((r) => !this.selected.has(r.path));
    let out: Row[];
    if (where === "top") out = [...sel, ...rest];
    else if (where === "bottom") out = [...rest, ...sel];
    else {
      out = [...this.rows];
      const idxs = sel.map((r) => out.indexOf(r));
      if (where === "up") {
        for (const i of idxs)
          if (i > 0 && !this.selected.has(out[i - 1].path))
            [out[i - 1], out[i]] = [out[i], out[i - 1]];
      } else {
        for (const i of idxs.reverse())
          if (i < out.length - 1 && !this.selected.has(out[i + 1].path))
            [out[i + 1], out[i]] = [out[i], out[i + 1]];
      }
    }
    this.rows = out;
    this.freezeOrder();
  }

  swap() {
    const sel = this.rows.filter((r) => this.selected.has(r.path));
    if (sel.length !== 2) {
      this.say("Select exactly two items to swap.");
      return;
    }
    const out = [...this.rows];
    const a = out.indexOf(sel[0]);
    const b = out.indexOf(sel[1]);
    [out[a], out[b]] = [out[b], out[a]];
    this.rows = out;
    this.freezeOrder();
  }

  randomSort() {
    const out = [...this.rows];
    for (let i = out.length - 1; i > 0; i--) {
      const j = Math.floor(Math.random() * (i + 1));
      [out[i], out[j]] = [out[j], out[i]];
    }
    this.rows = out;
    this.freezeOrder();
  }

  removeSelected() {
    this.rows = this.rows.filter((r) => !this.selected.has(r.path));
    this.selected.clear();
  }

  clearList() {
    this.rows = [];
    this.folders = [];
    this.selected.clear();
    this.preview = new Map();
  }

  clearUnchecked() {
    this.rows = this.rows.filter((r) => r.checked);
  }

  showOnlyAffected() {
    this.rows = this.rows.filter(
      (r) => this.preview.get(r.path)?.status !== "unchanged" && r.checked,
    );
  }

  checkAll(v: boolean) {
    for (const r of this.rows) r.checked = v;
  }

  invertChecks() {
    for (const r of this.rows) r.checked = !r.checked;
  }

  // -------------------------------------------------------------- preview

  private previewSeq = 0;

  items() {
    return this.checkedRows.map((r) => ({
      path: r.path,
      isDir: r.isDir,
      root: r.root,
    }));
  }

  rules(): Rule[] {
    return this.steps
      .filter((s) => s.enabled)
      .map((s) => $state.snapshot(s.rule) as Rule);
  }

  async runPreview() {
    const seq = ++this.previewSeq;
    const items = this.items();
    if (!items.length) {
      this.preview = new Map();
      return;
    }
    const rules = this.rules();
    const options = $state.snapshot(this.options) as Options;
    options.sizeUnit = this.settings.sizeUnit;
    if (rules.some(usesClipboard)) {
      try {
        options.clipboard = (await readText()) ?? "";
      } catch {
        options.clipboard = "";
      }
    }
    this.previewing = true;
    try {
      const p: Preview = await invoke("preview", {
        req: { items, rules, options, trace: $state.snapshot(this.trace) },
      });
      if (seq !== this.previewSeq) return;
      this.preview = new Map(p.items.map((i) => [i.path, i]));
      this.nextNumbers = p.nextNumbers;
      this.error = null;
      if (this.sort.col === "newName" || this.settings.groupAffected)
        this.applySort();
    } catch (e) {
      if (seq === this.previewSeq) this.setError(e);
    } finally {
      if (seq === this.previewSeq) this.previewing = false;
    }
  }

  async debug(path: string) {
    this.trace = [path];
    await this.runPreview();
    this.dialogData = this.preview.get(path) ?? null;
    this.dialog = "debug";
    this.trace = [];
  }

  // -------------------------------------------------------------- rename and undo

  private riskyWarnings(): string[] {
    const o = this.options;
    const w: string[] = [];
    if (o.overwrite)
      w.push("Existing items with the same name will be moved to the trash.");
    if (o.allowFoldersInNames)
      w.push("Slashes in new names will create folders.");
    if (o.skipExisting)
      w.push("Items whose new name already exists will be skipped.");
    return w;
  }

  async rename(skip: string[] = []) {
    await this.runPreview();
    const c = this.counts;
    const items = [...this.preview.values()].filter(
      (p) => !skip.includes(p.path),
    );
    const changing = items.filter((p) => p.status === "changed").length;
    const timesOrAttrs =
      JSON.stringify(this.options.attributes) !==
        JSON.stringify(defaultOptions().attributes) ||
      JSON.stringify(this.options.timestamps) !==
        JSON.stringify(defaultOptions().timestamps);
    if (!changing && !timesOrAttrs) {
      this.say("Nothing to rename.");
      return;
    }
    if (c.conflict || c.invalid) {
      const go = await ask(
        `${c.conflict + c.invalid} item${c.conflict + c.invalid === 1 ? " has" : "s have"} a problem and will be left as they are. Rename the other ${changing}?`,
        {
          title: "Alterix",
          kind: "warning",
          okLabel: "Rename",
          cancelLabel: "Cancel",
        },
      );
      if (!go) return;
    }
    if (this.settings.confirmEach && !skip.length) {
      this.dialogData = items.filter((p) => p.status === "changed");
      this.dialog = "confirmEach";
      return;
    }
    const warnings = this.settings.warnRiskyOptions ? this.riskyWarnings() : [];
    if ((this.settings.warnBeforeRename || warnings.length) && !skip.length) {
      const copy = this.options.copy && this.options.newLocation.trim();
      const text = [
        `${copy ? "Copy" : "Rename"} ${changing} item${changing === 1 ? "" : "s"}?`,
        ...warnings,
        "You can undo this from History.",
      ].join("\n\n");
      const go = await ask(text, {
        title: "Alterix",
        kind: warnings.length ? "warning" : "info",
        okLabel: copy ? "Copy" : "Rename",
        cancelLabel: "Cancel",
      });
      if (!go) return;
    }
    await this.doRename(skip);
  }

  async doRename(skip: string[]) {
    this.busy = true;
    try {
      const options = $state.snapshot(this.options) as Options;
      options.sizeUnit = this.settings.sizeUnit;
      const rules = this.rules();
      if (rules.some(usesClipboard))
        options.clipboard = (await readText().catch(() => "")) ?? "";
      const items = this.items().filter((i) => !skip.includes(i.path));
      const report: Report = await invoke("rename", {
        req: {
          items,
          rules,
          options,
          reverse: this.settings.reverseOrder,
          log: this.settings.logActivity,
        },
      });
      await this.afterRename(report);
      if (this.settings.retainAutonumber) {
        for (const [idx, next] of this.nextNumbers) {
          const step = this.steps.filter((s) => s.enabled)[idx];
          if (step?.rule.kind === "numbering") step.rule.start = next;
        }
      }
      this.saveRecentFromSteps();
      if (report.failed) {
        this.report = report;
        this.dialog = "report";
      } else if (this.settings.confirmAfterRename) {
        await message(
          `${report.renamed} item${report.renamed === 1 ? "" : "s"} renamed.`,
          { title: "Alterix", kind: "info" },
        );
      } else {
        this.say(
          `${report.renamed} item${report.renamed === 1 ? "" : "s"} renamed. Press ${isMac() ? "⌘" : "Ctrl+"}Z to undo.`,
        );
      }
    } catch (e) {
      this.setError(e);
    } finally {
      this.busy = false;
    }
  }

  private async afterRename(report: Report) {
    const moved = new Map(
      report.results
        .filter((r) => r.ok && r.path !== r.newPath)
        .map((r) => [r.path, r.newPath]),
    );
    this.options.overrides = {};
    const renamedFolder = report.results.some(
      (r) =>
        r.ok &&
        r.path !== r.newPath &&
        this.rows.find((x) => x.path === r.path)?.isDir,
    );
    const copy = this.options.copy && this.options.newLocation.trim();
    if (this.folders.length && (renamedFolder || this.filters.subfolders)) {
      await this.rescan(true);
    } else if (!copy && moved.size) {
      const paths = this.rows.map((r) => moved.get(r.path) ?? r.path);
      const keep = new Map(
        this.rows.map((r) => [moved.get(r.path) ?? r.path, r]),
      );
      const entries: Entry[] = await invoke("stat_paths", { paths });
      this.rows = this.toRows(entries, true, keep).map((r) => ({
        ...r,
        root: keep.get(r.path)?.root ?? r.root,
      }));
      this.selected.clear();
      this.loadExtraColumns();
    }
    await this.runPreview();
    this.history = await invoke("history");
  }

  saveRecentFromSteps() {
    for (const s of this.steps) {
      for (const [k, v] of Object.entries(s.rule)) {
        if (
          typeof v === "string" &&
          k !== "kind" &&
          v &&
          ![
            "mode",
            "position",
            "source",
            "preset",
            "crop",
            "leadDots",
            "action",
            "target",
            "numType",
            "letterCase",
            "code",
          ].includes(k)
        ) {
          this.addRecent(`${s.rule.kind}.${k}`, v);
        }
      }
    }
    this.saveSettings();
  }

  async manualRename(path: string, newName: string) {
    try {
      const report: Report = await invoke("manual_rename", {
        path,
        newName,
        log: this.settings.logActivity,
      });
      if (report.failed)
        throw new Error(report.results[0]?.message ?? "Rename failed.");
      await this.afterRename(report);
    } catch (e) {
      this.setError(e);
    }
  }

  async loadHistory() {
    this.history = await invoke("history");
  }

  async undo(id?: string) {
    await this.loadHistory();
    const batch = id
      ? this.history.find((b) => b.id === id)
      : this.history.find((b) => !b.undone);
    if (!batch) {
      this.say("Nothing to undo.");
      return;
    }
    const go = await ask(`Undo “${batch.summary}” from ${batch.when}?`, {
      title: "Alterix",
      okLabel: "Undo",
      cancelLabel: "Cancel",
    });
    if (!go) return;
    this.busy = true;
    try {
      const report: Report = await invoke("undo", {
        id: batch.id,
        log: this.settings.logActivity,
      });
      await this.afterRename({ ...report, results: report.results });
      if (this.folders.length) await this.rescan();
      if (report.failed) {
        this.report = report;
        this.dialog = "report";
      } else
        this.say(
          `Undone: ${report.renamed} item${report.renamed === 1 ? "" : "s"} restored.`,
        );
    } catch (e) {
      this.setError(e);
    } finally {
      this.busy = false;
    }
  }

  // -------------------------------------------------------------- presets

  snapshot(withPath = false): PresetFile {
    const o = $state.snapshot(this.options) as Options;
    const {
      clipboard: _c,
      pairs: _p,
      overrides: _o,
      scriptAnswers: _s,
      ...options
    } = o;
    return {
      app: "Alterix",
      version: 1,
      steps: this.steps.map((s) => ({
        enabled: s.enabled,
        rule: $state.snapshot(s.rule) as Rule,
      })),
      options,
      filters: $state.snapshot(this.filters) as Filters,
      ...(withPath ? { folders: $state.snapshot(this.folders) } : {}),
    };
  }

  applyPreset(p: PresetFile, path: string | null) {
    this.steps = (p.steps ?? [])
      .filter((s) => s.rule && RULE_BY_KIND[s.rule.kind as RuleKind])
      .map((s) => ({
        id: this.nextId++,
        enabled: s.enabled !== false,
        collapsed: false,
        rule: { ...RULE_BY_KIND[s.rule.kind].defaults(), ...s.rule },
      }));
    const base = defaultOptions();
    this.options = {
      ...base,
      ...(p.options ?? {}),
      pairs: this.options.pairs,
      overrides: {},
      clipboard: "",
      scriptAnswers: this.options.scriptAnswers,
    } as Options;
    this.filters = { ...defaultFilters(), ...(p.filters ?? {}) };
    if (path) {
      this.presetPath = path;
      this.lastSaved = JSON.stringify(p);
    }
    if (p.folders?.length) {
      this.folders = p.folders;
      this.rescan(true);
    } else if (this.folders.length) {
      this.rescan();
    }
  }

  applyPresetText(text: string, path: string | null) {
    try {
      const p = JSON.parse(text) as PresetFile;
      if (p.app !== "Alterix")
        throw new Error("This is not an Alterix preset.");
      this.applyPreset(p, path);
    } catch (e) {
      this.setError(e);
    }
  }

  async openPreset(path: string) {
    this.log("info", "preset", `Opening preset ${path}`);
    const text: string = await invoke("read_text_file", { path });
    this.applyPresetText(text, path);
    this.settings.recentPresets = [
      path,
      ...this.settings.recentPresets.filter((p) => p !== path),
    ].slice(0, 12);
    this.saveSettings();
  }

  async savePreset(path: string) {
    const withPath =
      this.settings.storePathInPreset ||
      path.toLowerCase().endsWith(`.path.${PRESET_EXT}`);
    const snap = this.snapshot(withPath);
    await invoke("write_text_file", {
      path,
      text: JSON.stringify(snap, null, 2),
    });
    this.log(
      "info",
      "preset",
      `Saved preset ${path} (${snap.steps.length} steps)`,
    );
    this.presetPath = path;
    this.lastSaved = JSON.stringify(snap);
    this.settings.recentPresets = [
      path,
      ...this.settings.recentPresets.filter((p) => p !== path),
    ].slice(0, 12);
    this.saveSettings();
    this.say("Preset saved.");
  }

  async resetCriteria() {
    const text: string | null = await invoke("default_preset");
    if (text) this.applyPresetText(text, null);
    else this.clearCriteria();
  }

  clearCriteria() {
    this.steps = [];
    this.options = { ...defaultOptions(), pairs: [] };
    this.filters = defaultFilters();
  }

  revertCriteria() {
    if (this.lastSaved) this.applyPresetText(this.lastSaved, this.presetPath);
  }

  async saveAsDefault() {
    await invoke("save_default_preset", {
      text: JSON.stringify(this.snapshot(), null, 2),
    });
    this.say("Saved as the default criteria.");
  }

  // -------------------------------------------------------------- pairs and clipboard

  async importPairs(text: string) {
    const pairs: Pair[] = await invoke("parse_pairs", { text });
    if (!pairs.length) {
      this.say("No rename pairs found. Use one old|new pair per line.");
      return;
    }
    this.options.pairs = pairs;
    this.say(
      `${pairs.length} rename pair${pairs.length === 1 ? "" : "s"} imported.`,
    );
  }

  checkPairRows() {
    const keys = new Set(this.options.pairs.map((p) => p.find.toLowerCase()));
    for (const r of this.rows)
      r.checked =
        keys.has(r.name.toLowerCase()) || keys.has(r.path.toLowerCase());
  }

  async setNamesFromClipboard() {
    const text = ((await readText().catch(() => "")) ?? "")
      .split(/\r?\n/)
      .filter((l) => l.trim());
    if (!text.length) {
      this.say("The clipboard has no names.");
      return;
    }
    const targets = this.rows.filter((r) => this.selected.has(r.path));
    const list = targets.length ? targets : this.checkedRows;
    const map: Record<string, string> = {};
    list.forEach((r, i) => (map[r.path] = text[i % text.length].trim()));
    this.options.overrides = map;
  }

  async pastePaths(replace: boolean) {
    const text = (await readText().catch(() => "")) ?? "";
    const paths = text
      .split(/\r?\n/)
      .map((l) => l.trim().replace(/^"|"$/g, ""))
      .filter(Boolean);
    await this.addPaths(paths, replace);
  }

  async checkFromClipboard() {
    const text = ((await readText().catch(() => "")) ?? "")
      .split(/\r?\n/)
      .map((l) => l.trim().toLowerCase())
      .filter(Boolean);
    const set = new Set(text);
    for (const r of this.rows)
      r.checked =
        set.has(r.name.toLowerCase()) || set.has(r.path.toLowerCase());
  }

  async copyColumns(what: "names" | "paths" | "newNames" | "folders" | "all") {
    const rows = this.selected.size
      ? this.rows.filter((r) => this.selected.has(r.path))
      : this.rows;
    const lines = rows.map((r) => {
      const p = this.preview.get(r.path);
      switch (what) {
        case "names":
          return r.name;
        case "paths":
          return r.path;
        case "newNames":
          return p?.newName ?? r.name;
        case "folders":
          return r.path.slice(0, r.path.length - r.name.length - 1);
        default:
          return [
            r.name,
            p?.newName ?? "",
            r.size,
            r.modified ?? "",
            r.path,
          ].join("\t");
      }
    });
    if (what === "all")
      lines.unshift(
        ["Name", "New name", "Size", "Modified", "Path"].join("\t"),
      );
    await writeText(lines.join("\n"));
    this.say(`Copied ${rows.length} line${rows.length === 1 ? "" : "s"}.`);
  }

  // -------------------------------------------------------------- update check, logs, feedback

  async checkForUpdates() {
    if (this.updateChecking || !this.appInfo) return;
    this.updateChecking = true;
    const current = this.appInfo.version;
    const t = performance.now();
    try {
      const info = await check(current);
      this.log(
        "info",
        "update",
        `Update check (${Math.round(performance.now() - t)} ms): running ${current}, latest ${info.latest} (${info.kind})`,
      );
      if (info.kind === "newer") this.newerVersion = info.latest;
      this.dialogData = info;
    } catch (e) {
      const msg =
        e instanceof UpdateError
          ? e.message
          : `The check failed: ${e instanceof Error ? e.message : String(e)}`;
      this.log("warning", "update", `Update check failed: ${msg}`);
      this.dialogData = { error: msg };
    } finally {
      this.updateChecking = false;
    }
    this.dialog = "update";
  }

  async viewLogs() {
    let path = "";
    try {
      path = await invoke("log_path");
      await revealItemInDir(path);
    } catch {
      this.dialogData = path || this.dataStatus?.path || "";
      this.dialog = "logs";
    }
  }

  async reportProblem() {
    if (!this.appInfo) return;
    this.log("info", "feedback", "Report a Problem opened in the browser");
    await openUrl(this.appInfo.reportUrl);
  }
}

export function isMac(): boolean {
  return navigator.userAgent.includes("Mac");
}

export const app = new AppState();
