<!-- One pipeline step, drawn from its catalogue entry: header controls and its fields. -->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import {
    type Field as FieldDef,
    RULE_BY_KIND,
    type Rule,
    scriptPrompts,
    type Step,
  } from "./catalog";
  import Field from "./Field.svelte";
  import Icon from "./Icon.svelte";
  import { app } from "./state.svelte";
  import type { Pair } from "./types";

  let {
    step = $bindable(),
    index,
    problem = null,
    onremove,
    onduplicate,
  }: {
    step: Step;
    index: number;
    problem?: string | null;
    onremove: () => void;
    onduplicate: () => void;
  } = $props();

  const def = $derived(RULE_BY_KIND[step.rule.kind]);
  const r = $derived(step.rule as Rule);
  const prompts = $derived(
    step.rule.kind === "script"
      ? scriptPrompts(String(step.rule.code ?? ""))
      : [],
  );
  const uid = `s${Math.random().toString(36).slice(2, 8)}`;

  const str = (k: string) => ({
    get: () => String(step.rule[k] ?? ""),
    set: (v: string) => (step.rule[k] = v),
  });

  async function importPairs() {
    const path = await open({
      title: "Import replacements",
      filters: [{ name: "Text", extensions: ["txt", "csv", "tsv"] }],
    });
    if (typeof path !== "string") return;
    const text: string = await invoke("read_text_file", { path });
    const pairs: Pair[] = await invoke("parse_pairs", { text });
    if (pairs.length) step.rule.pairs = pairs;
  }

  const pairs = $derived((step.rule.pairs as Pair[] | undefined) ?? []);
</script>

