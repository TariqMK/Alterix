// Retakes the README screenshots from the real Alterix interface, with made-up files only.
// It builds the UI, serves it locally, replaces the Tauri backend with sample data, sends every
// preview to the real rename engine (src-tauri/examples/preview_json.rs) and saves each view in
// light and dark at 2x into assets/screenshots/. Works the same on Windows, macOS and Linux.
//
// Run from the project root (needs Rust, Node and `npx playwright install chromium` once):
//   node assets/make_screenshots.mjs

import { execFileSync, spawnSync } from "node:child_process";
import { mkdirSync } from "node:fs";
import { join, resolve } from "node:path";
import { chromium } from "playwright";
import { preview as vitePreview } from "vite";

const ROOT = resolve(import.meta.dirname, "..");
const OUT = join(ROOT, "assets", "screenshots");
const WIDTH = 1120;
const HEIGHT = 720;
// The newest release tag. No release exists yet, so the first planned version is shown.
const VERSION = "v1.0.0";

// ---------------------------------------------------------------- sample data (all made up)

const FOLDER = "/Users/alex/Pictures/Holiday 2025";
const FILES = [
  ["IMG_4821.JPG", 4_182_331, "2025-06-14 09:12:33"],
  ["IMG_4822.JPG", 3_904_512, "2025-06-14 09:13:02"],
  ["IMG_4823.JPG", 4_420_118, "2025-06-14 11:47:51"],
  ["IMG_4825.JPG", 3_766_904, "2025-06-14 12:05:19"],
  ["IMG_4830.JPG", 5_011_276, "2025-06-15 08:31:40"],
  ["IMG_4831.JPG", 4_870_330, "2025-06-15 08:32:12"],
  ["DSC_0102.JPG", 6_204_877, "2025-06-15 16:20:05"],
  ["DSC_0107.JPG", 6_011_430, "2025-06-15 16:24:48"],
  ["tram 28.heic", 2_310_662, "2025-06-16 10:02:11"],
  ["VID_0007.MOV", 48_220_901, "2025-06-16 19:44:36"],
];
const ENTRIES = FILES.map(([name, size, modified]) => ({
  path: `${FOLDER}/${name}`,
  name,
  isDir: false,
  size,
  created: modified,
  modified,
  accessed: modified,
  attrs: "",
  root: FOLDER,
  subdir: "",
  depth: 0,
}));

const PRESET = {
  app: "Alterix",
  version: 1,
  steps: [
    { enabled: true, rule: { kind: "name", mode: "fixed", value: "Lisbon" } },
    {
      enabled: true,
      rule: {
        kind: "numbering",
        position: "suffix",
        at: 1,
        start: 1,
        step: 1,
        pad: 3,
        separator: " ",
        breakAt: 0,
        perFolder: false,
        numType: "decimal",
        base: 16,
        letterCase: "default",
      },
    },
    { enabled: true, rule: { kind: "extension", mode: "lower", value: "" } },
  ],
  options: {},
  filters: {
    mask: "*.jpg *.heic *.mov",
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
  },
};

const HISTORY = [
  {
    id: "2",
    when: "2025-06-16 20:15:42",
    summary: "10 items renamed",
    undone: false,
    ops: [],
  },
  {
    id: "1",
    when: "2025-06-16 20:11:08",
    summary: "10 items renamed",
    undone: true,
    ops: [],
  },
];
const RECENT_PRESETS = [
  "/Users/alex/Documents/Presets/Holiday photos.alterix",
  "/Users/alex/Documents/Presets/Music by track.alterix",
  "/Users/alex/Documents/Presets/Scans by date.alterix",
];

// ---------------------------------------------------------------- the real engine

function buildEngine() {
  execFileSync("cargo", ["build", "--quiet", "--example", "preview_json"], {
    cwd: join(ROOT, "src-tauri"),
    stdio: "inherit",
  });
  const exe =
    process.platform === "win32" ? "preview_json.exe" : "preview_json";
  return join(ROOT, "src-tauri", "target", "debug", "examples", exe);
}

function runEngine(exe, req) {
  const r = spawnSync(exe, [], {
    input: JSON.stringify(req),
    encoding: "utf8",
  });
  if (r.status !== 0) throw new Error(`Engine failed: ${r.stderr}`);
  return JSON.parse(r.stdout);
}

// ---------------------------------------------------------------- the fake Tauri backend

