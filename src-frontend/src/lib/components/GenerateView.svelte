<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { open } from "@tauri-apps/plugin-dialog";
  import { generate, generateComparison, getContextText, listFormats, injectGeneratedText, readFileAsText, getProfileOverview, getSettings, createCheckout, showMainWindow, logEdit, type GenerateResult, type ComparisonResult } from "$lib/api/tauri";
  import { emit } from "@tauri-apps/api/event";
  import { open as openUrl } from "@tauri-apps/plugin-shell";
  import { isFree, canExtract } from "$lib/stores/subscription.svelte";
  import { getIsExtracting } from "$lib/stores/extraction.svelte";
  import { friendlyError } from "$lib/utils/errors";
  import LoadingSpinner from "./LoadingSpinner.svelte";
  import { toastError } from "$lib/stores/toast.svelte";

  // --- Props ---
  let { isPopup = false, hasProfile: hasProfileProp = true, noApiKey = false }: { isPopup?: boolean; hasProfile?: boolean; noApiKey?: boolean } = $props();

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
  let mode: "generate" | "adapt" = $state("generate");
  let isGenerating = $state(false);
  let error = $state("");
  let attachedFiles = $state<{ name: string; content: string }[]>([]);
  let hasProfileLocal = $state(true);
  let hasProfile = $derived(isPopup ? hasProfileProp : hasProfileLocal);
  let noApiKeyLocal = $state(false);
  let noKey = $derived(isPopup ? noApiKey : noApiKeyLocal);
  let showCompareLock = $state(false);
  let dismissedEmpty = $state(false);
  let editedText = $state("");

  const levels = ["strict", "guided", "light"] as const;

  // --- Init ---
  $effect(() => {
    // Use profile overview for both profile status and format list
    // (works for both local and server-side profiles)
    getProfileOverview().then((overview) => {
      hasProfileLocal = overview.exists;
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

    getSettings().then((settings) => {
      if (settings.inference_mode === "byok" && !settings.has_key && settings.provider.requiresKey) {
        noApiKeyLocal = true;
      } else {
        noApiKeyLocal = false;
      }
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
          mode: mode !== "generate" ? mode : undefined,
          context: contextText || undefined,
          attachments: attachmentContents,
        });
      }
      if (output) {
        editedText = output.text;
        weaveComplete = true;
        setTimeout(() => { weaveComplete = false; }, 1000);
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
  let weaveComplete = $state(false);

  async function handleCopy() {
    if (!output) return;
    const text = editedText || output.text;
    if (editedText && editedText !== output.text) {
      try { await logEdit(format, output.text, editedText, "noren"); } catch (e) { console.error("logEdit failed:", e); }
    }
    await navigator.clipboard.writeText(text);
    copied = true;
    setTimeout(() => { copied = false; }, 1500);
  }

  async function handleInject() {
    if (!output) return;
    try {
      const text = editedText || output.text;
      if (editedText && editedText !== output.text) {
        try { await logEdit(format, output.text, editedText, "noren"); } catch (e) { console.error("logEdit failed:", e); }
      }
      await injectGeneratedText(text);
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
  <!-- Empty state: no profile, no output yet -->
  {#if !hasProfile && !getIsExtracting() && !output && !comparison && !dismissedEmpty && !isPopup}
    <div class="flex-1 flex flex-col items-center justify-center -m-4 overflow-hidden">
      <!-- Warm radial glow -->
      <div class="absolute inset-0 pointer-events-none" style="background: radial-gradient(ellipse 55% 45% at 50% 40%, var(--color-primary-muted), transparent 70%)"></div>

      <div class="relative flex flex-col items-center gap-8 animate-fade-in-up" style="animation-duration: 0.6s">
        <!-- Warp threads + accent weft -->
        <svg class="w-[120px] h-[68px]" viewBox="0 0 120 68" fill="none">
          <!-- Beam -->
          <line x1="18" y1="10" x2="102" y2="10" stroke="var(--color-primary)" stroke-width="1.5" stroke-linecap="round" opacity="0.2"/>

          <!-- Warp threads — staggered entrance -->
          {#each [
            { x: 36, delay: 0.15 },
            { x: 52, delay: 0.28 },
            { x: 68, delay: 0.41 },
            { x: 84, delay: 0.54 },
          ] as thread, i}
            <line
              x1={thread.x} y1="10" x2={thread.x} y2="64"
              stroke="var(--color-secondary)" stroke-width="1" stroke-linecap="round"
              stroke-dasharray="54" stroke-dashoffset="54"
              opacity={0.2 + (i % 2) * 0.15}
              style="animation: warp-appear 0.7s {thread.delay}s ease-out forwards"
            />
          {/each}

          <!-- Accent weft — draws after warps settle -->
          <path
            d="M26 36 C40 31, 50 40, 60 35 C70 30, 80 38, 94 33"
            stroke="var(--color-accent)" stroke-width="1.5" stroke-linecap="round"
            stroke-dasharray="80" stroke-dashoffset="80"
            style="animation: weft-weave 1s 0.9s ease-out forwards"
          />
        </svg>

        <div class="text-center max-w-[240px]">
          <h2 class="font-heading text-[21px] italic font-normal text-foreground leading-snug tracking-[-0.3px]">
            Waiting for your voice
          </h2>
          <p class="text-[11px] text-muted leading-[1.7] mt-3">
            Your voice profile tells Noren how you write. Extract it from samples or describe it yourself.
          </p>
        </div>

        <div class="flex flex-col items-center gap-3">
          {#if canExtract()}
            <button
              onclick={() => emit("navigate", "extract")}
              class="px-6 py-2.5 text-xs font-semibold bg-primary text-white hover:bg-primary-hover transition-all duration-200 cursor-pointer rounded-md hover:-translate-y-px"
              style="box-shadow: 0 2px 8px var(--color-primary-muted)"
            >
              Extract your voice
            </button>
            <button
              onclick={() => emit("navigate", "profiles")}
              class="text-[11px] text-secondary font-medium cursor-pointer hover:text-foreground transition-colors"
            >
              Or describe it manually
            </button>
          {:else}
            <button
              onclick={() => emit("navigate", "profiles")}
              class="px-6 py-2.5 text-xs font-semibold bg-primary text-white hover:bg-primary-hover transition-all duration-200 cursor-pointer rounded-md hover:-translate-y-px"
              style="box-shadow: 0 2px 8px var(--color-primary-muted)"
            >
              Create a voice profile
            </button>
            <button
              onclick={() => emit("navigate", "settings")}
              class="text-[11px] text-secondary font-medium cursor-pointer hover:text-foreground transition-colors"
            >
              Or upgrade to Pro for AI extraction
            </button>
          {/if}
        </div>

        <button
          onclick={() => { dismissedEmpty = true; }}
          class="text-[10px] text-muted cursor-pointer hover:text-foreground transition-colors mt-2"
        >
          Continue without a profile
        </button>

        {#if noKey}
          <div class="flex items-center gap-2 p-2.5 bg-tint border border-warning/20 rounded-lg max-w-[240px]">
            <p class="flex-1 text-[10px] text-muted leading-relaxed">
              API key also needed.
              <button
                onclick={() => emit("navigate", "settings")}
                class="text-secondary font-medium cursor-pointer hover:text-foreground"
              >Go to Settings</button>
            </p>
          </div>
        {/if}
      </div>
    </div>
  {:else}

  <!-- No API key banner -->
  {#if noKey}
    <div class="flex items-center gap-2 p-2 bg-tint border border-warning/20 rounded-lg">
      <p class="flex-1 text-[10px] text-muted leading-relaxed">
        Set up your API key to start generating.
        <button
          onclick={() => emit("navigate", "settings")}
          class="text-secondary font-medium cursor-pointer hover:text-foreground"
        >Go to Settings</button>
      </p>
    </div>
  {/if}

  <!-- No profile inline nudge (after dismissing empty state or in popup) -->
  {#if !hasProfile && !getIsExtracting()}
    <div class="flex items-center gap-2 p-2 bg-tint border border-secondary/20 rounded-lg">
      <p class="flex-1 text-[10px] text-muted leading-relaxed">
        {#if isPopup}
          No voice profile yet. Output will use default voice.
          <button
            onclick={() => showMainWindow()}
            class="text-secondary font-medium cursor-pointer hover:text-foreground"
          >Open Noren to set up</button>
        {:else if canExtract()}
          Output will be generic.
          <button
            onclick={() => emit("navigate", "extract")}
            class="text-secondary font-medium cursor-pointer hover:text-foreground"
          >Extract a profile</button> or
          <button
            onclick={() => emit("navigate", "profiles")}
            class="text-secondary font-medium cursor-pointer hover:text-foreground"
          >create one manually</button>.
        {:else}
          Output will be generic.
          <button
            onclick={() => emit("navigate", "profiles")}
            class="text-secondary font-medium cursor-pointer hover:text-foreground"
          >Create a profile</button> or
          <button
            onclick={() => emit("navigate", "settings")}
            class="text-secondary font-medium cursor-pointer hover:text-foreground"
          >upgrade to Pro</button> for AI extraction.
        {/if}
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

    <button
      onclick={() => { mode = mode === "generate" ? "adapt" : "generate"; }}
      class="px-2 py-1 text-[10px] transition-colors cursor-pointer rounded-md
        {mode === 'adapt'
          ? 'bg-secondary text-white font-medium'
          : 'bg-surface text-muted border border-border hover:border-secondary hover:text-foreground'}"
      title={mode === "adapt" ? "Adapt mode: paste existing content and restyle it in your voice" : "Adapt mode: restyle existing content in your voice instead of writing from scratch"}
    >
      Adapt
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
              onclick={async () => { try { const r = await createCheckout("pro"); if (r.checkout_url !== "dev://granted") await openUrl(r.checkout_url); } catch (e) { toastError(friendlyError(e)); } }}
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
              title={lvl === "strict"
                ? "Strict: maximizes voice fidelity, may constrain creativity"
                : lvl === "guided"
                  ? "Guided: balances your voice with natural flow (recommended)"
                  : "Light: uses your voice as a gentle guide, more creative freedom"}
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
        placeholder={mode === "adapt" ? "Paste content to restyle in your voice..." : "What do you want to write?"}
        disabled={isGenerating}
      ></textarea>
      <div class="absolute bottom-2 right-2 text-[10px] text-muted pointer-events-none">
        {#if !isGenerating}Cmd+Enter{/if}
      </div>
    </div>
  </div>

  <!-- Weave button -->
  <button
    onclick={handleGenerate}
    disabled={!prompt.trim() || isGenerating || noKey}
    class="w-full py-2.5 px-4 text-sm font-semibold tracking-wide transition-colors cursor-pointer rounded-md
      {!prompt.trim() || isGenerating || noKey
        ? 'bg-surface text-muted border border-border cursor-not-allowed opacity-50'
        : 'bg-primary text-white hover:bg-primary-hover'}
      {weaveComplete ? 'animate-loom-pulse' : ''}"
  >
    {#if isGenerating}
      <span class="inline-flex items-center gap-2 animate-breathe">
        <LoadingSpinner /> Weaving
      </span>
    {:else}
      Weave
    {/if}
  </button>

  <!-- Error -->
  {#if error}
    <div class="p-3 bg-tint border border-border rounded-lg text-xs text-muted leading-relaxed flex items-start gap-2">
      <span class="flex-1">{error}</span>
      <button
        onclick={() => { error = ""; handleGenerate(); }}
        class="shrink-0 px-2.5 py-1 text-[10px] font-medium bg-surface border border-border text-foreground rounded-md hover:border-secondary transition-colors cursor-pointer"
      >Retry</button>
    </div>
  {/if}

  <!-- Output -->
  {#if comparison}
    <!-- Side-by-side comparison -->
    <div class="flex-1 flex flex-col gap-2 min-h-0 animate-fabric-unfurl">
      <div class="flex-1 grid grid-cols-2 gap-2 min-h-0">
        <div class="flex flex-col min-h-0">
          <span class="text-[10px] font-medium text-primary mb-1 uppercase tracking-wide">With your voice</span>
          <div class="flex-1 p-3 rounded-lg overflow-y-auto output-accent-line" style="background:var(--color-warm-surface);border:1px solid rgba(0,0,0,0.04)">
            <div class="animate-shimmer rounded-lg">
              <p class="text-xs text-foreground whitespace-pre-wrap leading-relaxed">{comparison.with_voice.text}</p>
            </div>
          </div>
        </div>
        <div class="flex flex-col min-h-0">
          <span class="text-[10px] font-medium text-muted mb-1 uppercase tracking-wide">Without voice</span>
          <div class="flex-1 p-3 bg-surface border border-border rounded-lg overflow-y-auto">
            <div class="animate-shimmer rounded-lg">
              <p class="text-xs text-foreground whitespace-pre-wrap leading-relaxed">{comparison.without_voice.text}</p>
            </div>
          </div>
        </div>
      </div>

      <div class="flex items-center gap-2">
        <span class="font-mono text-[9px] text-muted mr-auto">
          {comparison.with_voice.input_tokens + comparison.with_voice.output_tokens + comparison.without_voice.input_tokens + comparison.without_voice.output_tokens} tokens
        </span>
        <button
          onclick={handleCopy}
          class="w-8 h-8 flex items-center justify-center border border-border hover:border-secondary transition-colors cursor-pointer rounded-md"
          title={copied ? "Copied" : "Copy voiced"}
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="text-muted">
            {#if copied}
              <path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7"/>
            {:else}
              <rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 01-2-2V4a2 2 0 012-2h9a2 2 0 012 2v1"/>
            {/if}
          </svg>
        </button>
        <button
          onclick={handleInject}
          class="flex items-center justify-center gap-1.5 h-8 px-3 text-xs font-semibold text-white transition-colors cursor-pointer rounded-md"
          style="background:var(--color-kon)"
        >
          Inject
          <kbd class="font-mono text-[8px] font-normal opacity-40 border border-white/15 px-1 py-px rounded">Cmd+Return</kbd>
        </button>
      </div>
    </div>
  {:else if output}
    <div class="flex-1 flex flex-col gap-2 min-h-0 animate-fabric-unfurl">
      <!-- Voice badge + format pills -->
      <div class="flex items-center gap-2">
        <span class="font-mono text-[9px] font-medium uppercase tracking-wide" style="color:var(--color-muted)">{format}</span>
        <span class="text-[9px]" style="color:var(--color-border)">/</span>
        <span class="font-mono text-[9px] font-medium uppercase tracking-wide" style="color:var(--color-muted)">{level}</span>
        <div class="ml-auto flex items-center gap-[5px]">
          <div class="w-[5px] h-[5px] rounded-full bg-signal animate-voice-pulse"></div>
          <span class="font-mono text-[8px] font-medium uppercase tracking-wide text-signal">Voice active</span>
        </div>
      </div>

      <!-- Output card (editable) -->
      <div class="flex-1 flex flex-col rounded-lg output-accent-line" style="background:var(--color-warm-surface);border:1px solid rgba(0,0,0,0.04)">
        <div class="animate-shimmer rounded-lg flex-1 flex flex-col min-h-0">
          <textarea
            bind:value={editedText}
            class="flex-1 w-full p-4 text-sm text-foreground bg-transparent resize-none min-h-0 border-none focus:outline-none selectable"
            style="line-height:1.75"
          ></textarea>
        </div>
      </div>

      <!-- Actions -->
      <div class="flex items-center gap-2">
        <span class="font-mono text-[9px] text-muted mr-auto">
          {output.input_tokens + output.output_tokens} tokens
          {#if editedText !== output.text}
            <span class="text-secondary font-medium ml-1.5">edited</span>
          {/if}
        </span>
        <button
          onclick={handleCopy}
          class="w-8 h-8 flex items-center justify-center border border-border hover:border-secondary transition-colors cursor-pointer rounded-md"
          title={copied ? "Copied" : "Copy"}
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="text-muted">
            {#if copied}
              <path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7"/>
            {:else}
              <rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 01-2-2V4a2 2 0 012-2h9a2 2 0 012 2v1"/>
            {/if}
          </svg>
        </button>
        <button
          onclick={handleInject}
          class="flex items-center justify-center gap-1.5 h-8 px-3 text-xs font-semibold text-white transition-colors cursor-pointer rounded-md"
          style="background:var(--color-kon)"
        >
          Inject
          <kbd class="font-mono text-[8px] font-normal opacity-40 border border-white/15 px-1 py-px rounded">Cmd+Return</kbd>
        </button>
      </div>
    </div>
  {/if}

  {/if}
</div>
