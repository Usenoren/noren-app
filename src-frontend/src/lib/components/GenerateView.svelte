<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { open } from "@tauri-apps/plugin-dialog";
  import { generate, generateComparison, getContextText, listFormats, injectGeneratedText, readFileAsText, getProfileOverview, createCheckout, type GenerateResult, type ComparisonResult } from "$lib/api/tauri";
  import { emit } from "@tauri-apps/api/event";
  import { open as openUrl } from "@tauri-apps/plugin-shell";
  import { isFree } from "$lib/stores/subscription.svelte";
  import { friendlyError } from "$lib/utils/errors";
  import LoadingSpinner from "./LoadingSpinner.svelte";

  // --- State ---
  let prompt = $state("");
  let format = $state("general");
  let level: "strict" | "guided" | "light" = $state("guided");
  let contextText = $state("");
  let detectedApp = $state("");
  let formats = $state<string[]>([]);
  let output = $state<GenerateResult | null>(null);
  let comparison = $state<ComparisonResult | null>(null);
  let compareMode = $state(false);
  let isGenerating = $state(false);
  let error = $state("");
  let attachedFiles = $state<{ name: string; content: string }[]>([]);
  let hasProfile = $state(true);
  let showCompareLock = $state(false);

  const levels = ["strict", "guided", "light"] as const;

  // --- Init ---
  $effect(() => {
    // Use profile overview for both profile status and format list
    // (works for both local and server-side profiles)
    getProfileOverview().then((overview) => {
      hasProfile = overview.exists;
      let f = overview.formats;
      if (!f.includes("general")) {
        f = ["general", ...f];
      }
      formats = f;
      if (!f.includes(format)) {
        format = f[0];
      }
    });

    // Also get local formats as fallback (for BYOK path)
    listFormats().then((f) => {
      if (formats.length <= 1) {
        if (!f.includes("general")) {
          f = ["general", ...f];
        }
        formats = f;
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
    comparison = null;

    try {
      const attachmentContents = attachedFiles.length > 0
        ? attachedFiles.map((f) => f.content)
        : undefined;

      if (compareMode) {
        comparison = await generateComparison({
          prompt: prompt.trim(),
          format,
          context: contextText || undefined,
          attachments: attachmentContents,
        });
        output = comparison.with_voice;
      } else {
        output = await generate({
          prompt: prompt.trim(),
          format,
          level,
          context: contextText || undefined,
          attachments: attachmentContents,
        });
      }
      if (output) {
        try {
          await navigator.clipboard.writeText(output.text);
          copied = true;
        } catch {
          // Clipboard API may not be available in Tauri webview — user can use Copy button
        }
      }
    } catch (e) {
      error = friendlyError(e);
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
      error = friendlyError(e);
    }
  }

  function clearContext() {
    contextText = "";
  }

  async function handleAttachFile() {
    if (attachedFiles.length >= 3) {
      error = "Maximum 3 attachments allowed";
      return;
    }

    try {
      const selected = await open({
        multiple: false,
        filters: [
          {
            name: "Documents",
            extensions: ["txt", "md", "csv", "json", "xml", "html", "pdf", "yaml", "yml", "toml"],
          },
        ],
      });

      if (!selected) return;

      const path = selected as string;
      const content = await readFileAsText(path);
      const name = path.split("/").pop() || path;

      attachedFiles = [...attachedFiles, { name, content }];
    } catch (e) {
      error = friendlyError(e);
    }
  }

  function removeAttachment(index: number) {
    attachedFiles = attachedFiles.filter((_, i) => i !== index);
  }
</script>

<div class="flex flex-col gap-3 h-full p-4 overflow-y-auto animate-fade-in-up">
  <!-- No profile nudge -->
  {#if !hasProfile}
    <div class="flex items-center gap-2 p-2 bg-tint border border-secondary/20 rounded-lg">
      <p class="flex-1 text-[10px] text-muted leading-relaxed">
        No voice profile yet — output will be generic.
        <button
          onclick={() => emit("navigate", "profiles")}
          class="text-secondary font-medium cursor-pointer hover:text-foreground"
        >Create one</button> or
        <button
          onclick={() => emit("navigate", "settings")}
          class="text-secondary font-medium cursor-pointer hover:text-foreground"
        >upgrade to Pro</button> for AI extraction.
      </p>
    </div>
  {/if}

  <!-- Format + Enforcement selectors -->
  <div class="flex items-center gap-2">
    <select
      bind:value={format}
      class="px-2 py-1 text-xs border border-border bg-surface text-foreground rounded-md focus:outline-none focus:border-secondary"
    >
      {#each formats as fmt}
        <option value={fmt}>{fmt}</option>
      {/each}
    </select>
    <button
      onclick={handleAttachFile}
      class="px-2 py-1 text-[10px] bg-surface text-muted border border-border hover:border-secondary hover:text-foreground transition-colors cursor-pointer rounded-md"
      title="Attach a file (PDF, text, etc.)"
      disabled={attachedFiles.length >= 3}
    >
      Attach{#if attachedFiles.length > 0} ({attachedFiles.length}/3){/if}
    </button>

    <div class="flex items-center gap-1.5 ml-auto">
      <!-- Compare toggle -->
      <div class="relative">
        <button
          onclick={() => {
            if (isFree()) {
              showCompareLock = true;
              setTimeout(() => { showCompareLock = false; }, 3000);
              return;
            }
            compareMode = !compareMode;
          }}
          class="px-2 py-1 text-[10px] transition-colors cursor-pointer rounded-md
            {compareMode
              ? 'bg-secondary text-white font-medium'
              : 'bg-surface text-muted border border-border hover:border-secondary hover:text-foreground'}"
          title={isFree() ? "Compare requires Pro" : "Compare with and without your voice"}
        >
          Compare
          {#if isFree()}
            <span class="ml-0.5 text-[8px] text-secondary font-medium">PRO</span>
          {/if}
        </button>
        {#if showCompareLock}
          <div class="absolute top-full mt-1 right-0 z-10 p-2 bg-tint border border-secondary/20 rounded-lg whitespace-nowrap animate-fade-in-up" style="box-shadow: var(--shadow-dropdown)">
            <p class="text-[10px] text-muted">Compare is a <span class="text-secondary font-medium">Pro</span> feature.</p>
            <button
              onclick={async () => { try { const r = await createCheckout("pro"); if (r.checkout_url !== "dev://granted") await openUrl(r.checkout_url); } catch {} }}
              class="mt-1 text-[10px] text-secondary font-medium cursor-pointer hover:text-foreground uppercase tracking-wide"
            >Upgrade</button>
          </div>
        {/if}
      </div>

      {#if !compareMode}
        <div class="flex gap-1">
          {#each levels as lvl}
            <button
              onclick={() => { level = lvl; }}
              class="px-1.5 py-1 text-[10px] transition-colors cursor-pointer uppercase tracking-wide rounded-md
                {level === lvl
                  ? 'bg-primary text-white font-medium'
                  : 'bg-surface text-muted border border-border hover:border-secondary hover:text-foreground'}"
            >
              {lvl}
            </button>
          {/each}
        </div>
      {/if}
    </div>
  </div>

  <!-- Prompt input area — context + attachments + textarea as one block -->
  <div class="border border-border rounded-lg bg-surface overflow-hidden focus-within:border-secondary transition-colors">
    <!-- Context banner -->
    {#if contextText}
      <div class="flex items-start gap-2 px-3 py-2 bg-tint/50 border-b border-border text-xs">
        <div class="flex-1 min-w-0">
          <span class="font-medium text-secondary">Context:</span>
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

    <!-- Attachments -->
    {#if attachedFiles.length > 0}
      <div class="flex items-center gap-1.5 flex-wrap px-3 py-1.5 {contextText ? '' : 'border-b border-border'}">
        {#each attachedFiles as file, i}
          <div class="inline-flex items-center gap-1 px-1.5 py-0.5 bg-tint border border-border rounded text-[10px] text-secondary">
            <span class="max-w-[120px] truncate">{file.name}</span>
            <button
              onclick={() => removeAttachment(i)}
              class="text-muted hover:text-error cursor-pointer ml-0.5"
              aria-label="Remove attachment"
            >&times;</button>
          </div>
        {/each}
      </div>
    {/if}

    <!-- Textarea -->
    <div class="relative">
      <textarea
        bind:value={prompt}
        onkeydown={(e) => { if (e.key === "Enter" && e.metaKey) handleGenerate(); }}
        class="w-full p-3 text-sm resize-none bg-transparent text-foreground placeholder-muted border-none focus:outline-none"
        rows={2}
        placeholder="What do you want to write?"
        disabled={isGenerating}
      ></textarea>
      <div class="absolute bottom-2 right-2 text-[10px] text-muted pointer-events-none">
        {#if !isGenerating}Cmd+Enter{/if}
      </div>
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
    <div class="p-3 bg-tint border border-border rounded-lg text-xs text-muted leading-relaxed">
      {error}
    </div>
  {/if}

  <!-- Output -->
  {#if comparison}
    <!-- Side-by-side comparison -->
    <div class="flex-1 flex flex-col gap-2 min-h-0 animate-fade-in-up">
      <div class="flex-1 grid grid-cols-2 gap-2 min-h-0">
        <div class="flex flex-col min-h-0">
          <span class="text-[10px] font-medium text-primary mb-1 uppercase tracking-wide">With your voice</span>
          <div class="flex-1 p-3 bg-surface border border-primary/30 rounded-lg overflow-y-auto">
            <p class="text-xs text-foreground whitespace-pre-wrap leading-relaxed">{comparison.with_voice.text}</p>
          </div>
        </div>
        <div class="flex flex-col min-h-0">
          <span class="text-[10px] font-medium text-muted mb-1 uppercase tracking-wide">Without voice</span>
          <div class="flex-1 p-3 bg-surface border border-border rounded-lg overflow-y-auto">
            <p class="text-xs text-foreground whitespace-pre-wrap leading-relaxed">{comparison.without_voice.text}</p>
          </div>
        </div>
      </div>

      <div class="flex items-center justify-between">
        <span class="text-[10px] text-muted">
          {comparison.with_voice.input_tokens + comparison.with_voice.output_tokens + comparison.without_voice.input_tokens + comparison.without_voice.output_tokens} tokens total
        </span>
        <div class="flex gap-2">
          <button
            onclick={handleCopy}
            class="px-3 py-1.5 text-xs border border-border hover:border-secondary transition-colors cursor-pointer text-muted hover:text-foreground rounded-md"
          >
            {copied ? "Copied" : "Copy voiced"}
          </button>
          <button
            onclick={handleInject}
            class="px-3 py-1.5 text-xs bg-accent text-white hover:bg-accent-hover transition-colors cursor-pointer rounded-md font-medium"
          >
            Inject voiced
          </button>
        </div>
      </div>
    </div>
  {:else if output}
    <div class="flex-1 flex flex-col gap-2 min-h-0 animate-fade-in-up">
      <div class="flex-1 p-3 bg-surface border border-border rounded-lg overflow-y-auto">
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
