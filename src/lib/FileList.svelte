<!-- The file list: a virtualised table of items with include checkboxes, sortable
     columns, multi-select, change highlighting, a context menu and a picture preview. -->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
  import { app, isMac } from "./state.svelte";
  import { type ColumnId, COLUMNS, type Row } from "./types";

  const ROW = 28;
  let scroller: HTMLDivElement | undefined = $state();
  let scrollTop = $state(0);
  let height = $state(400);
  let menu = $state<{ x: number; y: number } | null>(null);
  let picture = $state<string | null>(null);

  const cols = $derived(
    app.settings.columns.filter((c) => COLUMNS.some((x) => x.id === c)),
  );
  const start = $derived(Math.max(0, Math.floor(scrollTop / ROW) - 10));
  const end = $derived(
    Math.min(app.rows.length, Math.ceil((scrollTop + height) / ROW) + 10),
  );
  const visible = $derived(app.rows.slice(start, end));
  const template = $derived(
    [
      "28px",
      ...cols.map((c) =>
        c === "name" || c === "newName"
          ? "minmax(160px, 2fr)"
          : c === "custom" || c === "subdir"
            ? "minmax(100px, 1fr)"
            : c === "attrs" || c === "ext" || c === "pathLength"
              ? "80px"
              : c === "size"
                ? "90px"
                : "168px",
      ),
    ].join(" "),
  );

  function fmtSize(n: number, isDir: boolean): string {
    if (isDir) return "";
    const unit = app.settings.sizeUnit;
    const f = (v: number, u: string) =>
      `${v >= 100 || Number.isInteger(v) ? v.toFixed(0) : v.toFixed(1)} ${u}`;
    if (unit === "bytes") return `${n.toLocaleString()} B`;
    if (unit === "kb") return f(n / 1024, "KB");
    if (unit === "mb") return f(n / 1048576, "MB");
    if (n < 1024) return `${n} B`;
    if (n < 1048576) return f(n / 1024, "KB");
    if (n < 1073741824) return f(n / 1048576, "MB");
    return f(n / 1073741824, "GB");
  }

  function cell(r: Row, c: ColumnId): string {
    if (c === "size") return fmtSize(r.size, r.isDir);
    return String(app.value(r, c) ?? "");
  }

  /** Splits the new name into unchanged start, changed middle and unchanged end. */
  function diff(a: string, b: string): [string, string, string] {
    let i = 0;
    while (i < a.length && i < b.length && a[i] === b[i]) i++;
    let j = 0;
    while (
      j < a.length - i &&
      j < b.length - i &&
      a[a.length - 1 - j] === b[b.length - 1 - j]
    )
      j++;
    return [b.slice(0, i), b.slice(i, b.length - j), b.slice(b.length - j)];
  }

  function click(e: MouseEvent, r: Row, index: number) {
    const multi = isMac() ? e.metaKey : e.ctrlKey;
    if (e.shiftKey && app.anchor) {
      const a = app.rows.findIndex((x) => x.path === app.anchor);
      const [lo, hi] = a < index ? [a, index] : [index, a];
      if (!multi) app.selected.clear();
      for (let k = lo; k <= hi; k++) app.selected.add(app.rows[k].path);
    } else if (multi) {
      if (app.selected.has(r.path)) app.selected.delete(r.path);
      else app.selected.add(r.path);
      app.anchor = r.path;
    } else {
      app.selected.clear();
      app.selected.add(r.path);
      app.anchor = r.path;
    }
  }

  function context(e: MouseEvent, r: Row) {
    e.preventDefault();
    if (!app.selected.has(r.path)) {
      app.selected.clear();
      app.selected.add(r.path);
      app.anchor = r.path;
    }
    menu = {
      x: Math.min(e.clientX, window.innerWidth - 260),
      y: Math.min(e.clientY, window.innerHeight - 420),
    };
  }

  function act(fn: () => unknown) {
    menu = null;
    Promise.resolve(fn()).catch((e) => app.setError(e));
  }

  const focused = $derived(
    app.anchor ? app.rows.find((r) => r.path === app.anchor) : undefined,
  );

  $effect(() => {
    const p = focused?.path;
    if (!app.settings.showPreviewPane || !p || focused?.isDir) {
      picture = null;
      return;
    }
    let live = true;
    invoke<string | null>("image_preview", { path: p }).then(
      (d) => live && (picture = d),
    );
    return () => (live = false);
  });

  export function scrollToAnchor() {
    const i = app.rows.findIndex((r) => r.path === app.anchor);
    if (i < 0 || !scroller) return;
    const top = i * ROW;
    if (top < scroller.scrollTop) scroller.scrollTop = top;
    else if (top + ROW > scroller.scrollTop + height - ROW)
      scroller.scrollTop = top - height + 2 * ROW;
  }

  const sortMark = (c: ColumnId) =>
    app.sort.col === c ? (app.sort.dir === 1 ? " ▲" : " ▼") : "";
</script>

<svelte:window onclick={() => (menu = null)} />

<div
  class="wrap"
  class:mono={app.settings.monospaceList}
  class:grid={app.settings.gridlines}
