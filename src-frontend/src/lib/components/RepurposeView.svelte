<script lang="ts">
  import { repurpose, listFormats, type RepurposeFormatResult } from "$lib/api/tauri";

  // Flip to true when ready to ship
  const FEATURE_ENABLED = false;

  const FORMAT_FAMILIES = [
    { label: "Long-form", formats: ["blog", "article", "newsletter", "essay"] },
    { label: "Social", formats: ["tweet", "twitter", "thread"] },
    { label: "Messaging", formats: ["email", "slack"] },
    { label: "Professional", formats: ["linkedin", "memo"] },
  ];

  const ALL_FORMATS = FORMAT_FAMILIES.flatMap((f) => f.formats);

  // --- State ---
  let sourceFormat = $state("blog");
  let sourceContent = $state("");
  let targetChecked = $state<Record<string, boolean>>({});
  let isProcessing = $state(false);
  let error = $state("");
  let results = $state<RepurposeFormatResult[]>([]);
  let activeTab = $state("");
  let copiedTab = $state("");
  let totalInputTokens = $state(0);
  let totalOutputTokens = $state(0);
  let availableFormats = $state<string[]>([]);

  // Load available formats from profile
  $effect(() => {
    listFormats().then((f) => {
      availableFormats = f;
    });
  });

  function resetTargets() {
    const sourceFamily = FORMAT_FAMILIES.find((f) => f.formats.includes(sourceFormat));
    const exclude = new Set(sourceFamily?.formats || [sourceFormat]);
    const checked: Record<string, boolean> = {};
    for (const fmt of ALL_FORMATS) {
      checked[fmt] = !exclude.has(fmt);
    }
    targetChecked = checked;
  }
  resetTargets();

  $effect(() => {
    sourceFormat;
    resetTargets();
    results = [];
    activeTab = "";
  });

  const selectedTargets = $derived(
    Object.entries(targetChecked)
      .filter(([_, v]) => v)
      .map(([k]) => k)
  );

  const canRepurpose = $derived(
    sourceContent.trim().length > 0 && selectedTargets.length > 0 && !isProcessing
  );

  const activeResult = $derived(results.find((r) => r.format === activeTab));

  // --- Actions ---
  async function handleRepurpose() {
    if (!canRepurpose) return;
    isProcessing = true;
    error = "";
    results = [];
    activeTab = "";

    try {
      const resp = await repurpose({
        sourceContent: sourceContent.trim(),
        sourceFormat,
        targetFormats: selectedTargets,
      });
      results = resp.results;
      totalInputTokens = resp.total_input_tokens;
      totalOutputTokens = resp.total_output_tokens;
      if (results.length > 0) {
        activeTab = results[0].format;
      }
    } catch (e: any) {
      error = e?.message || String(e);
    } finally {
      isProcessing = false;
    }
  }

  async function handleCopy(format: string) {
    const r = results.find((r) => r.format === format);
    if (!r) return;
    try {
      await navigator.clipboard.writeText(r.content);
      copiedTab = format;
      setTimeout(() => { copiedTab = ""; }, 1500);
    } catch {}
  }

  function handleBack() {
    results = [];
    activeTab = "";
  }
</script>