{#snippet field(f: FieldDef)}
  {#if !("show" in f) || !f.show || f.show(r)}
    {#if f.type === "row"}
      <div class="row">
        {#each f.fields as sub, i (i)}{@render field(sub)}{/each}
      </div>
    {:else if f.type === "note"}
      <p class="note">{f.text}</p>
    {:else if f.type === "text"}
      {@const b = str(f.key)}
      <Field
        bind:value={b.get, b.set}
        label={f.label}
        tags={f.tags}
        mono={f.mono}
        placeholder={f.placeholder}
        help={f.help}
        recentKey="{r.kind}.{f.key}"
      />
    {:else if f.type === "area"}
      {@const b = str(f.key)}
      <Field
        bind:value={b.get, b.set}
        label={f.label}
        tags={f.tags}
        area
        rows={f.rows}
        mono={r.kind === "script"}
        placeholder={f.placeholder}
        help={f.help}
      />
    {:else if f.type === "number"}
      <label class="numf" title={f.help}>
        <span>{f.label}</span>
        <input
          type="number"
          min={f.min}
          max={f.max}
          step={f.step ?? 1}
          bind:value={
            () => Number(step.rule[f.key] ?? 0),
            (v) => (step.rule[f.key] = Number.isFinite(v) ? v : 0)
          }
        />
      </label>
    {:else if f.type === "check"}
      <label class="check" title={f.help}>
        <input
          type="checkbox"
          bind:checked={
            () => Boolean(step.rule[f.key]), (v) => (step.rule[f.key] = v)
          }
        />
        {f.label}
      </label>
    {:else if f.type === "select"}
      <label class="sel" title={f.help}>
        <span>{f.label}</span>
        <select
          bind:value={
            () => String(step.rule[f.key]), (v) => (step.rule[f.key] = v)
          }
        >
          {#each f.options as [v, l] (v)}<option value={v}>{l}</option>{/each}
        </select>
      </label>
    {:else if f.type === "pairs"}
      <div class="pairs">
        <div class="pairs-head">
          <span title={f.help}>
            {f.label}<span class="hint" aria-hidden="true">?</span>
          </span>
          <button
            class="mini"
            onclick={importPairs}
            title="Load pairs from a text file (comma, tab or | separated)"
          >
            Import…
          </button>
        </div>
        {#each pairs as p, i (i)}
          <div class="pair">
            <Field bind:value={p.find} label="Find" recentKey="replace.find" />
            <Field
              bind:value={p.with}
              label="Replace with"
              tags
              recentKey="replace.with"
            />
            <button
              class="icon rm"
              aria-label="Remove this pair"
              title="Remove this pair"
              disabled={pairs.length === 1}
              onclick={() =>
                (step.rule.pairs = pairs.filter((_, j) => j !== i))}
            >
              −
            </button>
          </div>
        {/each}
        <button
          class="quiet small"
          onclick={() => (step.rule.pairs = [...pairs, { find: "", with: "" }])}
        >
          + Another replacement
        </button>
      </div>
    {/if}
  {/if}
{/snippet}

<article
  class="card"
  class:off={!step.enabled}
  class:bad={!!problem}
  aria-labelledby={uid}
>
  <header>
    <span class="grip" title="Drag to reorder" aria-hidden="true">
      <Icon name="grip" size={12} />
    </span>
    <span class="num">{index + 1}</span>
    <button
      class="title"
      id={uid}
      title={def.summary}
      onclick={() => (step.collapsed = !step.collapsed)}
      aria-expanded={!step.collapsed}
    >
      {def.label}
    </button>
    <label
      class="switch"
      title={step.enabled ? "Turn this step off" : "Turn this step on"}
    >
      <input type="checkbox" bind:checked={step.enabled} />
      <span class="sr">Enabled</span>
    </label>
    <button
      class="icon"
      title="Duplicate"
      aria-label="Duplicate step"
      onclick={onduplicate}
    >
      <Icon name="duplicate" size={13} />
    </button>
    <button
      class="icon"
      title="Remove"
      aria-label="Remove step"
      onclick={onremove}
    >
      ✕
    </button>
  </header>
  {#if problem}<p class="problem">{problem}</p>{/if}
  {#if !step.collapsed}
    <div class="fields">
      {#each def.fields as f, i (i)}{@render field(f)}{/each}
      {#if prompts.length}
        <div class="prompts">
          <p class="note">This script asks for:</p>
          {#each prompts as p (p)}
            <label class="sel">
              <span>{p}</span>
              <input
                bind:value={
                  () => app.options.scriptAnswers[p] ?? "",
                  (v) => (app.options.scriptAnswers[p] = v)
                }
                placeholder="Default value"
              />
            </label>
          {/each}
        </div>
      {/if}
    </div>
  {/if}
</article>

<style>
  .card {
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: 10px;
    padding: 8px 10px 10px;
  }
  .card.off .fields,
  .card.off .title {
    opacity: 0.5;
  }
  .card.bad {
    border-color: var(--bad);
  }
  header {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .grip {
    cursor: grab;
    color: var(--muted);
    padding: 0 2px;
  }
  .num {
    font: 600 11px/18px var(--mono);
    color: var(--muted);
    min-width: 16px;
    text-align: center;
  }
  .title {
    flex: 1;
    border: 0;
    background: transparent;
    padding: 2px 0;
    text-align: left;
    font-weight: 600;
  }
  .fields {
    display: grid;
    gap: 8px;
    margin-top: 8px;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: flex-start;
  }
  .row > .check {
    align-self: flex-end;
    padding-bottom: 6px;
  }
  .numf > span,
  .sel > span {
    min-height: 18px;
    line-height: 18px;
  }
  .row > :global(*) {
    flex: 1 1 90px;
  }
  .row > .check {
    flex: 0 0 auto;
  }
  .numf,
  .sel {
    display: grid;
    gap: 4px;
    font-size: 12px;
    color: var(--muted);
  }
  .check {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    white-space: nowrap;
  }
  .note {
    margin: 0;
    font-size: 11.5px;
    color: var(--muted);
  }
  .problem {
    margin: 6px 0 0;
    font-size: 12px;
    color: var(--bad);
  }
  .pairs {
    display: grid;
    gap: 6px;
  }
  .pairs-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: 12px;
    color: var(--muted);
  }
  .pair {
    display: grid;
    grid-template-columns: 1fr 1fr auto;
    gap: 6px;
    align-items: end;
  }
  .rm {
    margin-bottom: 2px;
  }
  .hint {
    display: inline-grid;
    place-items: center;
    width: 13px;
    height: 13px;
    margin-left: 4px;
    border-radius: 50%;
    border: 1px solid var(--line);
    font-size: 9px;
  }
  .prompts {
    display: grid;
    gap: 6px;
  }
</style>
