<!-- Small inline SVG icons, bundled so they look the same on every system and work offline. -->
<script lang="ts">
  type Name =
    | "refresh"
    | "settings"
    | "expand"
    | "duplicate"
    | "grip"
    | "panel"
    | "close";
  let { name, size = 14 }: { name: Name; size?: number } = $props();
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 16 16"
  fill="none"
  stroke="currentColor"
  stroke-width="1.5"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
>
  {#if name === "refresh"}
    <path d="M13.5 8a5.5 5.5 0 1 1-1.6-3.9" /><path d="M13.5 2.5v3h-3" />
  {:else if name === "settings"}
    <circle cx="8" cy="8" r="2.2" />
    <path
      d="M8 1.5v1.8M8 12.7v1.8M1.5 8h1.8M12.7 8h1.8M3.4 3.4l1.3 1.3M11.3 11.3l1.3 1.3M3.4 12.6l1.3-1.3M11.3 4.7l1.3-1.3"
    />
  {:else if name === "expand"}
    <path d="M9.5 2.5h4v4M6.5 13.5h-4v-4M13.5 2.5 9 7M2.5 13.5 7 9" />
  {:else if name === "duplicate"}
    <rect x="5.5" y="5.5" width="8" height="8" rx="1.5" /><path
      d="M10.5 3.5v-.5a1 1 0 0 0-1-1h-6a1 1 0 0 0-1 1v6a1 1 0 0 0 1 1h.5"
    />
  {:else if name === "grip"}
    <g fill="currentColor" stroke="none">
      <circle cx="6" cy="4" r="1" /><circle cx="10" cy="4" r="1" /><circle
        cx="6"
        cy="8"
        r="1"
      /><circle cx="10" cy="8" r="1" /><circle cx="6" cy="12" r="1" /><circle
        cx="10"
        cy="12"
        r="1"
      />
    </g>
  {:else if name === "panel"}
    <rect x="2" y="2.5" width="12" height="11" rx="1.5" /><path
      d="M10 2.5v11"
    />
  {:else if name === "close"}
    <path d="M4 4l8 8M12 4l-8 8" />
  {/if}
</svg>

<style>
  svg {
    display: block;
  }
</style>
