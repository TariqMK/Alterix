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
    <!-- Cog from Lucide (lucide.dev, ISC licence), drawn on a 24 grid and scaled to 16. -->
    <g transform="scale(0.6667)" stroke-width="2.25">
      <path
        d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"
      />
      <circle cx="12" cy="12" r="3" />
    </g>
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
