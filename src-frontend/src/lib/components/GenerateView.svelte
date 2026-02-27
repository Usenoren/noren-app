<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { generate, getContextText, listFormats, injectGeneratedText, type GenerateResult } from "$lib/api/tauri";
  import LoadingSpinner from "./LoadingSpinner.svelte";

  // --- State ---
  let prompt = $state("");
  let format = $state("general");
  let level: "strict" | "guided" | "light" = $state("guided");
  let contextText = $state("");
  let detectedApp = $state("");
  let formats = $state<string[]>([]);
  let output = $state<GenerateResult | null>(null);
  let isGenerating = $state(false);
  let error = $state("");

  const levels = ["strict", "guided", "light"] as const;

  // --- Init ---
  $effect(() => {
    listFormats().then((f) => {
      formats = f;
      if (f.length > 0 && !f.includes(format)) {
        format = f[0];
      }
    });

    getContextText().then((text) => {
      if (text) contextText = text;
    });

    const cleanups: (() => void)[] = [];

    listen<string>("context-text", (event) => {
      if (event.payload) contextText = event.payload;
    }).then((fn) => cleanups.push(fn));

    listen<{ name: string; format: string | null }>("detected-app", (event) => {
      const { name, format: detected } = event.payload;
      detectedApp = name;
      if (detected && formats.includes(detected)) {
        format = detected;
      }
    }).then((fn) => cleanups.push(fn));

    return () => cleanups.forEach((fn) => fn());
  });

  // --- Actions ---
  async function handleGenerate() {
    if (!prompt.trim() || isGenerating) return;

    isGenerating = true;
    error = "";
    output = null;

    try {
      output = await generate({
        prompt: prompt.trim(),
        format,
        level,
        context: contextText || undefined,
      });
      if (output) {
        await navigator.clipboard.writeText(output.text);
        copied = true;
      }
    } catch (e) {
      error = String(e);
    } finally {
      isGenerating = false;
    }
  }

  let copied = $state(false);

  async function handleCopy() {
    if (!output) return;
    await navigator.clipboard.writeText(output.text);
    copied = true;
    setTimeout(() => { copied = false; }, 1500);
  }

  async function handleInject() {
    if (!output) return;
    try {
      await injectGeneratedText(output.text);
    } catch (e) {
      error = String(e);
    }
  }

  function clearContext() {
    contextText = "";
  }
</script>

<div class="flex flex-col gap-3 h-full p-4 overflow-y-auto animate-fade-in-up">
  <!-- Context banner -->
  {#if contextText}
    <div class="flex items-start gap-2 p-2 bg-tint border border-border rounded-md text-xs">
      <div class="flex-1 min-w-0">
        <span class="font-medium text-secondary">Selected text:</span>
        <span class="text-muted ml-1">
          {contextText.length > 150 ? contextText.slice(0, 150) + "..." : contextText}
        </span>
      </div>
      <button
        onclick={clearContext}
        class="text-muted hover:text-foreground shrink-0 cursor-pointer"
        aria-label="Clear context"
      >&times;</button>
    </div>
  {/if}

  <!-- Format + Enforcement selectors -->
  <div class="flex items-center gap-3">
    <div class="flex items-center gap-2">
      {#if formats.length > 0}
        <select
          bind:value={format}
          class="px-2 py-1 text-xs border border-border bg-surface text-foreground rounded-md focus:outline-none focus:border-secondary"
        >
          {#each formats as fmt}
            <option value={fmt}>{fmt}</option>
          {/each}
        </select>
      {:else}
        <span class="text-xs text-muted">No formats</span>
      {/if}
      {#if detectedApp}
        <span class="text-[10px] text-secondary">{detectedApp}</span>
      {/if}
    </div>

    <div class="flex gap-1 ml-auto">
      {#each levels as lvl}
        <button
          onclick={() => { level = lvl; }}
          class="px-2.5 py-1 text-xs transition-colors cursor-pointer uppercase tracking-wide rounded-md
            {level === lvl
              ? 'bg-primary text-white font-medium'
              : 'bg-surface text-muted border border-border hover:border-secondary hover:text-foreground'}"
        >
          {lvl}
        </button>
      {/each}
    </div>
  </div>

  <!-- Prompt input -->
  <div class="relative">
    <textarea
      bind:value={prompt}
      onkeydown={(e) => { if (e.key === "Enter" && e.metaKey) handleGenerate(); }}
      class="w-full p-3 text-sm border border-border resize-none bg-surface text-foreground placeholder-muted rounded-md focus:outline-none focus:border-secondary"
      rows={3}
      placeholder="What do you want to write?"
      disabled={isGenerating}
    ></textarea>
    <div class="absolute bottom-2 right-2 text-[10px] text-muted pointer-events-none">
      {#if !isGenerating}Cmd+Enter{/if}
    </div>
  </div>

  <!-- Generate button -->
  <button
    onclick={handleGenerate}
    disabled={!prompt.trim() || isGenerating}
    class="w-full py-2.5 px-4 text-sm font-semibold tracking-wide transition-colors cursor-pointer rounded-md
      {!prompt.trim() || isGenerating
        ? 'bg-surface text-muted border border-border cursor-not-allowed opacity-50'
        : 'bg-primary text-white hover:bg-primary-hover'}"
  >
    {#if isGenerating}
      <span class="inline-flex items-center gap-2 animate-breathe">
        <LoadingSpinner /> Generating
      </span>
    {:else}
      Generate
    {/if}
  </button>

  <!-- Error -->
  {#if error}
    <div class="p-3 bg-surface border border-error/30 rounded-md text-xs text-error">
      {error}
    </div>
  {/if}

  <!-- Output -->
  {#if output}
    <div class="flex-1 flex flex-col gap-2 min-h-0 animate-fade-in-up">
      <div class="flex-1 p-3 bg-surface border border-border rounded-md overflow-y-auto">
        <p class="text-sm text-foreground whitespace-pre-wrap leading-relaxed">{output.text}</p>
      </div>

      <div class="flex flex-col gap-1">
        <div class="flex items-center justify-between">
          <span class="text-[10px] text-muted">
            {output.input_tokens + output.output_tokens} tokens
            {#if copied}&middot; copied{/if}
          </span>
          <div class="flex gap-2">
            <button
              onclick={handleCopy}
              class="px-3 py-1.5 text-xs border border-border hover:border-secondary transition-colors cursor-pointer text-muted hover:text-foreground rounded-md"
            >
              {copied ? "Copied" : "Copy"}
            </button>
            <button
              onclick={handleInject}
              class="px-3 py-1.5 text-xs bg-accent text-white hover:bg-accent-hover transition-colors cursor-pointer rounded-md font-medium"
            >
              Inject
            </button>
          </div>
        </div>
        <p class="text-[10px] text-muted text-right">
          Text is on your clipboard — Cmd+V to paste manually
        </p>
      </div>
    </div>
  {/if}
</div>
