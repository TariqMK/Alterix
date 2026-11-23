<!-- Main window: toolbar and menus, the step list and editor, the file list with live
     preview, the status bar, keyboard shortcuts, drag and drop and startup. -->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { availableMonitors, getCurrentWindow } from "@tauri-apps/api/window";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { openPath } from "@tauri-apps/plugin-opener";
  import { onMount } from "svelte";
  import Dialogs from "./lib/Dialogs.svelte";
  import Icon from "./lib/Icon.svelte";
  import FileList from "./lib/FileList.svelte";
  import Pipeline from "./lib/Pipeline.svelte";
  import { app, isMac, PRESET_EXT } from "./lib/state.svelte";

  let menu = $state<"list" | "presets" | null>(null);
  let list: FileList | undefined = $state();
  let modHeld = false;
  let slot: HTMLSpanElement | undefined = $state();
  let versionPos = $state({ left: 0, top: 0 });

  // The version button sits after the main content in tab order, but is drawn
  // next to the title: it follows the position of an invisible slot in the header.
  $effect(() => {
    if (!slot) return;
    const place = () =>
      slot &&
      (versionPos = {
        left: slot.offsetLeft,
        top: slot.offsetTop + slot.offsetHeight / 2,
      });
    place();
    const ro = new ResizeObserver(place);
    ro.observe(document.body);
    return () => ro.disconnect();
  });
  const version = $derived(app.appInfo?.version ?? "");
  const versionTip = $derived(
    app.newerVersion
      ? `${app.newerVersion} is available. Click for details.`
      : "Check for updates",
  );
  const mod = isMac() ? "⌘" : "Ctrl+";

  const folderLabel = $derived(
    app.folders.length
      ? app.folders[0]
      : app.rows.length
        ? `${app.rows.length} items`
        : "No folder open",
  );
  const presetName = $derived(
    app.presetPath ? app.presetPath.split(/[\\/]/).pop() : null,
  );

  // Live preview: re-run whenever the included items, steps or options change.
  $effect(() => {
    const key = JSON.stringify([
      app.rows.map((r) => (r.checked ? r.path : "")),
      $state.snapshot(app.steps).map((s) => (s.enabled ? s.rule : null)),
      $state.snapshot(app.options),
    ]);
    if (!app.ready || !app.settings.autoRefresh) return;
    void key;
    const t = setTimeout(() => app.runPreview(), 120);
    return () => clearTimeout(t);
  });

  // Remember the criteria for next start.
  $effect(() => {
    const key = JSON.stringify([
      $state.snapshot(app.steps),
      $state.snapshot(app.filters),
    ]);
    if (!app.ready) return;
    void key;
    const t = setTimeout(() => app.rememberCriteria(), 1500);
    return () => clearTimeout(t);
  });

  // Display settings.
  $effect(() => {
    const root = document.documentElement;
    if (app.settings.theme === "system") delete root.dataset.theme;
    else root.dataset.theme = app.settings.theme;
    root.style.setProperty("--font-size", `${app.settings.fontSize}px`);
    root.style.setProperty("--list-size", `${app.settings.fontSize}px`);
  });
  $effect(() => {
    const on = app.settings.alwaysOnTop;
    if (app.ready)
      getCurrentWindow()
        .setAlwaysOnTop(on)
        .catch(() => {});
  });
  $effect(() => {
    const f = app.folders[0];
    const title = f ? `${f.split(/[\\/]/).pop()} — Alterix` : "Alterix";
    getCurrentWindow()
      .setTitle(title)
      .catch(() => {});
  });
  $effect(() => {
    if (app.settings.columns.length) app.loadExtraColumns();
  });

  async function openFolder() {
    const dir = await open({ directory: true, title: "Open a folder" });
    if (typeof dir === "string") await app.openFolder(dir);
  }

  async function addFiles() {
    const picked = await open({ multiple: true, title: "Add files" });
    if (picked) await app.addPaths(picked);
  }

  async function addFolders() {
    const picked = await open({
      directory: true,
      multiple: true,
      title: "Add folders as items",
    });
    if (picked) await app.addPaths(picked);
  }

  async function openPreset() {
    const p = await open({
      title: "Open preset",
      filters: [{ name: "Alterix preset", extensions: [PRESET_EXT] }],
    });
    if (typeof p === "string") await app.openPreset(p);
  }

  async function savePresetAs() {
    const p = await save({
      title: "Save preset",
      defaultPath: presetName ?? `My preset.${PRESET_EXT}`,
      filters: [{ name: "Alterix preset", extensions: [PRESET_EXT] }],
    });
    if (p) await app.savePreset(p);
  }

  async function savePreset() {
    if (app.presetPath) await app.savePreset(app.presetPath);
    else await savePresetAs();
  }

  async function handleLaunch(args: string[]) {
    if (!args.length) return false;
    const preset = args.find((a) => a.toLowerCase().endsWith(`.${PRESET_EXT}`));
    if (preset) await app.openPreset(preset);
    const paths = args.filter((a) => a !== preset);
    if (paths.length === 1) {
      const [e] = (await invoke("stat_paths", { paths })) as {
        isDir: boolean;
      }[];
      if (e?.isDir) {
        await app.openFolder(paths[0]);
        return true;
      }
    }
    if (paths.length) await app.addPaths(paths, true);
    return true;
  }

  const inField = (e: KeyboardEvent) => {
    const t = e.target as HTMLElement;
    return (
      t.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(t.tagName)
    );
  };

  function moveFocus(delta: number, extend: boolean) {
    if (!app.rows.length) return;
    const i = app.rows.findIndex((r) => r.path === app.anchor);
    const n = Math.max(
      0,
      Math.min(app.rows.length - 1, (i < 0 ? -1 : i) + delta),
    );
    const path = app.rows[n].path;
    if (!extend) app.selected.clear();
    app.selected.add(path);
    app.anchor = path;
    list?.scrollToAnchor();
  }

  async function onKey(e: KeyboardEvent) {
    modHeld = e.ctrlKey || e.metaKey;
    if (app.dialog) return;
    const m = isMac() ? e.metaKey : e.ctrlKey;
    const k = e.key.toLowerCase();
    const run = (fn: () => unknown) => {
      e.preventDefault();
      Promise.resolve(fn()).catch((err) => app.setError(err));
    };
    if (m && e.altKey) {
      if (e.key === "ArrowUp") return run(() => app.move("up"));
      if (e.key === "ArrowDown") return run(() => app.move("down"));
      if (e.key === "PageUp") return run(() => app.move("top"));
      if (e.key === "PageDown") return run(() => app.move("bottom"));
      if (k === "s") return run(() => app.swap());
      if (k === "c") return run(() => (app.dialog = "pairs"));
    }
    if (m && !e.altKey) {
      if (k === "o") return run(openFolder);
      if (k === "r") return run(() => app.rename());
      if (k === "z" && !inField(e)) return run(() => app.undo());
      if (k === "j") return run(() => (app.dialog = "jump"));
      if (k === "s") return run(e.shiftKey ? savePresetAs : savePreset);
      if (k === "p") return run(() => app.runPreview());
      if (k === "t")
        return run(() =>
          e.shiftKey ? app.clearCriteria() : app.resetCriteria(),
        );
      if (k === "e") return run(() => app.revertCriteria());
      if (k === "b") return run(() => (app.dialog = "history"));
      if (k === "l")
        return run(
          () => (
            (app.settings.logActivity = !app.settings.logActivity),
            app.saveSettings(),
            app.say(`Rename log ${app.settings.logActivity ? "on" : "off"}.`)
          ),
        );
      if (k === ",") return run(() => (app.dialog = "settings"));
      if (!inField(e)) {
        if (k === "a") return run(() => app.checkAll(true));
        if (k === "d") return run(() => app.checkAll(false));
        if (k === "i") return run(() => app.invertChecks());
        if (k === "v") return run(() => app.pastePaths(e.shiftKey));
        if (k === "c") return run(() => app.copyColumns("all"));
      }
    }
    if (e.key === "F9")
      return run(
        () => (
          (app.settings.wideList = !app.settings.wideList),
          app.saveSettings()
        ),
      );
    if (e.key === "F5") return run(() => app.refresh());
    if (inField(e)) return;
    if (e.altKey && k === "n") return run(() => app.setNamesFromClipboard());
    if (e.altKey && k === "p" && app.anchor)
      return run(
        () => ((app.dialogData = app.anchor), (app.dialog = "properties")),
      );
    if (e.key === "ArrowDown") return run(() => moveFocus(1, e.shiftKey));
    if (e.key === "ArrowUp") return run(() => moveFocus(-1, e.shiftKey));
    if (e.key === " " && app.selected.size) {
      const v = !app.rows.find((r) => r.path === app.anchor)?.checked;
      return run(() =>
        app.rows.forEach((r) => app.selected.has(r.path) && (r.checked = v)),
      );
    }
    if ((e.key === "Delete" || e.key === "Backspace") && app.selected.size)
      return run(() => app.removeSelected());
    const focusedRow = app.rows.find((r) => r.path === app.anchor);
    if (
      (e.key === "F2" || (e.key === "Enter" && !app.settings.enterOpensFile)) &&
      focusedRow
    ) {
      return run(
        () => ((app.dialogData = focusedRow), (app.dialog = "manual")),
      );
    }
    if (e.key === "Enter" && app.settings.enterOpensFile && app.selected.size) {
      return run(() =>
        [...app.selected].slice(0, 9).forEach((p) => openPath(p)),
      );
    }
  }

  onMount(() => {
    const cleanups: (() => void)[] = [];
    (async () => {
      const started = performance.now();
      await app.loadSettings();
      app.appInfo = await invoke("app_info");
      const screens = await availableMonitors().catch(() => []);
      app.log(
        "info",
        "app",
        `UI ready: language ${navigator.language}, ${screens.length} screen(s), scale ${window.devicePixelRatio}, web view ${navigator.userAgent}`,
      );
      app.logNonDefaultSettings();
      if (app.dataStatus && !app.dataStatus.writable) {
        app.error = `Alterix cannot save settings in ${app.dataStatus.path}. Open Settings › Data and integration to choose another folder.`;
      }
      const last = app.settings.lastCriteria as
        Parameters<typeof app.applyPreset>[0] | null;
      if (app.settings.recallLastCriteria && last?.app === "Alterix")
        app.applyPreset(last, null);
      else {
        const def: string | null = await invoke("default_preset");
        if (def) app.applyPresetText(def, null);
      }
      app.history = await invoke("history");
      const launched = await handleLaunch(await invoke("launch_args"));
      if (
        !launched &&
        app.settings.openLastFolder &&
        app.settings.lastFolders[0]
      )
        await app.openFolder(app.settings.lastFolders[0]);
      app.ready = true;
      app.runPreview();
      app.log(
        "info",
        "app",
        `Startup finished in ${Math.round(performance.now() - started)} ms`,
      );
      cleanups.push(
        await listen<string[]>("open-paths", (ev) => handleLaunch(ev.payload)),
      );
      cleanups.push(
        await getCurrentWebview().onDragDropEvent((ev) => {
          if (ev.payload.type === "drop" && ev.payload.paths.length) {
            const preset = ev.payload.paths.find((p) =>
              p.toLowerCase().endsWith(`.${PRESET_EXT}`),
            );
            if (preset) app.openPreset(preset);
            else app.addPaths(ev.payload.paths, modHeld);
          }
        }),
      );
      cleanups.push(
        await getCurrentWindow().onCloseRequested(async () => {
          app.rememberCriteria();
          if (app.settings.autoSavePreset && app.presetPath)
            await app.savePreset(app.presetPath).catch(() => {});
          await invoke("save_settings", {
            settings: $state.snapshot(app.settings),
          }).catch(() => {});
        }),
      );
    })().catch((e) => app.setError(e));
    return () => cleanups.forEach((c) => c());
  });
