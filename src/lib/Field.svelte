<!-- A labelled text field with recent entries, an insert-tag menu and a larger editor. -->
<script lang="ts">
  import { TAG_GROUPS } from "./catalog";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";
  import { app } from "./state.svelte";

  let {
    value = $bindable(),
    label,
    recentKey = "",
    tags = false,
    mono = false,
    area = false,
    rows = 3,
    placeholder = "",
    help = "",
  }: {
    value: string;
    label: string;
    recentKey?: string;
    tags?: boolean;
    mono?: boolean;
    area?: boolean;
    rows?: number;
    placeholder?: string;
    help?: string;
  } = $props();

  const uid = `f${Math.random().toString(36).slice(2, 9)}`;
  let el: HTMLInputElement | HTMLTextAreaElement | undefined = $state();
  let menu = $state(false);
  let zoom = $state(false);
  let zoomText = $state("");
  const recent = $derived(
    recentKey ? (app.settings.recent[recentKey] ?? []) : [],
  );

  function insert(tag: string) {
    menu = false;
    const v = value ?? "";
    const a = el?.selectionStart ?? v.length;
    const b = el?.selectionEnd ?? v.length;
    value = v.slice(0, a) + tag + v.slice(b);
    queueMicrotask(() => {
      el?.focus();
      el?.setSelectionRange(a + tag.length, a + tag.length);
    });
  }
</script>

<div class="field">
  <div class="top">
    <label for={uid} title={help || undefined}>
      {label}{#if help}<span class="hint" aria-hidden="true">?</span>{/if}
    </label>
    <span class="tools">
      {#if tags}
        <button
          class="mini"
          title="Insert a tag"
          aria-label="Insert a tag"
          onclick={() => (menu = !menu)}
        >
          &lt;&gt;
        </button>
      {/if}
      <button
        class="mini"
        title="Open a larger editor"
        aria-label="Open a larger editor"
        onclick={() => {
          zoomText = value ?? "";
          zoom = true;
        }}
      >
        <Icon name="expand" size={11} />
      </button>
    </span>
  </div>
  {#if area}
    <textarea
      id={uid}
      bind:this={el}
      bind:value
      {rows}
      {placeholder}
      class:mono
      spellcheck="false"></textarea>
  {:else}
    <input
      id={uid}
      bind:this={el}
      bind:value
      {placeholder}
      class:mono
      spellcheck="false"
      autocomplete="off"
      list={recent.length ? `${uid}-r` : undefined}
    />
    {#if recent.length}
      <datalist id="{uid}-r">
        {#each recent as r (r)}<option value={r}></option>{/each}
      </datalist>
    {/if}
  {/if}
  {#if menu}
    <div class="menu" role="menu">
      {#each TAG_GROUPS as g (g.group)}
        <div class="group">{g.group}</div>
        {#each g.tags as [tag, name] (tag)}
          <button role="menuitem" class="item" onclick={() => insert(tag)}>
            <span>{name}</span><code>{tag}</code>
          </button>
        {/each}
      {/each}
    </div>
  {/if}
</div>

{#if zoom}
  <Modal title={label} onclose={() => (zoom = false)} wide>
    <textarea
      class="zoom"
      class:mono
      bind:value={zoomText}
      rows="12"
      spellcheck="false"></textarea>
    {#if recent.length}
      <p class="muted small">Recent entries</p>
      <div class="recent">
        {#each recent as r (r)}<button
            class="chip"
            onclick={() => (zoomText = r)}
          >
            {r}
          </button>{/each}
      </div>
    {/if}
    {#snippet footer()}
      <button class="quiet" onclick={() => (zoom = false)}>Cancel</button>
      <button
        class="primary"
        onclick={() => {
          value = zoomText;
          zoom = false;
        }}
      >
        Apply
      </button>
    {/snippet}
  </Modal>
{/if}

<style>
  .field {
    position: relative;
    display: grid;
    gap: 4px;
    min-width: 0;
  }
  .top {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  label {
    flex: 1;
    font-size: 12px;
    color: var(--muted);
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
    color: var(--muted);
  }
  .tools {
    display: flex;
    gap: 2px;
  }
  .tools :global(button.mini) {
    height: 18px;
    display: grid;
    place-items: center;
    line-height: 1;
  }
  .top {
    min-height: 18px;
  }
  .mono {
    font-family: var(--mono);
  }
  textarea {
    resize: vertical;
  }
  .menu {
    position: absolute;
    top: 100%;
    right: 0;
    z-index: 30;
    width: 300px;
    max-height: 320px;
    overflow: auto;
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: 8px;
    box-shadow: var(--shadow);
    padding: 4px;
  }
  .group {
    font-size: 11px;
    color: var(--muted);
    padding: 6px 8px 2px;
  }
  .item {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    width: 100%;
    border: 0;
    background: transparent;
    text-align: left;
    padding: 4px 8px;
  }
  .item:hover {
    background: var(--hover);
  }
  .item code {
    color: var(--muted);
    font-size: 11px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .zoom {
    width: 100%;
    font-size: 14px;
  }
  .recent {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .chip {
    font-size: 12px;
    padding: 2px 8px;
  }
</style>