>
  <div
    class="table"
    role="grid"
    aria-rowcount={app.rows.length}
    aria-label="Files"
  >
    <div class="thead" role="row" style:grid-template-columns={template}>
      <div class="th check" role="columnheader">
        <input
          type="checkbox"
          aria-label="Include all"
          checked={app.rows.length > 0 && app.rows.every((r) => r.checked)}
          indeterminate={app.rows.some((r) => r.checked) &&
            !app.rows.every((r) => r.checked)}
          onchange={(e) => app.checkAll((e.target as HTMLInputElement).checked)}
        />
      </div>
      {#each cols as c (c)}
        <button
          class="th"
          role="columnheader"
          aria-sort={app.sort.col === c
            ? app.sort.dir === 1
              ? "ascending"
              : "descending"
            : "none"}
          onclick={() => app.sortBy(c)}
        >
          {COLUMNS.find((x) => x.id === c)?.label}{sortMark(c)}
        </button>
      {/each}
    </div>
    <div
      class="scroller"
      bind:this={scroller}
      bind:clientHeight={height}
      onscroll={(e) => (scrollTop = (e.target as HTMLElement).scrollTop)}
    >
      <div class="spacer" style:height="{app.rows.length * ROW}px">
        {#each visible as r, k (r.path)}
          {@const p = app.preview.get(r.path)}
          {@const i = start + k}
          <!-- svelte-ignore a11y_click_events_have_key_events (rows use the window's arrow, Space, F2 and Delete keys) -->
          <div
            class="tr {r.checked ? (p?.status ?? '') : 'off'}"
            class:sel={app.selected.has(r.path)}
            role="row"
            aria-selected={app.selected.has(r.path)}
            tabindex="-1"
            style:top="{i * ROW}px"
            style:grid-template-columns={template}
            title={p?.message ?? p?.alerts?.join("\n") ?? r.path}
            onclick={(e) => click(e, r, i)}
            oncontextmenu={(e) => context(e, r)}
            ondblclick={() => {
              if (app.settings.enterOpensFile) openPath(r.path);
              else {
                app.dialogData = r;
                app.dialog = "manual";
              }
            }}
          >
            <div class="td check" role="gridcell">
              <input
                type="checkbox"
                aria-label="Include {r.name}"
                bind:checked={r.checked}
                onclick={(e) => e.stopPropagation()}
              />
            </div>
            {#each cols as c (c)}
              <div class="td {c}" role="gridcell">
                {#if c === "name"}
                  <span class="icon-kind" aria-hidden="true">
                    {r.isDir ? "▸" : ""}
                  </span>{r.name}
                {:else if c === "newName"}
                  {#if r.checked && p && p.status !== "unchanged"}
                    {#if app.settings.highlightChanges && p.status === "changed"}
                      {@const [a, b, d] = diff(r.name, p.newName)}
                      {a}<mark>{b}</mark>{d}
                    {:else}
                      {p.newName}
                    {/if}
                    {#if p.status === "conflict" || p.status === "invalid" || p.status === "skipped"}
                      <span class="flag">
                        {p.status === "invalid"
                          ? "Invalid"
                          : p.status === "skipped"
                            ? "Skipped"
                            : "Conflict"}
                      </span>
                    {/if}
                  {:else if r.checked && p}
                    <span class="same">{r.name}</span>
                  {/if}
                {:else}
                  {cell(r, c)}
                {/if}
              </div>
            {/each}
          </div>
        {/each}
      </div>
    </div>
  </div>
  {#if app.settings.showPreviewPane}
    <div class="pane" aria-label="Picture preview">
      {#if picture}
        <img src={picture} alt={focused?.name ?? ""} />
      {:else}
        <p class="muted small">
          {focused
            ? "No preview for this item."
            : "Select a picture to preview it."}
        </p>
      {/if}
    </div>
  {/if}
</div>

{#if menu}
  <div
    class="ctx"
    role="menu"
    tabindex="-1"
    style:left="{menu.x}px"
    style:top="{menu.y}px"
    onclick={(e) => e.stopPropagation()}
    onkeydown={(e) => e.key === "Escape" && (menu = null)}
  >
    {#if app.anchor}
      {@const anchor = app.anchor}
      <button role="menuitem" onclick={() => act(() => openPath(anchor))}>
        Open
      </button>
      <button
        role="menuitem"
        onclick={() => act(() => revealItemInDir(anchor))}
      >
        Show in folder
      </button>
      <button
        role="menuitem"
        onclick={() =>
          act(() => {
            app.dialogData = anchor;
            app.dialog = "properties";
          })}
      >
        Properties…
      </button>
      <button role="menuitem" onclick={() => act(() => app.debug(anchor))}>
        Explain new name…
      </button>
      <button
        role="menuitem"
        onclick={() =>
          act(() => {
            app.dialogData = app.rows.find((r) => r.path === anchor);
            app.dialog = "manual";
          })}
      >
        Rename by hand… <kbd>F2</kbd>
      </button>
      <hr />
    {/if}
    <button
      role="menuitem"
      onclick={() =>
        act(() => {
          for (const r of app.rows)
            if (app.selected.has(r.path)) r.checked = true;
        })}
    >
      Include selected
    </button>
    <button
      role="menuitem"
      onclick={() =>
        act(() => {
          for (const r of app.rows)
            if (app.selected.has(r.path)) r.checked = false;
        })}
    >
      Exclude selected
    </button>
    <hr />
    <div class="sub">Copy</div>
    <button role="menuitem" onclick={() => act(() => app.copyColumns("names"))}>
      Names
    </button>
    <button
      role="menuitem"
      onclick={() => act(() => app.copyColumns("newNames"))}
    >
      New names
    </button>
    <button role="menuitem" onclick={() => act(() => app.copyColumns("paths"))}>
      Full paths
    </button>
    <button
      role="menuitem"
      onclick={() => act(() => app.copyColumns("folders"))}
    >
      Folders
    </button>
    <button role="menuitem" onclick={() => act(() => app.copyColumns("all"))}>
      All columns
    </button>
    <hr />
    <button
      role="menuitem"
      onclick={() => act(() => app.setNamesFromClipboard())}
    >
      Set new names from clipboard
    </button>
    <hr />
    <div class="sub">Move in list</div>
    <div class="inline">
      <button onclick={() => act(() => app.move("top"))} title="To top">
        ⤒
      </button>
      <button onclick={() => act(() => app.move("up"))} title="Up">↑</button>
      <button onclick={() => act(() => app.move("down"))} title="Down">
        ↓
      </button>
      <button onclick={() => act(() => app.move("bottom"))} title="To bottom">
        ⤓
      </button>
      <button onclick={() => act(() => app.swap())} title="Swap two">⇅</button>
    </div>
    <button role="menuitem" onclick={() => act(() => app.removeSelected())}>
      Remove from list <kbd>Del</kbd>
    </button>
  </div>
{/if}

<style>
  .wrap {
    display: flex;
    min-height: 0;
    min-width: 0;
    height: 100%;
  }
  .table {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    font-size: var(--list-size);
  }
  .mono .table {
    font-family: var(--mono);
  }
  .thead {
    display: grid;
    border-bottom: 1px solid var(--line);
    background: var(--bg);
  }
  .th {
    text-align: left;
    font-size: 11.5px;
    font-weight: 600;
    color: var(--muted);
    padding: 7px 10px;
    border: 0;
    border-radius: 0;
    background: transparent;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .th:hover {
    color: var(--text);
  }
  .check {
    display: grid;
    place-items: center;
    padding: 0;
  }
  .scroller {
    flex: 1;
    overflow: auto;
    position: relative;
  }
  .spacer {
    position: relative;
  }
  .tr {
    position: absolute;
    left: 0;
    right: 0;
    height: 28px;
    display: grid;
    align-items: center;
    cursor: default;
  }
  .grid .tr {
    border-bottom: 1px solid var(--line-soft);
  }
  .tr:hover {
    background: var(--hover);
  }
  .tr.sel {
    background: var(--select);
  }
  .td {
    padding: 0 10px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .grid .td + .td {
    border-left: 1px solid var(--line-soft);
  }
  .tr.off .td:not(.check) {
    color: var(--muted);
  }
  .newName {
    font-weight: 500;
  }
  .changed .newName {
    color: var(--accent-text);
  }
  mark {
    background: var(--mark);
    color: inherit;
    border-radius: 3px;
    padding: 0 1px;
  }
  .conflict .newName,
  .invalid .newName {
    color: var(--bad);
  }
  .skipped .newName {
    color: var(--muted);
  }
  .same {
    color: var(--muted);
    font-weight: 400;
  }
  .flag {
    font-size: 10.5px;
    font-weight: 600;
    margin-left: 6px;
    padding: 1px 5px;
    border-radius: 4px;
    background: var(--bad-bg);
    color: var(--bad);
  }
  .icon-kind {
    display: inline-block;
    width: 12px;
    color: var(--muted);
  }
  .size,
  .pathLength {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .created,
  .modified,
  .accessed,
  .taken {
    font-variant-numeric: tabular-nums;
    color: var(--muted);
  }
  .pane {
    width: 280px;
    border-left: 1px solid var(--line);
    display: grid;
    place-items: center;
    padding: 12px;
  }
  .pane img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
    border-radius: 6px;
  }
  .ctx {
    position: fixed;
    z-index: 40;
    width: 240px;
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: 8px;
    box-shadow: var(--shadow);
    padding: 4px;
  }
  .ctx button {
    display: flex;
    justify-content: space-between;
    width: 100%;
    border: 0;
    background: transparent;
    text-align: left;
    padding: 5px 8px;
  }
  .ctx button:hover {
    background: var(--hover);
  }
  .ctx hr {
    border: 0;
    border-top: 1px solid var(--line);
    margin: 4px 0;
  }
  .sub {
    font-size: 11px;
    color: var(--muted);
    padding: 4px 8px 2px;
  }
  .inline {
    display: flex;
    gap: 2px;
    padding: 0 4px;
  }
  .inline button {
    justify-content: center;
  }
  kbd {
    font: 11px var(--mono);
    color: var(--muted);
  }
</style>
