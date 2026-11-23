<!-- A modal dialog: title, scrolling body and a footer for buttons. Escape closes it. -->
<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    title,
    onclose,
    wide = false,
    children,
    footer,
  }: {
    title: string;
    onclose: () => void;
    wide?: boolean;
    children: Snippet;
    footer?: Snippet;
  } = $props();

  let box: HTMLDivElement | undefined = $state();
  $effect(() => {
    box?.querySelector<HTMLElement>("input, textarea, select, button")?.focus();
  });
</script>

<div
  class="backdrop"
  role="presentation"
  onmousedown={(e) => e.target === e.currentTarget && onclose()}
>
  <div
    class="modal"
    class:wide
    role="dialog"
    aria-modal="true"
    aria-label={title}
    tabindex="-1"
    bind:this={box}
    onkeydown={(e) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        onclose();
      }
    }}
  >
    <header>
      <h2>{title}</h2>
      <button class="icon" aria-label="Close" onclick={onclose}>✕</button>
    </header>
    <div class="body">{@render children()}</div>
    {#if footer}<footer>{@render footer()}</footer>{/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: var(--scrim);
    display: grid;
    place-items: center;
    z-index: 50;
  }
  .modal {
    width: min(560px, calc(100vw - 32px));
    max-height: calc(100vh - 64px);
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: 12px;
    box-shadow: var(--shadow);
  }
  .modal.wide {
    width: min(860px, calc(100vw - 32px));
  }
  header {
    display: flex;
    align-items: center;
    padding: 12px 14px 8px 18px;
  }
  h2 {
    flex: 1;
    margin: 0;
    font-size: 14px;
    font-weight: 500;
  }
  .body {
    padding: 4px 18px 16px;
    overflow: auto;
  }
  footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 12px 18px;
    border-top: 1px solid var(--line);
  }
</style>
