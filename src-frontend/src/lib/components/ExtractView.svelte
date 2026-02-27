<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import LoadingSpinner from "./LoadingSpinner.svelte";

  let samples = $state("");
  let format = $state("twitter");
  let isExtracting = $state(false);
  let error = $state("");

  const formats = ["twitter", "email", "longform", "slack", "linkedin"];

  async function handleExtract() {
    if (!samples.trim()) return;
    isExtracting = true;
    error = "";
    try {
      await invoke("run_extraction", { samples: samples.trim(), format });
    } catch (e) {
      error = String(e);
    } finally {
      isExtracting = false;
    }
  }
</script>

<div class="flex flex-col gap-3 h-full p-4 overflow-y-auto animate-fade-in-up">
  <!-- Coming soon banner -->
  <div class="p-3 bg-tint border border-warning/30 rounded-md">
    <p class="text-xs font-medium text-warning">
      In-app extraction coming soon
    </p>
    <p class="text-xs text-muted mt-1 leading-relaxed">
      Voice extraction runs server-side to protect our analysis engine.
      For now, use the CLI to create profiles:
    </p>
    <code class="block mt-2 px-2 py-1 bg-background border border-border rounded text-[11px] text-secondary font-mono">
      noren extract --samples your-writing.txt
    </code>
  </div>

  <!-- Format selector -->
  <div>
    <span class="block text-xs font-medium text-muted mb-1.5 uppercase tracking-wide">Format</span>
    <div class="flex flex-wrap gap-1">
      {#each formats as fmt}
        <button
          onclick={() => { format = fmt; }}
          class="px-2.5 py-1 text-xs transition-colors cursor-pointer uppercase tracking-wide rounded-md
            {format === fmt
              ? 'bg-primary text-white font-medium'
              : 'bg-surface text-muted border border-border hover:border-secondary hover:text-foreground'}"
        >
          {fmt}
        </button>
      {/each}
    </div>
  </div>

  <!-- Samples input -->
  <div class="flex-1 flex flex-col min-h-0">
    <span class="block text-xs font-medium text-muted mb-1.5 uppercase tracking-wide">
      Writing samples
    </span>
    <textarea
      bind:value={samples}
      class="flex-1 p-3 text-xs leading-relaxed border border-border bg-surface text-foreground resize-none placeholder-muted rounded-md focus:outline-none focus:border-secondary"
      placeholder="Paste 10-20 writing samples here (tweets, emails, posts, etc.)..."
    ></textarea>
  </div>

  <!-- Extract button -->
  <button
    onclick={handleExtract}
    disabled={!samples.trim() || isExtracting}
    class="w-full py-2.5 px-4 text-sm font-semibold tracking-wide transition-colors cursor-pointer rounded-md
      {!samples.trim() || isExtracting
        ? 'bg-surface text-muted border border-border cursor-not-allowed opacity-50'
        : 'bg-primary text-white hover:bg-primary-hover'}"
  >
    {#if isExtracting}
      <span class="inline-flex items-center gap-2 animate-breathe">
        <LoadingSpinner /> Extracting
      </span>
    {:else}
      Extract Voice Profile
    {/if}
  </button>

  <!-- Error -->
  {#if error}
    <div class="p-2 bg-surface border border-error/30 rounded-md text-xs text-error">
      {error}
    </div>
  {/if}
</div>
