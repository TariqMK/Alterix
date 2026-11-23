<!-- Which items a folder listing shows: masks, kinds, subfolders, lengths and a script
     condition. Shown in the step editor. Changes apply when "Apply" is pressed or Enter
     is typed. -->
<script lang="ts">
  import { app } from "./state.svelte";
  import { defaultFilters } from "./types";

  const apply = () => app.rescan();
  const key = (e: KeyboardEvent) => e.key === "Enter" && apply();
</script>

<section class="filters" aria-label="Filters">
  <div class="row">
    <label class="sel grow">
      <span>Show names matching</span>
      <input
        class="mono"
        bind:value={app.filters.mask}
        onkeydown={key}
        placeholder="* or *.jpg *.png !*_old.*"
      />
    </label>
    <label class="check">
      <input type="checkbox" bind:checked={app.filters.regex} /> Regular expression
    </label>
    <label class="check">
      <input type="checkbox" bind:checked={app.filters.matchCase} /> Match case
    </label>
  </div>
  <div class="row">
    <label class="check">
      <input type="checkbox" bind:checked={app.filters.files} /> Files
    </label>
    <label class="check">
      <input type="checkbox" bind:checked={app.filters.folders} /> Folders
    </label>
    <label class="check">
      <input type="checkbox" bind:checked={app.filters.hidden} /> Hidden items
    </label>
    <label class="check">
      <input type="checkbox" bind:checked={app.filters.subfolders} /> Subfolders
    </label>
    {#if app.filters.subfolders}
      <label class="sel small-in">
        <span>Levels</span><input
          bind:value={app.filters.level}
          onkeydown={key}
          placeholder="all, 2, >0, =1"
        />
      </label>
    {/if}
    <label class="sel small-in">
      <span>Name length</span>
      <span class="pair">
        <input
          type="number"
          min="0"
          bind:value={app.filters.nameMin}
          title="Minimum (0 = any)"
        />–<input
          type="number"
          min="0"
          bind:value={app.filters.nameMax}
          title="Maximum (0 = any)"
        />
      </span>
    </label>
    <label class="sel small-in">
      <span>Path length</span>
      <span class="pair">
        <input
          type="number"
          min="0"
          bind:value={app.filters.pathMin}
          title="Minimum (0 = any)"
        />–<input
          type="number"
          min="0"
          bind:value={app.filters.pathMax}
          title="Maximum (0 = any)"
        />
      </span>
    </label>
  </div>
  <div class="row">
    <label class="sel grow">
      <span>Script condition (JavaScript, true keeps the item)</span>
      <input
        class="mono"
        bind:value={app.filters.condition}
        onkeydown={key}
        placeholder="object('size') > 1000000 && ext == 'jpg'"
      />
    </label>
    <div class="buttons">
      <button class="quiet" onclick={() => (app.filters = defaultFilters())}>
        Reset
      </button>
      <button onclick={apply} disabled={!app.folders.length}> Apply </button>
    </div>
  </div>
</section>

<style>
  .filters {
    display: grid;
    gap: 12px;
  }
  .row {
    display: flex;
    align-items: end;
    gap: 8px 16px;
    flex-wrap: wrap;
  }
  .grow {
    flex: 1 1 100%;
  }
  .sel {
    display: grid;
    gap: 4px;
    font-size: 12px;
    color: var(--text-2);
  }
  .small-in input {
    width: 72px;
  }
  .pair {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: 0 0 calc(50% - 8px);
  }
  .buttons {
    display: flex;
    gap: 8px;
    margin-left: auto;
  }
  .mono {
    font-family: var(--mono);
  }
</style>
