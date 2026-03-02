<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import { startExtraction, type ExtractionProgress } from "../api/tauri";
  import { friendlyError } from "$lib/utils/errors";
  import LoadingSpinner from "./LoadingSpinner.svelte";

  let samples = $state("");
  let format = $state("twitter");
  let isExtracting = $state(false);
  let error = $state("");
  let progress = $state<ExtractionProgress | null>(null);
  let isDone = $state(false);

  const formats = ["twitter", "email", "longform", "slack", "linkedin"];

  const statusLabels: Record<string, string> = {
    pending: "Starting extraction...",
    preprocessing: "Preprocessing samples...",
    pass_1_core_identity: "Analyzing core identity...",
    pass_2_surface_patterns: "Extracting surface patterns...",
    pass_3_structural_patterns: "Mapping structural patterns...",
    pass_4_rhetorical_patterns: "Identifying rhetorical moves...",
    assembling: "Assembling voice profile...",
    quality_check: "Running quality check...",
    completed: "Extraction complete",
    saved: "Profile saved",
    stored_server: "Profile created on Noren servers",
    failed: "Extraction failed",
  };

  onMount(() => {
    const cleanups: (() => void)[] = [];

    listen<ExtractionProgress>("extraction-progress", (event) => {
      progress = event.payload;

      if (progress.status === "saved" || progress.status === "stored_server") {
        isExtracting = false;
        isDone = true;
      } else if (progress.status === "failed") {
        isExtracting = false;
        error = progress.error || "Unknown error";
      }
    }).then((fn) => cleanups.push(fn));

    return () => cleanups.forEach((fn) => fn());
  });

  async function handleExtract() {
    if (!samples.trim()) return;
    isExtracting = true;
    error = "";
    isDone = false;
    progress = null;

    try {
      await startExtraction({ samples: samples.trim(), format });
    } catch (e) {
      error = friendlyError(e);
      isExtracting = false;
    }
  }

  function sampleCount(): number {
    const text = samples.trim();
    if (!text) return 0;
    return text.split(/\n\s*\n/).filter((s) => s.trim()).length;
  }
</script>

<div class="flex flex-col gap-3 h-full p-4 overflow-y-auto animate-fade-in-up">
  {#if isDone}
    <!-- Success state -->
    <div class="flex-1 flex flex-col items-center justify-center gap-4">
      <div class="w-12 h-12 rounded-full bg-signal/10 flex items-center justify-center">
        <svg class="w-6 h-6 text-signal" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
          <path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7" />
        </svg>
      </div>
      <div class="text-center">
        <p class="text-sm font-semibold text-foreground">Voice profile created</p>
        <p class="text-xs text-muted mt-1">
          {progress?.status === "stored_server"
            ? "Your profile is stored on Noren servers and ready to use."
            : "Your profile has been saved and is ready to use."}
        </p>
      </div>
      <button
        onclick={() => { isDone = false; samples = ""; progress = null; }}
        class="px-4 py-2 text-xs font-medium bg-surface border border-border text-foreground rounded-md hover:border-secondary transition-colors cursor-pointer"
      >
        Extract another
      </button>
    </div>

  {:else if isExtracting && progress}
    <!-- Progress state -->
    <div class="flex-1 flex flex-col items-center justify-center gap-4">
      <LoadingSpinner />
      <div class="text-center">
        <p class="text-sm font-medium text-foreground">
          {statusLabels[progress.status] || progress.status}
        </p>
        <p class="text-xs text-muted mt-1">
          {progress.progress}% complete
        </p>
      </div>
      <!-- Progress bar -->
      <div class="w-48 h-1.5 bg-tint rounded-full overflow-hidden">
        <div
          class="h-full bg-primary rounded-full transition-all duration-500 ease-out"
          style="width: {progress.progress}%"
        ></div>
      </div>
    </div>

  {:else}
    <!-- Input state -->

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
      <div class="flex items-center justify-between mb-1.5">
        <span class="text-xs font-medium text-muted uppercase tracking-wide">
          Writing samples
        </span>
        {#if samples.trim()}
          <span class="text-[10px] text-muted">
            ~{sampleCount()} samples
          </span>
        {/if}
      </div>
      <textarea
        bind:value={samples}
        class="flex-1 p-3 text-xs leading-relaxed border border-border bg-surface text-foreground resize-none placeholder-muted rounded-md focus:outline-none focus:border-secondary"
        placeholder="Paste 10+ writing samples here, separated by blank lines.

Example formats: tweets, emails, blog posts, Slack messages, LinkedIn posts..."
      ></textarea>
    </div>

    <!-- Extract button -->
    <button
      onclick={handleExtract}
      disabled={!samples.trim() || isExtracting || sampleCount() < 5}
      class="w-full py-2.5 px-4 text-sm font-semibold tracking-wide transition-colors cursor-pointer rounded-md
        {!samples.trim() || isExtracting || sampleCount() < 5
          ? 'bg-surface text-muted border border-border cursor-not-allowed opacity-50'
          : 'bg-primary text-white hover:bg-primary-hover'}"
    >
      {#if isExtracting}
        <span class="inline-flex items-center gap-2 animate-breathe">
          <LoadingSpinner /> Starting...
        </span>
      {:else}
        Extract Voice Profile
      {/if}
    </button>

    {#if samples.trim() && sampleCount() < 5}
      <p class="text-[10px] text-muted text-center">
        Need at least 5 samples (separated by blank lines). Currently: ~{sampleCount()}
      </p>
    {/if}

    <!-- Error -->
    {#if error}
      <div class="p-2 bg-tint border border-border rounded-md text-xs text-muted leading-relaxed">
        {error}
      </div>
    {/if}
  {/if}
</div>
