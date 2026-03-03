<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import Shell from "./lib/components/Shell.svelte";
  import MainShell from "./lib/components/MainShell.svelte";

  const label = getCurrentWindow().label;

  // Disable browser context menu globally
  document.addEventListener("contextmenu", (e) => e.preventDefault());
</script>

{#if label === "main-app"}
  <MainShell />
{:else}
  <Shell />
{/if}
