// Types shared with the Rust side (engine, listing, ops), plus their default values.

export type Status =
  "unchanged" | "changed" | "conflict" | "invalid" | "skipped";

export interface Entry {
  path: string;
  name: string;
  isDir: boolean;
  size: number;
  created: string | null;
  modified: string | null;
  accessed: string | null;
  attrs: string;
  root: string;
  subdir: string;
  depth: number;
}

/** A row in the file list: an entry plus UI state. */
export interface Row extends Entry {
  /** Included in the rename. */
  checked: boolean;
  /** Order the row was added in, for "no sort". */
  added: number;
  extra?: Record<string, string>;
}

export interface TraceStep {
  step: string;
  name: string;
}

export interface PreviewItem {
  path: string;
  original: string;
  newName: string;
  newPath: string;
  status: Status;
  message: string | null;
  newCreated: string | null;
  newModified: string | null;
  newAccessed: string | null;
  trace: TraceStep[] | null;
  alerts: string[];
}

export interface Preview {
  items: PreviewItem[];
  nextNumbers: [number, number][];
}

export interface OpResult {
  path: string;
  newPath: string;
  ok: boolean;
  message: string | null;
}

export interface Report {
  results: OpResult[];
  renamed: number;
  failed: number;
  batchId: string | null;
}

export interface UndoOp {
  from: string;
  to: string;
  isDir: boolean;
  copied: boolean;
}

export interface Batch {
  id: string;
  when: string;
  summary: string;
  ops: UndoOp[];
  undone: boolean;
}

export interface Pair {
  find: string;
  with: string;
}

export type Tri = "keep" | "set" | "clear";
export type StampMode =
  "keep" | "set" | "current" | "fromCreated" | "fromModified" | "fromTaken";

export interface StampSet {
  mode: StampMode;
  value: string;
  delta: string;
}

export interface Options {
  extensionsInName: boolean;
  folderExtensionsInName: boolean;
  preventDuplicates: boolean;
  duplicateFormat: string;
  duplicateStartOne: boolean;
  duplicateAcrossFolders: boolean;
  linkMaster: string;
  linkLinked: string;
  linkAcrossFolders: boolean;
  allowFoldersInNames: boolean;
  relaxValidation: boolean;
  overwrite: boolean;
  skipExisting: boolean;
  newLocation: string;
  copy: boolean;
  keepStructure: boolean;
  timestamps: {
    created: StampSet;
    modified: StampSet;
    accessed: StampSet;
    incrementSeconds: number;
  };
  attributes: { readOnly: Tri; hidden: Tri; system: Tri; archive: Tri };
  sizeUnit: string;
  clipboard: string;
  pairs: Pair[];
  overrides: Record<string, string>;
  scriptAnswers: Record<string, string>;
}

export interface Filters {
  mask: string;
  matchCase: boolean;
  regex: boolean;
  files: boolean;
  folders: boolean;
  hidden: boolean;
  subfolders: boolean;
  level: string;
  nameMin: number;
  nameMax: number;
  pathMin: number;
  pathMax: number;
  condition: string;
}

const stamp = (): StampSet => ({ mode: "keep", value: "", delta: "" });

export function defaultOptions(): Options {
  return {
    extensionsInName: false,
    folderExtensionsInName: false,
    preventDuplicates: false,
    duplicateFormat: "%name%_%count%",
    duplicateStartOne: false,
    duplicateAcrossFolders: false,
    linkMaster: "",
    linkLinked: "",
    linkAcrossFolders: false,
    allowFoldersInNames: false,
    relaxValidation: false,
    overwrite: false,
    skipExisting: false,
    newLocation: "",
    copy: true,
    keepStructure: false,
    timestamps: {
      created: stamp(),
      modified: stamp(),
      accessed: stamp(),
      incrementSeconds: 0,
    },
    attributes: {
      readOnly: "keep",
      hidden: "keep",
      system: "keep",
      archive: "keep",
    },
    sizeUnit: "auto",
    clipboard: "",
    pairs: [],
    overrides: {},
    scriptAnswers: {},
  };
}

