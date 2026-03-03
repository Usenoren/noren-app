<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import Shell from "./lib/components/Shell.svelte";
  import MainShell from "./lib/components/MainShell.svelte";

  const label = getCurrentWindow().label;
</script>

{#if label === "main-app"}
  <MainShell />
{:else}
  <Shell />
{/if}