<div class="flex flex-col h-full overflow-hidden px-6 py-5">
  <div class="mb-4 shrink-0">
    <h1 class="font-heading italic text-2xl text-foreground font-normal">Repurpose</h1>
    <p class="text-xs text-muted mt-1">Transform content across formats, in your voice</p>
  </div>

  {#if !FEATURE_ENABLED}
    <!-- Coming soon -->
    <div class="flex-1 flex flex-col items-center justify-center">
      <svg class="w-12 h-12 text-accent/25 mb-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
        <path stroke-linecap="round" stroke-linejoin="round" d="M19.5 12c0-1.232-.046-2.453-.138-3.662a4.006 4.006 0 00-3.7-3.7 48.678 48.678 0 00-7.324 0 4.006 4.006 0 00-3.7 3.7c-.017.22-.032.441-.046.662M19.5 12l3-3m-3 3l-3-3m-12 3c0 1.232.046 2.453.138 3.662a4.006 4.006 0 003.7 3.7 48.656 48.656 0 007.324 0 4.006 4.006 0 003.7-3.7c.017-.22.032-.441.046-.662M4.5 12l3 3m-3-3l-3 3" />
      </svg>
      <p class="text-base text-foreground font-medium mb-1.5">Coming soon</p>
      <p class="text-sm text-muted text-center leading-relaxed max-w-sm">
        Transform a blog post into tweets, emails, and LinkedIn posts. One piece of content, every format, all in your voice.
      </p>
    </div>
  {:else if results.length === 0}
    <!-- Input mode -->
    <div class="flex-1 min-h-0 flex flex-col overflow-y-auto">
      <!-- Source format -->
      <div class="flex items-center gap-3 mb-3 shrink-0">
        <label class="text-xs text-muted font-medium w-12">From</label>
        <select
          bind:value={sourceFormat}
          class="px-3 py-1.5 text-sm border border-border bg-surface text-foreground rounded-lg focus:outline-none focus:border-secondary"
        >
          {#each ALL_FORMATS as fmt}
            <option value={fmt}>{fmt}</option>
          {/each}
        </select>
      </div>

      <!-- Source content -->
      <textarea
        bind:value={sourceContent}
        class="w-full min-h-[160px] flex-1 p-4 text-sm resize-none bg-surface text-foreground placeholder-muted border border-border rounded-xl focus:outline-none focus:border-secondary"
        placeholder="Paste the content you want to repurpose..."
        disabled={isProcessing}
      ></textarea>

      <!-- Target formats -->
      <div class="mt-4 shrink-0">
        <label class="text-xs text-muted font-medium mb-2 block">Target formats</label>
        <div class="flex flex-col gap-2">
          {#each FORMAT_FAMILIES as family}
            {@const visibleFormats = family.formats.filter((f) => f !== sourceFormat)}
            {#if visibleFormats.length > 0}
              <div class="flex items-center gap-1">
                <span class="text-[10px] text-muted w-20 shrink-0">{family.label}</span>
                <div class="flex items-center gap-1.5 flex-wrap">
                  {#each visibleFormats as fmt}
                    <label
                      class="inline-flex items-center gap-1.5 px-2.5 py-1 text-xs rounded-lg cursor-pointer transition-colors
                        {targetChecked[fmt]
                          ? 'bg-accent/10 text-accent border border-accent/20'
                          : 'bg-surface text-muted border border-border hover:border-secondary/30'}"
                    >
                      <input type="checkbox" bind:checked={targetChecked[fmt]} class="hidden" />
                      {fmt}
                    </label>
                  {/each}
                </div>
              </div>
            {/if}
          {/each}
        </div>
      </div>

      <!-- Error -->
      {#if error}
        <div class="mt-3 p-3 bg-warning/5 border border-warning/20 rounded-xl text-xs text-warning">
          {error}
        </div>
      {/if}

      <!-- Action -->
      <div class="mt-4 shrink-0">
        <button
          onclick={handleRepurpose}
          disabled={!canRepurpose}
          class="w-full py-2.5 text-sm font-medium rounded-xl transition-colors cursor-pointer
            {canRepurpose
              ? 'bg-accent text-white hover:bg-accent-hover'
              : 'bg-surface text-muted border border-border cursor-not-allowed opacity-50'}"
          style={canRepurpose ? 'box-shadow: 0 0 12px var(--color-accent-glow)' : ''}
        >
          {#if isProcessing}
            <span class="inline-flex items-center gap-2">
              <span class="w-3.5 h-3.5 border-2 border-white/30 border-t-white rounded-full animate-spin"></span>
              Repurposing {selectedTargets.length} format{selectedTargets.length !== 1 ? 's' : ''}...
            </span>
          {:else}
            Repurpose to {selectedTargets.length} format{selectedTargets.length !== 1 ? 's' : ''}
          {/if}
        </button>
      </div>
    </div>

  {:else}
    <!-- Results mode -->
    <div class="flex-1 min-h-0 flex flex-col overflow-hidden">
      <!-- Tabs -->
      <div class="flex items-center gap-1 pb-3 shrink-0 overflow-x-auto border-b border-border">
        {#each results as r}
          <button
            onclick={() => { activeTab = r.format; }}
            class="relative px-3 py-1.5 text-xs rounded-lg transition-colors cursor-pointer whitespace-nowrap
              {activeTab === r.format
                ? 'text-accent font-medium bg-accent/5 border border-accent/15'
                : 'text-muted hover:text-foreground hover:bg-foreground/[0.04] border border-transparent'}"
          >
            {r.format}
            {#if !r.passed}
              <span class="ml-1 inline-block w-1.5 h-1.5 rounded-full bg-warning"></span>
            {/if}
          </button>
        {/each}

        <button
          onclick={handleBack}
          class="ml-auto px-3 py-1.5 text-xs text-muted hover:text-foreground transition-colors cursor-pointer rounded-lg hover:bg-foreground/[0.04]"
        >
          New
        </button>
      </div>

      <!-- Active result -->
      {#if activeResult}
        <div class="flex-1 min-h-0 overflow-y-auto mt-3">
          <textarea
            value={activeResult.content}
            oninput={(e) => {
              const r = results.find((r) => r.format === activeTab);
              if (r) r.content = (e.target as HTMLTextAreaElement).value;
            }}
            class="w-full h-full p-4 text-sm text-foreground bg-surface border border-border rounded-xl resize-none focus:outline-none focus:border-secondary"
            style="line-height:1.75"
          ></textarea>
        </div>

        <!-- Result footer -->
        <div class="flex items-center py-3 shrink-0">
          <div class="flex items-center gap-3">
            {#if activeResult.passed}
              <span class="inline-flex items-center gap-1 text-xs text-signal">
                <svg class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7"/></svg>
                Voice check passed
              </span>
            {:else}
              <span class="inline-flex items-center gap-1 text-xs text-warning">
                <svg class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M12 9v3.75m9-.75a9 9 0 11-18 0 9 9 0 0118 0zm-9 3.75h.008v.008H12v-.008z"/></svg>
                Voice check flagged
              </span>
            {/if}
            <span class="font-mono text-[10px] text-muted">
              {activeResult.input_tokens + activeResult.output_tokens} tokens
            </span>
          </div>

          <div class="flex items-center gap-2 ml-auto">
            <button
              onclick={() => handleCopy(activeTab)}
              class="inline-flex items-center gap-1.5 px-3 py-1.5 text-xs border border-border hover:border-secondary transition-colors cursor-pointer text-muted hover:text-foreground rounded-lg"
            >
              {#if copiedTab === activeTab}
                <svg class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7"/></svg>
                Copied
              {:else}
                <svg class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5"><path stroke-linecap="round" stroke-linejoin="round" d="M15.666 3.888A2.25 2.25 0 0013.5 2.25h-3c-1.03 0-1.9.693-2.166 1.638m7.332 0c.055.194.084.4.084.612v0a.75.75 0 01-.75.75H9.75a.75.75 0 01-.75-.75v0c0-.212.03-.418.084-.612m7.332 0c.646.049 1.288.11 1.927.184 1.1.128 1.907 1.077 1.907 2.185V19.5a2.25 2.25 0 01-2.25 2.25H6.75A2.25 2.25 0 014.5 19.5V6.257c0-1.108.806-2.057 1.907-2.185a48.208 48.208 0 011.927-.184"/></svg>
                Copy
              {/if}
            </button>
          </div>
        </div>
      {/if}

      <!-- Summary bar -->
      <div class="shrink-0 pt-2 border-t border-border">
        <span class="font-mono text-[10px] text-muted">
          {results.length} format{results.length !== 1 ? 's' : ''} generated &middot;
          {totalInputTokens + totalOutputTokens} total tokens
        </span>
      </div>
    </div>
  {/if}
</div>
