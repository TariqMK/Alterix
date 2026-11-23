<!-- The step list (ordered steps, drag to reorder, Filters and After renaming) and the
     editor column beside it, which shows whichever item is selected in the list. -->
<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { RULE_BY_KIND, RULES, type RuleKind, stepSummary } from "./catalog";
  import Field from "./Field.svelte";
  import FiltersBar from "./FiltersBar.svelte";
  import Icon from "./Icon.svelte";
  import { app } from "./state.svelte";
  import StepCard from "./StepCard.svelte";
  import type { StampSet, Tri } from "./types";

  /** What the editor shows: a step (by id), Filters, After renaming or the step catalogue. */
  type Pick = number | "filters" | "after" | "add";
  // What was last chosen. When it is missing (startup, a removed step, a preset opened),
  // the editor shows the first step, or the step catalogue when there are none.
  let chosen = $state<Pick | null>(null);
  const pick = $derived<Pick>(
    chosen !== null &&
      (typeof chosen !== "number" || app.steps.some((s) => s.id === chosen))
      ? chosen
      : (app.steps[0]?.id ?? "add"),
  );
  let dragFrom = $state<number | null>(null);
  let dragOver = $state<number | null>(null);

  const groups = [...new Set(RULES.map((r) => r.group))];
  const editorOpen = $derived(!app.settings.wideList);
  const pickedIndex = $derived(
    typeof pick === "number" ? app.steps.findIndex((s) => s.id === pick) : -1,
  );

  function show(p: Pick) {
    chosen = p;
    if (!editorOpen) {
      app.settings.wideList = false;
      app.saveSettings();
    }
  }

  function add(kind: RuleKind) {
    const s = app.newStep(kind);
    app.steps.push(s);
    chosen = s.id;
  }

  function remove(i: number) {
    const next = app.steps[i + 1] ?? app.steps[i - 1];
    app.steps.splice(i, 1);
    chosen = next?.id ?? null;
  }

  function duplicate(i: number) {
    const copy = app.newStep(app.steps[i].rule.kind);
    copy.rule = JSON.parse(JSON.stringify(app.steps[i].rule));
    app.steps.splice(i + 1, 0, copy);
    chosen = copy.id;
  }

  function drop(to: number) {
    if (dragFrom === null || dragFrom === to) return;
    const [s] = app.steps.splice(dragFrom, 1);
    app.steps.splice(to > dragFrom ? to - 1 : to, 0, s);
    dragFrom = dragOver = null;
  }

  async function pickLocation() {
    const dir = await open({
      directory: true,
      title: "Choose the new location",
    });
    if (typeof dir === "string") app.options.newLocation = dir;
  }

  const afterActive = $derived(
    !!app.options.newLocation.trim() ||
      Object.values(app.options.attributes).some((v) => v !== "keep") ||
      [
        app.options.timestamps.created,
        app.options.timestamps.modified,
        app.options.timestamps.accessed,
      ].some((s) => s.mode !== "keep" || s.delta),
  );
  const afterSummary = $derived(
    app.options.newLocation.trim()
      ? `${app.options.copy ? "Copy" : "Move"} to ${app.options.newLocation.trim().split(/[\\/]/).pop()}`
      : afterActive
        ? "On"
        : "Off",
  );
  const filtersSummary = $derived.by(() => {
    const f = app.filters;
    const mask = f.mask.trim();
    const parts = [mask && mask !== "*" ? mask : ""];
    if (f.subfolders) parts.push("subfolders");
    if (f.hidden) parts.push("hidden");
    if (f.folders && !f.files) parts.push("folders only");
    else if (f.folders) parts.push("+ folders");
    if (f.condition.trim()) parts.push("script");
    return parts.filter(Boolean).join(", ") || "All files";
  });

  const TRI: [Tri, string][] = [
    ["keep", "Keep"],
    ["set", "Set"],
    ["clear", "Clear"],
  ];
  const STAMP_MODES: [string, string][] = [
    ["keep", "Keep"],
    ["set", "Set to"],
    ["current", "Now"],
    ["fromCreated", "Same as created"],
    ["fromModified", "Same as modified"],
    ["fromTaken", "Date taken"],
  ];
  const stamps: ["created" | "modified" | "accessed", string][] = [
    ["created", "Created"],
    ["modified", "Modified"],
    ["accessed", "Accessed"],
  ];
</script>