function installBackend(data) {
  const callbacks = new Map();
  let next = 1;
  let launched = false;
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
  window.__TAURI_INTERNALS__ = {
    metadata: {
      currentWindow: { label: "main" },
      currentWebview: { windowLabel: "main", label: "main" },
    },
    transformCallback(cb) {
      const id = next++;
      callbacks.set(id, cb);
      return id;
    },
    unregisterCallback: (id) => callbacks.delete(id),
    convertFileSrc: (p) => p,
    async invoke(cmd, args) {
      switch (cmd) {
        case "data_status":
          return {
            path: "/Users/alex/Applications/Alterix-data",
            writable: true,
          };
        case "load_settings":
          return data.settings;
        case "app_info":
          return {
            version: data.version,
            buildKind: "release",
            os: "macOS 26.1",
            arch: "aarch64",
            reportUrl: "https://example.com",
          };
        case "launch_args":
          if (launched) return [];
          launched = true;
          return [data.folder];
        case "stat_paths":
          return args.paths.map((p) => ({
            path: p,
            name: p.split("/").pop(),
            isDir: true,
            size: 0,
            created: null,
            modified: null,
            accessed: null,
            attrs: "D",
            root: "/Users/alex/Pictures",
            subdir: "",
            depth: 0,
          }));
        case "scan":
          return data.entries;
        case "preview":
          return JSON.parse(await window.__engine(JSON.stringify(args.req)));
        case "history":
          return data.history;
        case "columns":
          return args.paths.map(() => ({}));
        case "default_preset":
        case "find_autoload":
        case "image_preview":
          return null;
        case "plugin:window|available_monitors":
          return [];
        default:
          return null;
      }
    },
  };
}

// ---------------------------------------------------------------- capture

async function main() {
  const exe = buildEngine();
  execFileSync(
    process.platform === "win32" ? "npm.cmd" : "npm",
    ["run", "build", "--silent"],
    { cwd: ROOT, stdio: "inherit", shell: process.platform === "win32" },
  );
  const server = await vitePreview({
    root: ROOT,
    preview: { port: 0, strictPort: false, open: false },
    logLevel: "silent",
  });
  const url = server.resolvedUrls.local[0];
  mkdirSync(OUT, { recursive: true });

  const settings = {
    recallLastCriteria: true,
    lastCriteria: PRESET,
    columns: ["name", "newName", "size", "modified"],
    autoCheckAfterListing: true,
    highlightChanges: true,
    recentPresets: RECENT_PRESETS,
  };
  const data = {
    settings,
    folder: FOLDER,
    entries: ENTRIES,
    history: HISTORY,
    version: VERSION,
  };
  const browser = await chromium.launch();
  const views = [];

  for (const scheme of ["light", "dark"]) {
    const context = await browser.newContext({
      viewport: { width: WIDTH, height: HEIGHT },
      deviceScaleFactor: 2,
      colorScheme: scheme,
    });
    const page = await context.newPage();
    await page.exposeFunction("__engine", (json) =>
      JSON.stringify(runEngine(exe, JSON.parse(json))),
    );
    await page.addInitScript(installBackend, data);

    const fresh = async () => {
      await page.goto(url);
      await page.getByText("Lisbon 001.jpg").waitFor();
      // Leave one photo out, to show an excluded item.
      await page.getByLabel("Include IMG_4825.JPG").click();
      await page.getByText("Lisbon 004.jpg").waitFor();
      await page.mouse.move(0, HEIGHT - 1);
    };
    const shot = async (name) => {
      await page.waitForTimeout(250);
      const file = join(OUT, `${name}-${scheme}.png`);
      await page.screenshot({ path: file });
      views.push(file);
    };

    await fresh();
    await shot("main");

    await page.getByRole("button", { name: "+ Add step" }).click();
    await shot("add-step");

    await fresh();
    await page.getByRole("button", { name: "Insert a tag" }).first().click();
    await shot("tags");

    await fresh();
    await page.getByRole("button", { name: /^Filters/ }).click();
    await shot("filters");

    await fresh();
    await page.getByRole("button", { name: /After renaming/ }).click();
    await page
      .getByLabel("Folder (absolute, or relative such as .. or Sorted)")
      .fill("/Users/alex/Pictures/Sorted");
    await page
      .locator(".after .pipeline, .pipeline")
      .first()
      .evaluate((el) => el.scrollTo(0, el.scrollHeight));
    await shot("after-renaming");

    await fresh();
    await page.getByRole("button", { name: "History" }).click();
    await shot("history");

    await fresh();
    await page.getByRole("button", { name: /^Presets/ }).click();
    await shot("presets");

    await context.close();
  }
  await browser.close();
  await new Promise((done) => server.httpServer.close(done));
  for (const v of views) console.log(`Wrote ${v.slice(ROOT.length + 1)}`);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
