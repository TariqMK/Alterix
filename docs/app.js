// Alterix product page: live previews (hero window, steps, tags, undo) built from small models, plus OS-aware download links.

const REPO = "TariqMK/Alterix";
const ASSET = {
  windows: /windows-x64\.exe$/i,
  macos: /macos-arm64\.zip$/i,
  linux: /linux-x64\.AppImage$/i,
};
const OS_NAME = { windows: "Windows", macos: "macOS", linux: "Linux" };
const REDUCED = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
const IS_MAC = /mac/i.test(
  navigator.userAgentData?.platform || navigator.platform || "",
);
const MOD = IS_MAC ? "⌘" : "Ctrl+";

const $ = (id) => document.getElementById(id);
const esc = (s) =>
  s.replace(
    /[&<>"]/g,
    (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c],
  );

// The app's highlight: common start and end stay plain, the changed middle is marked.
function diff(a, b) {
  let i = 0;
  while (i < a.length && i < b.length && a[i] === b[i]) i++;
  let j = 0;
  while (
    j < a.length - i &&
    j < b.length - i &&
    a[a.length - 1 - j] === b[b.length - 1 - j]
  )
    j++;
  return `${esc(b.slice(0, i))}<mark>${esc(b.slice(i, b.length - j))}</mark>${esc(b.slice(b.length - j))}`;
}

const splitExt = (n) => {
  const d = n.lastIndexOf(".");
  return d > 0 ? [n.slice(0, d), n.slice(d + 1)] : [n, ""];
};

/* ---------- Hero: the Alterix window ---------- */

const FILES = [
  ["IMG_4821.JPG", 4.0, "2025-06-14 09:12:33"],
  ["IMG_4822.JPG", 3.7, "2025-06-14 09:13:02"],
  ["IMG_4823.JPG", 4.2, "2025-06-14 11:47:51"],
  ["IMG_4825.JPG", 3.6, "2025-06-14 12:05:19", false],
  ["IMG_4830.JPG", 4.8, "2025-06-15 08:31:40"],
  ["IMG_4831.JPG", 4.6, "2025-06-15 08:32:12"],
  ["DSC_0102.JPG", 5.9, "2025-06-15 16:20:05"],
  ["DSC_0107.JPG", 5.7, "2025-06-15 16:24:48"],
  ["tram 28.heic", 2.2, "2025-06-16 10:02:11"],
  ["VID_0007.MOV", 46.0, "2025-06-16 19:44:36"],
];

const hero = {
  rows: FILES.map(([name, mb, date, checked = true]) => ({
    name,
    orig: name,
    mb,
    date,
    checked,
  })),
  steps: { name: true, numbers: true, ext: true },
  nameText: "Lisbon",
  typing: false,
  focus: null,
  sel: "name", // The step shown in the editor column.
  msg: "",
  pressed: false,
};

function preview() {
  const s = hero.steps;
  let n = 0;
  const out = hero.rows.map((r) => {
    if (!r.checked) return null;
    let [base, ext] = splitExt(r.name);
    if (s.name && hero.nameText) base = hero.nameText;
    if (s.numbers) base = `${base} ${String(++n).padStart(3, "0")}`;
    if (s.ext) ext = ext.toLowerCase();
    return ext ? `${base}.${ext}` : base;
  });
  const seen = {};
  out.forEach(
    (x) => x && (seen[x.toLowerCase()] = (seen[x.toLowerCase()] || 0) + 1),
  );
  return out.map((x, i) => {
    if (x == null) return { status: "off" };
    if (seen[x.toLowerCase()] > 1) return { status: "conflict", name: x };
    return { status: x === hero.rows[i].name ? "same" : "changed", name: x };
  });
}

function renderHero() {
  if (hero.focus) hero.sel = hero.focus;
  const p = preview();
  $("w-rows").innerHTML = hero.rows
    .map((r, i) => {
      const q = p[i];
      const arrow = '<span class="arrow">→</span>';
      let nn = "";
      if (q.status === "changed") nn = arrow + diff(r.name, q.name);
      else if (q.status === "same") nn = esc(q.name);
      else if (q.status === "conflict")
        nn = `${arrow}${esc(q.name)}<span class="flag">Conflict</span>`;
      return `<div class="w-row ${q.status}">
        <span class="c-check"><span class="box${r.checked ? " on" : ""}"></span></span>
        <span class="c-name">${esc(r.name)}</span>
        <span class="c-new">${nn}</span>
        <span class="c-size">${r.mb.toFixed(1)} MB</span>
        <span class="c-date">${r.date}</span>
      </div>`;
    })
    .join("");

  const included = hero.rows.filter((r) => r.checked).length;
  const changed = p.filter((q) => q.status === "changed").length;
  const conflicts = p.filter((q) => q.status === "conflict").length;
  $("w-counts").innerHTML =
    `${included} of ${hero.rows.length} included · ${changed} will change` +
    (conflicts ? ` · <span class="bad">${conflicts} conflicts</span>` : "");
  renderEditor();
  const btn = $("w-rename");
  btn.textContent = `Rename ${changed}`;
  btn.classList.toggle("idle", changed === 0);
  btn.classList.toggle("pressed", hero.pressed);
  $("w-msg").textContent = hero.msg;

  document.querySelectorAll(".ritem[data-step]").forEach((el) => {
    const on = hero.steps[el.dataset.step];
    el.classList.toggle("off", !on);
    el.classList.toggle("sel", hero.sel === el.dataset.step);
    const sum = el.querySelector(".rsum");
    sum.textContent = on ? SUMMARY[el.dataset.step]() : "Off";
  });
}

// What each step's row in the step list says, as in the app.
const SUMMARY = {
  name: () => hero.nameText || "Set a new name",
  numbers: () => "001",
  ext: () => "lower case",
};

const field = (label, value, id = "") =>
  `<div class="efield"><span class="flabel">${label}</span><span class="input"${id ? ` id="${id}"` : ""}>${esc(value)}</span></div>`;
const EDITOR = {
  name: [
    "1",
    "Name",
    () =>
      field("Action", "Set a new name") +
      field("New name", hero.nameText, "w-name"),
  ],
  numbers: [
    "2",
    "Numbers",
    () =>
      `<div class="split">${field("Position", "After the name")}${field("Digits", "3")}</div>` +
      field("Start", "1"),
  ],
  ext: ["3", "Extension", () => field("Action", "lower case")],
};

// The editor column shows the selected step, like the app.
function renderEditor() {
  const [num, label, body] = EDITOR[hero.sel];
  const on = hero.steps[hero.sel];
  $("w-editor").innerHTML =
    `<div class="ehead"><span class="num">${num}</span><span class="ename">${label}</span><span class="eon"><span class="box${on ? " on" : ""}"></span>${on ? "On" : "Off"}</span></div>
    <div class="efields${on ? "" : " disabled"}">${body()}</div>`;
  $("w-name")?.classList.toggle("typing", hero.typing);
}

function doRename() {
  const p = preview();
  hero.rows.forEach((r, i) => {
    if (p[i].status === "changed") r.name = p[i].name;
  });
  const n = p.filter((q) => q.status === "changed").length;
  hero.msg = `${n} items renamed. Press ${MOD}Z to undo.`;
}

function doUndo() {
  const n = hero.rows.filter((r) => r.name !== r.orig).length;
  hero.rows.forEach((r) => (r.name = r.orig));
  hero.msg = `Undone: ${n} items restored.`;
}

// One story on a loop: switch steps off and on, cause and fix a clash, rename, undo, then retype the name.
const typeTo = (text) => () => (
  (hero.nameText = text),
  (hero.typing = true),
  (hero.focus = "name")
);
const SCRIPT = {
  3: () => ((hero.focus = "ext"), (hero.steps.ext = false)),
  6: () => ((hero.focus = "numbers"), (hero.steps.numbers = false)),
  10: () => (hero.steps.numbers = true),
  13: () => ((hero.focus = "ext"), (hero.steps.ext = true)),
  15: () => ((hero.focus = null), (hero.pressed = true)),
  16: () => ((hero.pressed = false), doRename()),
  23: doUndo,
  28: () => ((hero.msg = ""), typeTo("P")()),
  29: typeTo("Por"),
  30: typeTo("Porto"),
  32: () => ((hero.typing = false), (hero.focus = null)),
  38: typeTo("L"),
  39: typeTo("Lis"),
  40: typeTo("Lisb"),
  41: typeTo("Lisbon"),
  43: () => ((hero.typing = false), (hero.focus = null)),
};
const HERO_LOOP = 48;
let heroTick = 0;
function stepHero() {
  heroTick = (heroTick + 1) % HERO_LOOP;
  SCRIPT[heroTick]?.();
  renderHero();
}

/* ---------- Feature: steps pipeline (visitor-controlled) ---------- */

const PIPE_FILES = [
  "meeting_notes_FINAL copy.docx",
  "budget_2026_v2 copy.xlsx",
  "team_photo_DRAFT.png",
  "invoice_march copy.pdf",
  "client_brief_final.docx",
];
const titleCase = (s) =>
  s.toLowerCase().replace(/(^|\s)(\S)/g, (m, a, b) => a + b.toUpperCase());
const PIPE_STEPS = [
  (b) => b.replace(/\s*\bcopy\b\s*/gi, " ").trim(),
  (b) => b.replace(/_/g, " "),
  (b) => titleCase(b),
  (b, i) => `${String(i + 1).padStart(2, "0")} ${b}`,
];
function renderPipe() {
  const on = [...document.querySelectorAll("#pipe-steps input")].map(
    (x) => x.checked,
  );
  $("pipe-rows").innerHTML = PIPE_FILES.map((f, i) => {
    let [b, ext] = splitExt(f);
    PIPE_STEPS.forEach((fn, k) => on[k] && (b = fn(b, i)));
    const n = `${b}.${ext}`;
    const cell =
      n === f
        ? `<span class="muted">${esc(n)}</span>`
        : `<span class="chg">${diff(f, n)}</span>`;
    return `<div class="prow"><span>${esc(f)}</span>${cell}</div>`;
  }).join("");
}

/* ---------- Feature: tags from file data ---------- */

const TAG_SETS = [
  {
    tpl: "<($exif:DateTimeOriginal){YYYYMMDD_hhmmss}> <(exif:Model)>",
    rows: [
      ["IMG_4821.JPG", "20250614_091233 X-T5.JPG"],
      ["IMG_4822.JPG", "20250614_091302 X-T5.JPG"],
      ["DSC_0102.JPG", "20250615_162005 ILCE-7M4.JPG"],
      ["DSC_0107.JPG", "20250615_162448 ILCE-7M4.JPG"],
    ],
  },
  {
    tpl: "<(#Track){pad0:2}> <(Artist)> - <(Title)>",
    rows: [
      ["track01.mp3", "01 Nadia Vale - Low Tide.mp3"],
      ["track02.mp3", "02 Nadia Vale - Northbound.mp3"],
      ["track03.mp3", "03 Nadia Vale - Glass Harbour.mp3"],
      ["track04.mp3", "04 Nadia Vale - Last Ferry.mp3"],
    ],
  },
  {
    tpl: "<(PDF:Title)>",
    rows: [
      ["scan_0041.pdf", "Lease agreement 2026.pdf"],
      ["scan_0042.pdf", "Boiler service report.pdf"],
      ["download (3).pdf", "Train tickets Leeds to York.pdf"],
      ["document.pdf", "Garden plan, spring.pdf"],
    ],
  },
];
let tagIndex = 0;
function renderTags() {
  const set = TAG_SETS[tagIndex];
  $("tag-tpl").textContent = set.tpl;
  $("tag-rows").innerHTML = set.rows
    .map(
      ([a, b]) =>
        `<div class="trow"><span>${esc(a)}</span><span class="chg">${diff(a, b)}</span></div>`,
    )
    .join("");
  document
    .querySelectorAll("#tag-tabs button")
    .forEach((el) => el.classList.toggle("on", +el.dataset.t === tagIndex));
}
function showTags(i) {
  if (i === tagIndex) return;
  const rows = $("tag-rows");
  if (REDUCED) {
    tagIndex = i;
    renderTags();
    return;
  }
  rows.classList.add("fade");
  setTimeout(() => {
    tagIndex = i;
    renderTags();
    rows.classList.remove("fade");
  }, 250);
}
let tagTick = 0;
let tagHold = 0;
function stepTags() {
  if (tagHold > 0) return void tagHold--;
  if (++tagTick % 5 === 0) showTags((tagIndex + 1) % TAG_SETS.length);
}

/* ---------- Feature: undo from History ---------- */

const HIST_ITEMS = [
  ["IMG_4821.JPG", "Lisbon 001.jpg"],
  ["IMG_4822.JPG", "Lisbon 002.jpg"],
  ["tram 28.heic", "Lisbon 008.heic"],
];
const hist = { undone: false, pressed: false };
function renderHist() {
  $("hist-items").innerHTML = HIST_ITEMS.map(([oldN, newN]) => {
    const [from, to] = hist.undone ? [newN, oldN] : [oldN, newN];
    return `<div class="hrow"><span class="muted">${esc(from)}</span><span class="arrow">→</span><span class="chg">${diff(from, to)}</span></div>`;
  }).join("");
  $("hist-a-meta").textContent =
    `2025-06-16 20:15:42${hist.undone ? " · undone" : ""}`;
  $("hist-a").classList.toggle("hist-dim", hist.undone);
  const u = $("hist-undo");
  u.classList.toggle("hbtn-off", hist.undone);
  u.classList.toggle("pressed", hist.pressed);
  const msg = $("hist-msg");
  msg.textContent = hist.undone ? "Undone: 10 items restored." : " ";
}
let histTick = 0;
function stepHist() {
  histTick = (histTick + 1) % 12;
  if (histTick === 5) hist.pressed = true;
  if (histTick === 6) ((hist.pressed = false), (hist.undone = true));
  if (histTick === 0) hist.undone = false;
  renderHist();
}

/* ---------- Timer ---------- */

let timer = null;
function tick() {
  stepHero();
  stepTags();
  stepHist();
}
function startTimer() {
  if (!REDUCED && !timer && !document.hidden) timer = setInterval(tick, 1000);
}
function stopTimer() {
  clearInterval(timer);
  timer = null;
}

/* ---------- Downloads ---------- */

function detectOS() {
  const platform = (
    navigator.userAgentData?.platform ||
    navigator.platform ||
    ""
  ).toLowerCase();
  const ua = navigator.userAgent.toLowerCase();
  if (/android|iphone|ipad|ipod/.test(ua)) return null; // Phones and tablets get no direct download.
  if (platform.includes("win")) return "windows";
  if (platform.includes("mac")) return "macos";
  if (platform.includes("linux")) return "linux";
  return null;
}

async function wireDownloads() {
  const os = detectOS();
  const heroBtn = $("hero-download");
  if (os) {
    heroBtn.textContent = `Download for ${OS_NAME[os]}`;
    document
      .querySelector(`.platform[data-os="${os}"]`)
      ?.classList.add("detected");
  }
  try {
    const res = await fetch(
      `https://api.github.com/repos/${REPO}/releases/latest`,
      {
        headers: { Accept: "application/vnd.github+json" },
      },
    );
    if (!res.ok) return;
    const rel = await res.json();
    for (const [key, pattern] of Object.entries(ASSET)) {
      const asset = rel.assets?.find((a) => pattern.test(a.name));
      if (!asset) continue;
      const card = document.querySelector(`.platform[data-os="${key}"]`);
      card.querySelector(".dl").href = asset.browser_download_url;
      card.querySelector(".size").textContent =
        `${(asset.size / 1048576).toFixed(0)} MB`;
      if (key === os) heroBtn.href = asset.browser_download_url;
    }
    const date = new Date(rel.published_at).toLocaleDateString("en-GB", {
      day: "numeric",
      month: "short",
      year: "numeric",
    });
    $("dl-version").textContent = `${rel.tag_name} · released ${date}`;
    $("hero-meta").textContent =
      `${rel.tag_name} · Free · Open source · Offline · No tracking`;
    $("w-ver").textContent = rel.tag_name;
  } catch {
    // Offline, rate-limited (60 requests an hour) or no release yet: change nothing.
  }
}

/* ---------- Start ---------- */

renderHero();
renderPipe();
renderTags();
renderHist();
document
  .querySelectorAll("#pipe-steps input")
  .forEach((x) => x.addEventListener("change", renderPipe));
document.querySelectorAll("#tag-tabs button").forEach((el) =>
  el.addEventListener("click", () => {
    tagHold = 3; // Let the visitor read the set they picked before the cycle moves on.
    tagTick = 0;
    showTags(+el.dataset.t);
  }),
);
document.addEventListener("visibilitychange", () =>
  document.hidden ? stopTimer() : startTimer(),
);
startTimer();
wireDownloads();