<nav class="rail" aria-label="Rename steps">
  <div class="rail-head">
    <h2>Steps</h2>
    <button
      class="icon"
      title="{editorOpen ? 'Hide' : 'Show'} the step editor (F9)"
      aria-label="{editorOpen ? 'Hide' : 'Show'} the step editor"
      aria-pressed={editorOpen}
      onclick={() => (
        (app.settings.wideList = !app.settings.wideList),
        app.saveSettings()
      )}
    >
      <Icon name="panel" />
    </button>
  </div>

  <ol class="steps">
    {#each app.steps as step, i (step.id)}
      <li
        class="slot"
        class:over={dragOver === i && dragFrom !== null && dragFrom !== i}
        draggable="true"
        ondragstart={(e) => {
          dragFrom = i;
          e.dataTransfer?.setData("text/plain", String(i));
        }}
        ondragover={(e) => {
          if (dragFrom === null) return;
          e.preventDefault();
          dragOver = i;
        }}
        ondrop={(e) => {
          e.preventDefault();
          drop(i);
        }}
        ondragend={() => (dragFrom = dragOver = null)}
      >
        <button
          class="item"
          class:on={pick === step.id && editorOpen}
          class:off={!step.enabled}
          aria-current={pick === step.id ? "true" : undefined}
          title="{RULE_BY_KIND[step.rule.kind].label}{step.enabled
            ? ''
            : ' (off)'}. Drag to change the order."
          onclick={() => show(step.id)}
        >
          <span class="num">{i + 1}</span>
          <span class="label">{RULE_BY_KIND[step.rule.kind].label}</span>
          <span class="sum">
            {step.enabled ? stepSummary(step.rule) : "Off"}
          </span>
        </button>
      </li>
    {/each}
    {#if dragFrom !== null}
      <li
        class="endzone"
        class:over={dragOver === app.steps.length}
        ondragover={(e) => {
          e.preventDefault();
          dragOver = app.steps.length;
        }}
        ondrop={(e) => {
          e.preventDefault();
          drop(app.steps.length);
        }}
      ></li>
    {/if}
  </ol>
  <button
    class="item add"
    class:on={pick === "add" && editorOpen}
    onclick={() => show("add")}
  >
    <span class="num">+</span><span class="label">Add step</span>
  </button>

  <div class="rail-foot">
    <button
      class="item"
      class:on={pick === "filters" && editorOpen}
      onclick={() => show("filters")}
    >
      <span class="label">Filters</span>
      <span class="sum">{filtersSummary}</span>
    </button>
    <button
      class="item"
      class:on={pick === "after" && editorOpen}
      onclick={() => show("after")}
    >
      <span class="label">After renaming</span>
      <span class="sum">{afterSummary}</span>
    </button>
  </div>
</nav>

{#if editorOpen}
  <section class="editor" aria-label="Step editor">
    {#if pick === "add"}
      <div class="head">
        <h2>Add a step</h2>
      </div>
      <p class="hint">
        Steps run from top to bottom. Each one changes the name the step before
        it made.
      </p>
      <div class="catalogue">
        {#each groups as g (g)}
          <h3>{g}</h3>
          {#each RULES.filter((r) => r.group === g) as r (r.kind)}
            <button class="kind" onclick={() => add(r.kind)}>
              <span class="kind-label">{r.label}</span>
              <span class="kind-sum">{r.summary}</span>
            </button>
          {/each}
        {/each}
      </div>
    {:else if pick === "filters"}
      <div class="head"><h2>Filters</h2></div>
      <FiltersBar />
    {:else if pick === "after"}
      <div class="head"><h2>After renaming</h2></div>
      <div class="after">
        <h3>New location</h3>
        <div class="loc">
          <Field
            bind:value={app.options.newLocation}
            label="Folder (absolute, or relative such as .. or Sorted)"
            recentKey="location"
          />
          <button class="small" onclick={pickLocation}>Choose…</button>
        </div>
        <div class="checks">
          <label class="check">
            <input type="radio" bind:group={app.options.copy} value={true} /> Copy
          </label>
          <label class="check">
            <input type="radio" bind:group={app.options.copy} value={false} /> Move
          </label>
          <label class="check">
            <input type="checkbox" bind:checked={app.options.keepStructure} /> Keep
            subfolders
          </label>
        </div>

        <h3>Attributes</h3>
        <div class="grid2">
          {#each [["readOnly", "Read-only"], ["hidden", "Hidden"], ["system", "System"], ["archive", "Archive"]] as [k, label] (k)}
            <label class="sel">
              <span>{label}</span>
              <select
                bind:value={
                  app.options.attributes[
                    k as "readOnly" | "hidden" | "system" | "archive"
                  ]
                }
              >
                {#each TRI as [v, l] (v)}<option value={v}>{l}</option>{/each}
              </select>
            </label>
          {/each}
        </div>
        <p class="hint">
          System and Archive apply on Windows only. Hidden works on Windows and
          macOS.
        </p>

        <h3>Timestamps</h3>
        {#each stamps as [k, label] (k)}
          {@const s = app.options.timestamps[k] as StampSet}
          <div class="grid2">
            <label class="sel">
              <span>{label}</span>
              <select bind:value={s.mode}>
                {#each STAMP_MODES.filter(([v]) => !(k === "created" && v === "fromCreated")) as [v, l] (v)}<option
                    value={v}
                  >
                    {l}
                  </option>{/each}
              </select>
            </label>
            <label class="sel">
              <span>Shift by</span><input
                bind:value={s.delta}
                placeholder="+1 02:00:00"
              />
            </label>
            {#if s.mode === "set"}
              <label class="sel wide">
                <span>Date and time</span><input
                  bind:value={s.value}
                  placeholder="2024-06-14 09:30:00"
                />
              </label>
            {/if}
          </div>
        {/each}
        <label class="sel half">
          <span>Add per file (seconds)</span>
          <input
            type="number"
            bind:value={app.options.timestamps.incrementSeconds}
          />
        </label>
        <p class="hint">Created cannot be changed on Linux.</p>
      </div>
    {:else if pickedIndex >= 0}
      {#key app.steps[pickedIndex].id}
        <StepCard
          bind:step={app.steps[pickedIndex]}
          index={pickedIndex}
          onremove={() => remove(pickedIndex)}
          onduplicate={() => duplicate(pickedIndex)}
        />
      {/key}
    {/if}
  </section>
{/if}

<style>
  .rail {
    display: flex;
    flex-direction: column;
    min-height: 0;
    min-width: 0;
    padding: 12px 8px;
    gap: 2px;
    background: var(--panel);
    border-right: 1px solid var(--line);
    overflow: auto;
  }
  .rail-head,
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    min-height: 28px;
  }
  .rail-head {
    padding: 0 0 4px 8px;
  }
  h2 {
    margin: 0;
    font-size: 13px;
    font-weight: 500;
  }
  .rail h2 {
    color: var(--text-2);
  }
  h3 {
    margin: 16px 0 8px;
    font-size: 12px;
    font-weight: 500;
    color: var(--text-2);
  }
  h3:first-child {
    margin-top: 0;
  }
  .steps {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .slot {
    border-top: 2px solid transparent;
    margin-top: -2px;
  }
  .slot.over,
  .endzone.over {
    border-top-color: var(--accent);
  }
  .endzone {
    height: 24px;
    border-top: 2px solid transparent;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    height: 32px;
    padding: 0 8px;
    border: 0;
    border-radius: 8px;
    background: transparent;
    text-align: left;
    color: var(--text);
  }
  .item:hover:not(.on) {
    background: var(--hover);
  }
  .item.on {
    background: var(--raised);
    box-shadow: inset 2px 0 0 var(--accent);
  }
  .item.off .label {
    color: var(--muted);
  }
  .slot[draggable="true"] .item {
    cursor: grab;
  }
  .num {
    width: 16px;
    flex: none;
    text-align: right;
    font: 12px var(--mono);
    color: var(--muted);
  }
  .label {
    flex: none;
  }
  .sum {
    flex: 1;
    min-width: 0;
    text-align: right;
    color: var(--muted);
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .add {
    color: var(--text-2);
  }
  .add .num {
    font-family: inherit;
    font-size: 14px;
  }
  .rail-foot {
    margin-top: auto;
    padding-top: 8px;
    border-top: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .editor {
    min-height: 0;
    min-width: 0;
    overflow: auto;
    padding: 12px 16px 24px;
    background: var(--panel);
    border-right: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .hint {
    margin: 0;
    font-size: 12px;
    color: var(--muted);
  }
  .catalogue {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .catalogue h3 {
    margin: 12px 0 4px 8px;
  }
  .catalogue h3:first-child {
    margin-top: 0;
  }
  .kind {
    display: grid;
    gap: 2px;
    width: 100%;
    padding: 8px;
    border: 0;
    border-radius: 8px;
    background: transparent;
    text-align: left;
    white-space: normal;
  }
  .kind:hover,
  .kind:focus-visible {
    background: var(--hover);
  }
  .kind-sum {
    font-size: 12px;
    color: var(--muted);
  }
  .after {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .after h3 {
    margin: 8px 0 0;
  }
  .loc {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 8px;
    align-items: end;
  }
  .checks {
    display: flex;
    gap: 16px;
    flex-wrap: wrap;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
  }
  .grid2 {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }
  .sel {
    display: grid;
    gap: 4px;
    font-size: 12px;
    color: var(--text-2);
  }
  .wide {
    grid-column: span 2;
  }
  .half {
    max-width: 50%;
  }
</style>