</script>

<svelte:window
  onkeydown={onKey}
  onkeyup={(e) => (modHeld = e.ctrlKey || e.metaKey)}
  onclick={() => (menu = null)}
/>

<div class="app" class:narrow={app.settings.wideList}>
  <header class="bar">
    <h1>Alterix</h1>
    <span class="vslot" bind:this={slot} aria-hidden="true">
      {app.updateChecking ? "checking…" : version}
    </span>
    <div class="group">
      <button onclick={openFolder} title="Open a folder ({mod}O)">
        Open folder…
      </button>
      <button onclick={addFiles} title="Add files to the list">
        Add files…
      </button>
      <button
        class="icon-btn"
        onclick={() => app.refresh()}
        title="Refresh (F5)"
        aria-label="Refresh"
      >
        <Icon name="refresh" />
      </button>
    </div>
    <button
      class="path"
      title="Go to folder ({mod}J)"
      onclick={() => (app.dialog = "jump")}
    >
      <bdi>{folderLabel}</bdi>
    </button>
    <div class="menuwrap">
      <button
        onclick={(e) => (
          e.stopPropagation(),
          (menu = menu === "list" ? null : "list")
        )}
        aria-expanded={menu === "list"}
      >
        List ▾
      </button>
      {#if menu === "list"}
        <div class="menu" role="menu">
          <button role="menuitem" onclick={addFolders}>
            Add folders as items…
          </button>
          <button role="menuitem" onclick={() => app.pastePaths(false)}>
            Paste paths <kbd>{mod}V</kbd>
          </button>
          <button role="menuitem" onclick={() => app.pastePaths(true)}>
            Replace list with pasted paths <kbd>⇧{mod}V</kbd>
          </button>
          <hr />
          <button role="menuitem" onclick={() => app.checkAll(true)}>
            Include all <kbd>{mod}A</kbd>
          </button>
          <button role="menuitem" onclick={() => app.checkAll(false)}>
            Include none <kbd>{mod}D</kbd>
          </button>
          <button role="menuitem" onclick={() => app.invertChecks()}>
            Invert <kbd>{mod}I</kbd>
          </button>
          <button role="menuitem" onclick={() => app.checkFromClipboard()}>
            Include names on the clipboard
          </button>
          <hr />
          <button role="menuitem" onclick={() => app.randomSort()}>
            Shuffle order
          </button>
          <button role="menuitem" onclick={() => app.showOnlyAffected()}>
            Show only items that change
          </button>
          <button role="menuitem" onclick={() => app.clearUnchecked()}>
            Remove excluded items
          </button>
          <button role="menuitem" onclick={() => app.clearList()}>
            Clear list
          </button>
          <hr />
          <button role="menuitem" onclick={() => (app.dialog = "pairs")}>
            Rename pairs… <kbd>{mod}⌥C</kbd>
          </button>
          <button role="menuitem" onclick={() => app.setNamesFromClipboard()}>
            Set new names from clipboard <kbd>⌥N</kbd>
          </button>
          <button role="menuitem" onclick={() => app.copyColumns("all")}>
            Copy list <kbd>{mod}C</kbd>
          </button>
        </div>
      {/if}
    </div>
    <div class="menuwrap">
      <button
        onclick={(e) => (
          e.stopPropagation(),
          (menu = menu === "presets" ? null : "presets")
        )}
        aria-expanded={menu === "presets"}
        title={app.presetPath ?? "Presets"}
      >
        {presetName ? presetName.replace(/\.alterix$/i, "") : "Presets"} ▾
      </button>
      {#if menu === "presets"}
        <div class="menu right" role="menu">
          <button role="menuitem" onclick={openPreset}>Open preset…</button>
          <button role="menuitem" onclick={savePreset}>
            Save <kbd>{mod}S</kbd>
          </button>
          <button role="menuitem" onclick={savePresetAs}>
            Save as… <kbd>⇧{mod}S</kbd>
          </button>
          {#if app.settings.recentPresets.length}
            <hr />
            {#each app.settings.recentPresets as p (p)}
              <button
                role="menuitem"
                title={p}
                onclick={() => app.openPreset(p)}
              >
                {p
                  .split(/[\\/]/)
                  .pop()
                  ?.replace(/\.alterix$/i, "")}
              </button>
            {/each}
          {/if}
          <hr />
          <button role="menuitem" onclick={() => app.saveAsDefault()}>
            Use current steps as default
          </button>
          <button role="menuitem" onclick={() => app.resetCriteria()}>
            Reset to default <kbd>{mod}T</kbd>
          </button>
          <button
            role="menuitem"
            disabled={!app.lastSaved}
            onclick={() => app.revertCriteria()}
          >
            Revert to saved <kbd>{mod}E</kbd>
          </button>
          <button role="menuitem" onclick={() => app.clearCriteria()}>
            Clear all steps <kbd>⇧{mod}T</kbd>
          </button>
        </div>
      {/if}
    </div>
    <button
      onclick={() => (app.dialog = "history")}
      title="History and undo ({mod}B)"
    >
      History
    </button>
    <button
      class="icon-btn"
      onclick={() => (app.dialog = "settings")}
      title="Settings ({mod},)"
      aria-label="Settings"
    >
      <Icon name="settings" />
    </button>
    {#if !app.settings.autoRefresh}<button onclick={() => app.runPreview()}>
        Preview <kbd>{mod}P</kbd>
      </button>{/if}
    <button
      class="primary rename"
      title="Rename the included items ({mod}R)"
      disabled={app.busy || (!app.counts.changed && !app.checkedRows.length)}
      onclick={() => app.rename()}
    >
      {app.options.copy && app.options.newLocation.trim()
        ? "Copy"
        : "Rename"}{app.counts.changed ? ` ${app.counts.changed}` : ""}
    </button>
  </header>

  <main>
    <Pipeline />
    <section class="files" aria-label="Files">
      {#if app.rows.length === 0}
        <div class="empty">
          <h2>Rename many files at once</h2>
          <p>Open a folder, or drop files and folders here.</p>
          <div class="cta">
            <button class="primary" onclick={openFolder}>Open folder…</button>
            <button onclick={addFiles}>Add files…</button>
          </div>
          <p class="muted small">
            Nothing changes on disk until you press Rename. Every rename can be
            undone.
          </p>
        </div>
      {:else}
        <FileList bind:this={list} />
      {/if}
    </section>
  </main>

  <footer class="bar status">
    {#if app.error}
      <span class="bad" title={app.error}>{app.error}</span>
      <button class="mini" onclick={() => (app.error = null)}>Dismiss</button>
    {:else if app.notice}
      <span>{app.notice}</span>
    {:else}
      <span>{app.checkedRows.length} of {app.rows.length} included</span>
      <span>{app.counts.changed} will change</span>
      {#if app.counts.conflict}<span class="bad">
          {app.counts.conflict} conflict{app.counts.conflict === 1 ? "" : "s"}
        </span>{/if}
      {#if app.counts.invalid}<span class="bad">
          {app.counts.invalid} invalid
        </span>{/if}
      {#if app.counts.skipped}<span class="muted">
          {app.counts.skipped} skipped
        </span>{/if}
      {#if app.options.pairs.length}<button
          class="mini"
          onclick={() => (app.dialog = "pairs")}
        >
          {app.options.pairs.length} pairs
        </button>{/if}
      {#if Object.keys(app.options.overrides).length}<button
          class="mini"
          onclick={() => (app.options.overrides = {})}
        >
          Clear clipboard names
        </button>{/if}
    {/if}
    <div class="spacer"></div>
    {#if app.busy || app.previewing}<span class="muted">Working…</span>{/if}
  </footer>
</div>

{#if version}
  <span id="version-desc" class="sr">Check GitHub for a newer release.</span>
  <button
    class="version"
    class:newer={!!app.newerVersion}
    style:left="{versionPos.left}px"
    style:top="{versionPos.top}px"
    title={versionTip}
    aria-label="Version {version}"
    aria-describedby="version-desc"
    disabled={app.updateChecking}
    onclick={() => app.checkForUpdates()}
  >
    {app.updateChecking ? "checking…" : version}
  </button>
{/if}

<Dialogs />

<style>
  .app {
    position: relative;
    display: grid;
    grid-template-rows: auto minmax(0, 1fr) auto;
    height: 100vh;
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--line);
    min-width: 0;
  }
  header.bar {
    grid-row: 1;
    min-height: 48px;
    background: var(--panel);
  }
  .group {
    display: flex;
    gap: 8px;
    margin-left: 8px;
  }
  .rename {
    margin-left: 8px;
  }
  .status {
    grid-row: 3;
    min-height: 32px;
    padding-block: 4px;
    border-top: 1px solid var(--line);
    border-bottom: 0;
    font-size: 12px;
    color: var(--text-2);
    gap: 16px;
  }
  .status > span {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  h1 {
    margin: 0 4px 0 4px;
    font-size: 15px;
    font-weight: 600;
  }
  .vslot {
    visibility: hidden;
    line-height: 1.4;
    font-size: 11.5px;
    padding: 2px 6px;
    margin-left: -6px;
    margin-right: 4px;
  }
  .version {
    position: absolute;
    transform: translateY(-50%);
    line-height: 1.4;
    z-index: 5;
    margin-left: -6px;
    font-size: 11.5px;
    padding: 2px 6px;
    border: 0;
    border-radius: 5px;
    background: transparent;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }
  .version:hover:not(:disabled) {
    background: var(--hover);
    border-color: transparent;
  }
  .version.newer {
    color: var(--accent-text);
  }
  .version:disabled {
    opacity: 1;
  }
  .path {
    flex: 1;
    margin-left: 8px;
    border: 0;
    background: transparent;
    color: var(--muted);
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    direction: rtl;
    text-align: left;
  }
  .path:hover {
    color: var(--text);
  }
  .spacer {
    flex: 1;
  }
  .icon-btn {
    padding: 4px 8px;
    color: var(--text-2);
  }
  /* Fixed rows, so the status bar always stays at the bottom of the window. Columns: the
     step list, the step editor (hidden with F9) and the file list. */
  main {
    grid-row: 2;
    display: grid;
    grid-template-columns: 200px 320px minmax(0, 1fr);
    min-height: 0;
  }
  .narrow main {
    grid-template-columns: 200px minmax(0, 1fr);
  }
  .files {
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }
  .empty {
    display: grid;
    place-content: center;
    justify-items: center;
    text-align: center;
    height: 100%;
    gap: 6px;
    padding: 24px;
  }
  .empty h2 {
    margin: 0 0 4px;
    font-size: 18px;
    font-weight: 500;
  }
  .empty p {
    margin: 0;
  }
  .cta {
    display: flex;
    gap: 8px;
    margin: 12px 0;
  }
  .menuwrap {
    position: relative;
  }
  .menu {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    z-index: 30;
    min-width: 260px;
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: 8px;
    box-shadow: var(--shadow);
    padding: 4px;
  }
  .menu.right {
    left: auto;
    right: 0;
  }
  .menu button {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    width: 100%;
    border: 0;
    background: transparent;
    text-align: left;
    padding: 4px 8px;
    min-height: 28px;
    align-items: center;
    white-space: nowrap;
  }
  .menu button:hover:not(:disabled) {
    background: var(--hover);
  }
  .menu hr {
    border: 0;
    border-top: 1px solid var(--line);
    margin: 4px 0;
  }
  kbd {
    font: 11px var(--mono);
    color: var(--muted);
  }
</style>
