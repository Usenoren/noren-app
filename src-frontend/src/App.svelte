<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { ensureAnalyticsBootstrap } from "./lib/api/analytics";
  import Shell from "./lib/components/Shell.svelte";
  import MainShell from "./lib/components/MainShell.svelte";

  const label = getCurrentWindow().label;

  // Disable browser context menu globally
  document.addEventListener("contextmenu", (e) => e.preventDefault());

  ensureAnalyticsBootstrap().catch(() => {});
</script>

{#if label === "main-app"}
  <MainShell />
{:else}
  <Shell />
{/if}
