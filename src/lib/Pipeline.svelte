<!-- The rename pipeline: ordered steps (drag to reorder), the add-step menu and the
     "after renaming" settings: new location, attributes and timestamps. -->
<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { RULES, type RuleKind } from "./catalog";
  import Field from "./Field.svelte";
  import { app } from "./state.svelte";
  import StepCard from "./StepCard.svelte";
  import type { StampSet, Tri } from "./types";

  let adding = $state(false);
  let dragFrom = $state<number | null>(null);
  let dragOver = $state<number | null>(null);
  let showAfter = $state(false);

  const groups = [...new Set(RULES.map((r) => r.group))];

  let adder: HTMLDivElement | undefined = $state();

  function add(kind: RuleKind) {
    app.steps.push(app.newStep(kind));
    adding = false;
  }

  $effect(() => {
    if (adding)
      queueMicrotask(() => {
        // Scroll only the steps panel, never the window.
        const panel = adder?.closest(".pipeline") as HTMLElement | null;
        if (panel && adder)
          panel.scrollTo({ top: adder.offsetTop - 12, behavior: "smooth" });
      });
  });

  function drop(to: number) {
    if (dragFrom === null || dragFrom === to) return;
    const [s] = app.steps.splice(dragFrom, 1);
    app.steps.splice(to > dragFrom ? to - 1 : to, 0, s);
    dragFrom = dragOver = null;
  }

  function duplicate(i: number) {
    const copy = app.newStep(app.steps[i].rule.kind);
    copy.rule = JSON.parse(JSON.stringify(app.steps[i].rule));
    app.steps.splice(i + 1, 0, copy);
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

<aside class="pipeline" aria-label="Rename steps">
  <div class="head">
    <h2>Steps</h2>
    <span class="muted small">
      {app.steps.length ? "Run top to bottom" : ""}
    </span>
  </div>

  {#if !app.steps.length}
    <div class="empty">
      <p>Add a step to start building new names.</p>
      <p class="muted small">
        Steps run in order. Drag a step to change the order.
      </p>
    </div>
  {/if}

  <div class="list">
    {#each app.steps as step, i (step.id)}
      <div
        class="slot"
        class:over={dragOver === i && dragFrom !== null && dragFrom !== i}
        role="listitem"
        draggable="true"
        ondragstart={(e) => {
          const t = e.target as HTMLElement;
          if (!t.closest(".grip") && !(t instanceof HTMLDivElement)) {
            e.preventDefault();
            return;
          }
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
        <StepCard
          bind:step={app.steps[i]}
          index={i}
          onremove={() => app.steps.splice(i, 1)}
          onduplicate={() => duplicate(i)}
        />
      </div>
    {/each}
    {#if dragFrom !== null}
      <div
        class="endzone"
        class:over={dragOver === app.steps.length}
        role="listitem"
        ondragover={(e) => {
          e.preventDefault();
          dragOver = app.steps.length;
        }}
        ondrop={(e) => {
          e.preventDefault();
          drop(app.steps.length);
        }}
      ></div>
    {/if}
  </div>

  <div class="adder" bind:this={adder}>
    <button
      class="add"
      onclick={() => (adding = !adding)}
      aria-expanded={adding}
    >
      + Add step
    </button>
    {#if adding}
      <div class="menu" role="menu">
        {#each groups as g (g)}
          <div class="group">{g}</div>
          {#each RULES.filter((r) => r.group === g) as r (r.kind)}
            <button class="item" role="menuitem" onclick={() => add(r.kind)}>
              <strong>{r.label}</strong>
              <span>{r.summary}</span>
            </button>
          {/each}
        {/each}
      </div>
    {/if}
  </div>

  <section class="after">
    <button
      class="section"
      onclick={() => (showAfter = !showAfter)}
      aria-expanded={showAfter}
    >
      <span>{showAfter ? "▾" : "▸"} After renaming</span>
      {#if afterActive}<span class="badge">On</span>{/if}
    </button>
    {#if showAfter}
      <div class="after-body">
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
        <div class="grid4">
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
        <p class="muted small">
          System and Archive apply on Windows only. Hidden works on Windows and
          macOS.
        </p>

        <h3>Timestamps</h3>
        {#each stamps as [k, label] (k)}
          {@const s = app.options.timestamps[k] as StampSet}
          <div class="stamp">
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
            {#if s.mode === "set"}
              <label class="sel">
                <span>Date and time</span><input
                  bind:value={s.value}
                  placeholder="2024-06-14 09:30:00"
                />
              </label>
            {/if}
            <label class="sel">
              <span>Shift by</span><input
                bind:value={s.delta}
                placeholder="+1 02:00:00"
              />
            </label>
          </div>
        {/each}
        <label class="sel inc">
          <span>Add per file (seconds)</span>
          <input
            type="number"
            bind:value={app.options.timestamps.incrementSeconds}
          />
        </label>
        <p class="muted small">Created cannot be changed on Linux.</p>
      </div>
    {/if}
  </section>
</aside>

<style>
  .pipeline {
    position: relative;
    border-left: 1px solid var(--line);
    background: var(--panel);
    padding: 12px;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-width: 0;
  }
  .head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
  }
  h2 {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
  }
  h3 {
    margin: 10px 0 6px;
    font-size: 12px;
    font-weight: 600;
  }
  .empty {
    padding: 18px 8px;
    text-align: center;
    border: 1px dashed var(--line);
    border-radius: 10px;
  }
  .empty p {
    margin: 4px 0;
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 8px;
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
  .adder {
    position: relative;
  }
  .add {
    width: 100%;
    padding: 7px;
    border-style: dashed;
    background: transparent;
  }
  .menu {
    margin-top: 6px;
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: 10px;
    box-shadow: var(--shadow);
    padding: 4px;
  }
  .group {
    font-size: 11px;
    color: var(--muted);
    padding: 8px 8px 2px;
  }
  .item {
    display: grid;
    gap: 1px;
    white-space: normal;
    width: 100%;
    text-align: left;
    border: 0;
    background: transparent;
    padding: 6px 8px;
  }
  .item:hover,
  .item:focus-visible {
    background: var(--hover);
  }
  .item span {
    font-size: 11.5px;
    color: var(--muted);
  }
  .section {
    display: flex;
    justify-content: space-between;
    width: 100%;
    border: 0;
    background: transparent;
    padding: 6px 0;
    font-weight: 600;
  }
  .badge {
    font-size: 11px;
    color: var(--accent-text);
  }
  .after-body {
    display: grid;
    gap: 6px;
  }
  .loc {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 6px;
    align-items: end;
  }
  .checks {
    display: flex;
    gap: 14px;
    flex-wrap: wrap;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
  }
  .grid4 {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 6px;
  }
  .sel {
    display: grid;
    gap: 4px;
    font-size: 12px;
    color: var(--muted);
  }
  .stamp {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px;
  }
  .inc {
    max-width: 50%;
  }
</style>