export function defaultFilters(): Filters {
  return {
    mask: "*",
    matchCase: false,
    regex: false,
    files: true,
    folders: false,
    hidden: false,
    subfolders: false,
    level: "",
    nameMin: 0,
    nameMax: 0,
    pathMin: 0,
    pathMax: 0,
    condition: "",
  };
}

export type ColumnId =
  | "name"
  | "newName"
  | "ext"
  | "size"
  | "created"
  | "modified"
  | "accessed"
  | "attrs"
  | "subdir"
  | "pathLength"
  | "taken"
  | "dimensions"
  | "duration"
  | "custom";

export const COLUMNS: { id: ColumnId; label: string; extra?: boolean }[] = [
  { id: "name", label: "Name" },
  { id: "newName", label: "New name" },
  { id: "ext", label: "Extension" },
  { id: "size", label: "Size" },
  { id: "created", label: "Created" },
  { id: "modified", label: "Modified" },
  { id: "accessed", label: "Accessed" },
  { id: "attrs", label: "Attributes" },
  { id: "subdir", label: "Subfolder" },
  { id: "pathLength", label: "Path length" },
  { id: "taken", label: "Date taken", extra: true },
  { id: "dimensions", label: "Dimensions", extra: true },
  { id: "duration", label: "Duration", extra: true },
  { id: "custom", label: "Custom", extra: true },
];

/** User preferences stored in Alterix-data/settings.json. */
export interface Settings {
  theme: "system" | "light" | "dark";
  fontSize: number;
  monospaceList: boolean;
  gridlines: boolean;
  alwaysOnTop: boolean;
  highlightChanges: boolean;
  showPreviewPane: boolean;
  wideList: boolean;
  autoRefresh: boolean;
  naturalSort: boolean;
  groupAffected: boolean;
  foldersWithFiles: boolean;
  columns: ColumnId[];
  customColumn: string;
  sizeUnit: string;
  autoCheckAfterListing: boolean;
  autoCheckAfterDrop: boolean;
  enterOpensFile: boolean;
  allowMultipleInstances: boolean;
  autoload: boolean;
  recallLastCriteria: boolean;
  openLastFolder: boolean;
  saveRecentEntries: boolean;
  retainAutonumber: boolean;
  reverseOrder: boolean;
  logActivity: boolean;
  warnBeforeRename: boolean;
  confirmAfterRename: boolean;
  confirmEach: boolean;
  warnRiskyOptions: boolean;
  autoSavePreset: boolean;
  storePathInPreset: boolean;
  recent: Record<string, string[]>;
  recentPresets: string[];
  lastFolders: string[];
  lastCriteria: unknown;
}

export function defaultSettings(): Settings {
  return {
    theme: "system",
    fontSize: 13,
    monospaceList: false,
    gridlines: false,
    alwaysOnTop: false,
    highlightChanges: true,
    showPreviewPane: false,
    wideList: false,
    autoRefresh: true,
    naturalSort: true,
    groupAffected: false,
    foldersWithFiles: false,
    columns: ["name", "newName", "size", "modified"],
    customColumn: "",
    sizeUnit: "auto",
    autoCheckAfterListing: true,
    autoCheckAfterDrop: true,
    enterOpensFile: false,
    allowMultipleInstances: false,
    autoload: true,
    recallLastCriteria: true,
    openLastFolder: false,
    saveRecentEntries: true,
    retainAutonumber: false,
    reverseOrder: false,
    logActivity: false,
    warnBeforeRename: true,
    confirmAfterRename: false,
    confirmEach: false,
    warnRiskyOptions: true,
    autoSavePreset: false,
    storePathInPreset: false,
    recent: {},
    recentPresets: [],
    lastFolders: [],
    lastCriteria: null,
  };
}
