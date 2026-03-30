<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { open } from "@tauri-apps/plugin-dialog";
  import { generate, generateStream, generateComparison, cancelGeneration, getContextText, listFormats, injectGeneratedText, readFileAsText, getProfileOverview, getSettings, createCheckout, showMainWindow, logEdit, saveGeneration, listGenerations, loadGeneration, loadLatestGeneration, deleteGeneration, rewriteSelection, type GenerateResult, type ComparisonResult, type FixSpan, type Generation, type GenerationSummary } from "$lib/api/tauri";
  import { emit } from "@tauri-apps/api/event";
  import { open as openUrl } from "@tauri-apps/plugin-shell";
  import { isFree, canExtract } from "$lib/stores/subscription.svelte";
  import { getIsExtracting } from "$lib/stores/extraction.svelte";
  import { friendlyError } from "$lib/utils/errors";
  import LoadingSpinner from "./LoadingSpinner.svelte";
  import { toastError } from "$lib/stores/toast.svelte";
  import loomIdleUrl from "../../assets/loom-idle.png";

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
  let error = $state("");
  let attachedFiles = $state<{ name: string; content: string }[]>([]);
  let hasProfileLocal = $state(true);
  let hasProfile = $derived(isPopup ? hasProfileProp : hasProfileLocal);
  let noApiKeyLocal = $state(false);
  let noKey = $derived(isPopup ? noApiKey : noApiKeyLocal);
  let showCompareLock = $state(false);
  let dismissedEmpty = $state(false);
  let editedText = $state("");

  // --- Version history state ---
  let versions = $state<GenerationSummary[]>([]);
  let currentVersionId = $state<string | null>(null);
  let editMode = $state(false);
  let promptCollapsed = $state(false);
  let savedPrompt = $state("");
  let currentVersionIndex = $derived(currentVersionId ? versions.findIndex((v) => v.id === currentVersionId) : -1);
  let allGenerations = $state<GenerationSummary[]>([]);
  let savedFeedback = $state(false);

  // --- Instruction popup state ---
  let showInstructionPopup = $state(false);
  let selectedText = $state("");
  let selectionStart = $state(0);
  let selectionEnd = $state(0);
  let instructionInput = $state("");
  let isRewriting = $state(false);
  let textareaElement: HTMLTextAreaElement | undefined = $state();
  let refineInputElement: HTMLInputElement | undefined = $state();
  let streamCleanups: (() => void)[] = [];

  // --- Streaming state (ready for Pro streaming, currently wraps blocking call) ---
  let phase = $state<"idle" | "streaming" | "polishing" | "done">("idle");
  let streamedText = $state("");
  let cleanedText = $state("");
  let fixSpans = $state<FixSpan[]>([]);
  let cleanupStats = $state<{ found: number; fixed: number } | null>(null);
  let isGenerating = $derived(phase === "streaming" || phase === "polishing");

  // --- Helpers ---
  function generateId(): string {
    return Date.now().toString(36) + Math.random().toString(36).slice(2, 8);
  }

  function nowISO(): string {
    return new Date().toISOString().replace(/\.\d+Z$/, "Z");
  }

  function formatTimestamp(iso: string): string {
    const diffMs = Date.now() - new Date(iso).getTime();
    const diffMin = Math.floor(diffMs / 60000);
    if (diffMin < 1) return "just now";
    if (diffMin < 60) return `${diffMin}m ago`;
    const diffHr = Math.floor(diffMin / 60);
    if (diffHr < 24) return `${diffHr}h ago`;
    const diffDay = Math.floor(diffHr / 24);
    if (diffDay === 1) return "yesterday";
    if (diffDay < 7) return `${diffDay}d ago`;
    return new Date(iso).toLocaleDateString();
  }

  // --- Init ---
  $effect(() => {
    // Keyboard: Escape closes instruction popup
    function handleKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape" && showInstructionPopup) {
        e.preventDefault();
        closeInstructionPopup();
      }
    }
    window.addEventListener("keydown", handleKeyDown);

    // Restore last generation (fast: reads only the newest file, not all)
    loadLatestGeneration().then((gen) => {
      if (gen) {
        output = gen.output;
        editedText = gen.output.text;
        savedPrompt = gen.prompt;
        currentVersionId = gen.id;
        dismissedEmpty = true;
        versions = [{
          id: gen.id, timestamp: gen.timestamp, format: gen.format,
          prompt: gen.prompt, mode: gen.mode,
          token_count: gen.output.input_tokens + gen.output.output_tokens,
          is_edited: !gen.edits || gen.edits.length === 0 ? false : true,
        }];
      }
    }).catch(() => {});

    // Load all generations for history list
    listGenerations().then((gens) => { allGenerations = gens; }).catch(() => {});

    const defaultFormats = ["general", "blog", "tweet", "thread", "email", "linkedin", "newsletter", "essay"];

    getProfileOverview().then((overview) => {
      hasProfileLocal = overview.exists;
      // Merge profile formats with defaults, deduplicating
      const profileFormats = overview.formats || [];
      const merged = [...new Set([...defaultFormats, ...profileFormats])];
      formats = merged;
      if (!merged.includes(format)) {
        format = merged[0];
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

    return () => {
      cleanups.forEach((fn) => fn());
      window.removeEventListener("keydown", handleKeyDown);
    };
  });

  // Save a generation to disk and reset version nav to just this piece
  function persistGeneration(result: GenerateResult) {
    const id = generateId();
    const ts = nowISO();
    currentVersionId = id;
    const gen: Generation = {
      id,
      timestamp: ts,
      format,
      prompt: savedPrompt,
      mode,
      output: { text: result.text, input_tokens: result.input_tokens, output_tokens: result.output_tokens },
      edits: [],
    };
    // Reset version nav to just this generation (new piece = fresh start)
    versions = [{
      id, timestamp: ts, format, prompt: savedPrompt, mode,
      token_count: result.input_tokens + result.output_tokens, is_edited: false,
    }];
    // Persist to disk for history/learning, then refresh history list
    saveGeneration(gen).then(() => listGenerations()).then((gens) => { allGenerations = gens; }).catch(() => {});
  }

  // --- Actions ---
  async function handleGenerate() {
    if (!prompt.trim() || isGenerating) return;

    phase = "streaming";
    error = "";
    output = null;
    savedPrompt = prompt.trim();
    prompt = "";
    dismissedEmpty = true;
    promptCollapsed = false;
    editMode = false;
    closeInstructionPopup();
    comparison = null;
    streamedText = "";
    cleanedText = "";
    fixSpans = [];
    cleanupStats = null;

    const attachmentContents = attachedFiles.length > 0
      ? attachedFiles.map((f) => f.content)
      : undefined;

    // Compare mode: non-streaming
    if (compareMode) {
      try {
        comparison = await generateComparison({
          prompt: savedPrompt,
          format,
          context: contextText || undefined,
          attachments: attachmentContents,
        });
        output = comparison.with_voice;
        editedText = output.text;
      } catch (e) {
        error = friendlyError(e);
      } finally {
        phase = output ? "done" : "idle";
      }
      return;
    }

    // Streaming generation via Tauri events
    const cleanups: (() => void)[] = [];
    streamCleanups = cleanups;
    let cleanupTimeout: ReturnType<typeof setTimeout> | undefined;

    try {
      // Set up event listeners before starting the stream
      const deltaUn = await listen<{ text: string }>("gen:delta", (e) => {
        streamedText += e.payload.text;
      });
      cleanups.push(deltaUn);

      const doneUn = await listen<{ content: string; input_tokens: number; output_tokens: number }>("gen:done", (e) => {
        streamedText = e.payload.content;
        const result = { text: streamedText, input_tokens: e.payload.input_tokens, output_tokens: e.payload.output_tokens };
        output = result;
        editedText = streamedText;
        phase = "done";
        persistGeneration(result);
        weaveComplete = true;
        setTimeout(() => { weaveComplete = false; }, 1000);
      });
      cleanups.push(doneUn);

      const cleanupStartUn = await listen("gen:cleanup_start", () => {
        if (cleanupTimeout) clearTimeout(cleanupTimeout);
        phase = "polishing";
      });
      cleanups.push(cleanupStartUn);

      const cleanupDoneUn = await listen<{
        content: string; issues_found: number; issues_fixed: number;
        fix_spans: FixSpan[]; checks: unknown;
      }>("gen:cleanup_done", (e) => {
        if (cleanupTimeout) clearTimeout(cleanupTimeout);
        cleanedText = e.payload.content;
        fixSpans = e.payload.fix_spans || [];
        cleanupStats = { found: e.payload.issues_found, fixed: e.payload.issues_fixed };
        const tokens = output ? { input: output.input_tokens, output: output.output_tokens } : { input: 0, output: 0 };
        const result = { text: e.payload.content, input_tokens: tokens.input, output_tokens: tokens.output };
        output = result;
        editedText = e.payload.content;
        phase = "done";
        // Update existing version with cleaned text instead of creating a new one
        if (currentVersionId) {
          loadGeneration(currentVersionId).then((gen) => {
            gen.output = result;
            return saveGeneration(gen);
          }).catch(() => {});
        }
        weaveComplete = true;
        setTimeout(() => { weaveComplete = false; }, 1000);
      });
      cleanups.push(cleanupDoneUn);

      const errorUn = await listen<{ message: string }>("gen:error", (e) => {
        error = e.payload.message;
        phase = "idle";
      });
      cleanups.push(errorUn);

      // Start the stream. Race with a timeout so we don't hang forever
      // if the SSE connection doesn't close cleanly.
      const streamPromise = generateStream({
        prompt: savedPrompt,
        format,
        level,
        mode: mode !== "generate" ? mode : undefined,
        context: contextText || undefined,
        attachments: attachmentContents,
      });
      const timeoutPromise = new Promise<void>((resolve) => {
        setTimeout(resolve, 300_000); // 5 min max
      });
      await Promise.race([streamPromise, timeoutPromise]);

      // If stream ended without reaching done (edge case), finalize
      if (phase === "streaming" && streamedText) {
        if (cleanupTimeout) clearTimeout(cleanupTimeout);
        const result = { text: streamedText, input_tokens: 0, output_tokens: 0 };
        output = result;
        editedText = streamedText;
        phase = "done";
        persistGeneration(result);
        weaveComplete = true;
        setTimeout(() => { weaveComplete = false; }, 1000);
      } else if (phase === "streaming") {
        phase = "idle";
      }
    } catch (e) {
      error = friendlyError(e);
      phase = "idle";
    } finally {
      cleanups.forEach((fn) => fn());
      if (cleanupTimeout) clearTimeout(cleanupTimeout);
    }
  }

  async function handleCompare() {
    if (!output || isGenerating) return;
    if (isFree()) {
      showCompareLock = true;
      setTimeout(() => { showCompareLock = false; }, 3000);
      return;
    }

    const prevPhase = phase;
    phase = "streaming";
    error = "";

    try {
      comparison = await generateComparison({
        prompt: savedPrompt || prompt.trim(),
        format,
        context: contextText || undefined,
        attachments: attachedFiles.length > 0 ? attachedFiles.map((f) => f.content) : undefined,
      });
      compareMode = true;
      phase = "done";
    } catch (e) {
      error = friendlyError(e);
      phase = prevPhase;
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

  function handleCancelGeneration() {
    // Tell the Rust backend to stop the SSE stream
    cancelGeneration().catch(() => {});
    streamCleanups.forEach((fn) => fn());
    streamCleanups = [];
    phase = "idle";
    streamedText = "";
  }

  // --- Edit mode handlers ---

  async function handleEditModeToggle() {
    if (!editMode) {
      editMode = true;
      return;
    }
    // Exiting edit mode: if edited, save as a new version snapshot
    if (output && editedText !== output.text) {
      try {
        // Log edit for living profile learning
        await logEdit(format, output.text, editedText, "noren");

        // Create a new version snapshot (edited copy)
        const id = generateId();
        const ts = nowISO();
        const editedOutput = { text: editedText, input_tokens: output.input_tokens, output_tokens: output.output_tokens };
        const gen: Generation = {
          id, timestamp: ts, format, prompt: savedPrompt, mode,
          output: editedOutput, edits: [],
        };
        await saveGeneration(gen);

        // Update state
        output = editedOutput;
        currentVersionId = id;
        versions = [...versions, {
          id, timestamp: ts, format, prompt: savedPrompt, mode,
          token_count: output.input_tokens + output.output_tokens, is_edited: true,
        }];
        savedFeedback = true;
        setTimeout(() => { savedFeedback = false; }, 1500);
      } catch (e) {
        console.error("Failed to save edits:", e);
      }
    }
    editMode = false;
    closeInstructionPopup();
  }

  async function handleVersionClick(versionId: string) {
    // Auto-save edits before switching versions
    if (editMode && output && editedText !== output.text && currentVersionId) {
      try {
        await logEdit(format, output.text, editedText, "noren");
        const id = generateId();
        const ts = nowISO();
        const editedOutput = { text: editedText, input_tokens: output.input_tokens, output_tokens: output.output_tokens };
        await saveGeneration({ id, timestamp: ts, format, prompt: savedPrompt, mode, output: editedOutput, edits: [] });
        versions = [...versions, { id, timestamp: ts, format, prompt: savedPrompt, mode, token_count: output.input_tokens + output.output_tokens, is_edited: true }];
      } catch {}
    }
    try {
      const gen = await loadGeneration(versionId);
      output = gen.output;
      editedText = gen.output.text;
      currentVersionId = versionId;
      editMode = false;
      closeInstructionPopup();
    } catch (e) {
      console.error("Failed to load version:", e);
    }
  }

  async function handleDeleteVersion(e: Event, versionId: string) {
    e.stopPropagation();
    if (!confirm("Delete this version?")) return;
    try {
      await deleteGeneration(versionId);
      const gens = await listGenerations();
      versions = gens;
      if (currentVersionId === versionId) {
        if (gens.length > 0) {
          await handleVersionClick(gens[0].id);
        } else {
          output = null;
          editedText = "";
          currentVersionId = null;
          editMode = false;
        }
      }
    } catch (e) {
      console.error("Failed to delete version:", e);
    }
  }

  function navigateVersion(direction: "prev" | "next") {
    if (currentVersionIndex === -1 || versions.length === 0) return;
    // versions ordered oldest-first: ← = older (index-1), → = newer (index+1)
    const nextIdx = direction === "prev" ? currentVersionIndex - 1 : currentVersionIndex + 1;
    if (nextIdx < 0 || nextIdx >= versions.length) return;
    handleVersionClick(versions[nextIdx].id);
  }

  // --- Instruction popup handlers ---

  function handleTextareaMouseUp() {
    if (!editMode || !textareaElement) {
      showInstructionPopup = false;
      return;
    }
    const start = textareaElement.selectionStart;
    const end = textareaElement.selectionEnd;
    if (start === end) {
      showInstructionPopup = false;
      return;
    }
    selectedText = editedText.substring(start, end);
    selectionStart = start;
    selectionEnd = end;
    showInstructionPopup = true;
    // Focus the refine input after Svelte renders it
    requestAnimationFrame(() => { refineInputElement?.focus(); });
  }

  async function handleInstructionSubmit() {
    if (!instructionInput.trim() || !selectedText || !output || isRewriting) return;
    isRewriting = true;
    try {
      const result = await rewriteSelection({
        instruction: instructionInput,
        selectionText: selectedText,
        fullText: editedText,
        format,
      });
      const before = editedText.substring(0, selectionStart);
      const after = editedText.substring(selectionEnd);
      editedText = before + result.text + after;
      closeInstructionPopup();
    } catch (e) {
      error = friendlyError(e);
    } finally {
      isRewriting = false;
    }
  }

  function closeInstructionPopup() {
    showInstructionPopup = false;
    instructionInput = "";
    selectedText = "";
  }

  async function handleHistoryClick(id: string) {
    try {
      const gen = await loadGeneration(id);
      output = gen.output;
      editedText = gen.output.text;
      savedPrompt = gen.prompt;
      currentVersionId = gen.id;
      format = gen.format;
      versions = [{
        id: gen.id, timestamp: gen.timestamp, format: gen.format,
        prompt: gen.prompt, mode: gen.mode,
        token_count: gen.output.input_tokens + gen.output.output_tokens,
        is_edited: false,
      }];
      phase = "done";
    } catch (e) {
      console.error("Failed to load generation:", e);
    }
  }

  async function handleClearHistory() {
    for (const g of allGenerations) {
      try { await deleteGeneration(g.id); } catch {}
    }
    allGenerations = [];
  }
</script>

<div class="flex flex-col h-full animate-fade-in-up">
  <!-- Empty state: no profile, no output yet -->
  {#if !hasProfile && !getIsExtracting() && !output && !comparison && !dismissedEmpty && !isPopup}
    <div class="flex-1 flex flex-col items-center justify-center overflow-hidden relative">
      <div class="absolute inset-0 pointer-events-none" style="background: radial-gradient(ellipse 55% 45% at 50% 40%, var(--color-primary-muted), transparent 70%)"></div>

      <div class="relative flex flex-col items-center gap-8 animate-fade-in-up" style="animation-duration: 0.6s">
        <img src={loomIdleUrl} alt="" class="w-[130px] loom-idle-img" />

        <div class="text-center max-w-[280px]">
          <h2 class="font-heading text-[32px] italic font-normal text-foreground leading-snug tracking-[-0.3px]">
            Write like yourself
          </h2>
          <p class="text-[11px] text-muted leading-[1.7] mt-3">
            Your voice profile tells Noren how you write. Extract it from samples or describe it yourself.
          </p>
        </div>

        <div class="flex flex-col items-center gap-3">
          {#if canExtract()}
            <button
              onclick={() => emit("navigate", "extract")}
              class="px-6 py-2.5 text-xs font-semibold bg-accent text-white hover:bg-accent-hover transition-all duration-200 cursor-pointer rounded-md hover:-translate-y-px"
              style="box-shadow: 0 2px 8px var(--color-accent-glow)"
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
              class="px-6 py-2.5 text-xs font-semibold bg-accent text-white hover:bg-accent-hover transition-all duration-200 cursor-pointer rounded-md hover:-translate-y-px"
              style="box-shadow: 0 2px 8px var(--color-accent-glow)"
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

  <div class="flex flex-col h-full">
    <!-- Toolbar -->
    <div class="w-toolbar">
      <span class="w-toolbar-title">Weave</span>
      {#if output || isGenerating}
        <button
          onclick={() => { output = null; editedText = ""; savedPrompt = ""; currentVersionId = null; versions = []; phase = "idle"; editMode = false; closeInstructionPopup(); comparison = null; compareMode = false; listGenerations().then((gens) => { allGenerations = gens; }).catch(() => {}); }}
          class="w-new-btn"
          title="New piece"
        >+ New</button>
      {/if}
      <span style="flex: 1"></span>
      {#if noKey}
        <span class="w-toolbar-pill" style="color: var(--color-muted); font-size: 10px; border-color: rgba(200,160,60,0.3)">
          No API key.
          <button onclick={() => emit("navigate", "settings")} style="color: var(--color-secondary); background: none; border: none; cursor: pointer; font-family: inherit; font-size: 10px; font-weight: 600">Settings</button>
        </span>
      {:else}
        <select bind:value={format} class="w-toolbar-pill w-toolbar-select">
          {#each formats as fmt}
            <option value={fmt}>{fmt.charAt(0).toUpperCase() + fmt.slice(1)}</option>
          {/each}
        </select>

        <button onclick={handleAttachFile} class="w-toolbar-pill" disabled={attachedFiles.length >= 3}>
          Attach{#if attachedFiles.length > 0} ({attachedFiles.length}/3){/if}
        </button>

        <button
          onclick={() => { mode = mode === "generate" ? "adapt" : "generate"; }}
          class="w-toolbar-pill"
          class:active={mode === "adapt"}
        >Adapt</button>
      {/if}
    </div>

    <!-- No profile inline nudge -->
    {#if !hasProfile && !getIsExtracting() && !noKey}
      <div class="flex items-center gap-2 mx-4 mb-3 p-2 bg-tint border border-secondary/20 rounded-lg">
        <p class="flex-1 text-[10px] text-muted leading-relaxed">
          {#if isPopup}
            No voice profile yet. Output won't carry your voice.
            <button
              onclick={() => showMainWindow()}
              class="text-secondary font-medium cursor-pointer hover:text-foreground"
            >Open Noren to set up</button>
          {:else if canExtract()}
            Output won't carry your voice.
            <button
              onclick={() => emit("navigate", "extract")}
              class="text-secondary font-medium cursor-pointer hover:text-foreground"
            >Extract a profile</button> or
            <button
              onclick={() => emit("navigate", "profiles")}
              class="text-secondary font-medium cursor-pointer hover:text-foreground"
            >create one manually</button>.
          {:else}
            Output won't carry your voice.
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

    <!-- Composition workspace -->
    <div class="flex-1 min-h-0 overflow-y-auto px-6">
      {#if phase === "idle" && !comparison && !output}
        <!-- Idle state with history -->
        <div class="flex flex-col items-center pt-12 pb-6 gap-8" style="max-width: 660px; margin: 0 auto; width: 100%;">
          <div class="idle-hero">
            <img src={loomIdleUrl} alt="" class="idle-hero-img" />
            <p class="idle-hero-title">Ready to weave</p>
            <p class="idle-hero-sub">{hasProfile ? "Type below to start, or resume a recent piece" : "Type below to start writing"}</p>
          </div>

          {#if allGenerations.length > 0}
            <div class="history-section">
              <div class="history-header">
                <span class="history-label">Recent</span>
                <button class="history-clear" onclick={handleClearHistory}>Clear history</button>
              </div>
              <div class="gen-list">
                {#each allGenerations as g, i}
                  <button class="gen-item" class:latest={i === 0} onclick={() => handleHistoryClick(g.id)}>
                    <div class="gen-item-content">
                      <div class="gen-item-prompt">{g.prompt}</div>
                      <div class="gen-item-meta">
                        <span class="gen-item-format">{g.format}</span>
                        <span class="gen-item-dot">&middot;</span>
                        <span class="gen-item-time">{formatTimestamp(g.timestamp)}</span>
                        <span class="gen-item-dot">&middot;</span>
                        <span class="gen-item-tokens">{g.token_count.toLocaleString()} tokens</span>
                      </div>
                    </div>
                  </button>
                {/each}
              </div>
            </div>
          {/if}
        </div>
      {:else if comparison}
        <!-- Side-by-side comparison -->
        <div class="flex flex-col gap-2 h-full animate-fabric-unfurl">
          <div class="flex-1 grid grid-cols-2 gap-2 min-h-0">
            <div class="flex flex-col min-h-0">
              <span class="font-heading italic text-[11px] text-accent mb-1 tracking-wide">With your voice</span>
              <div class="flex-1 p-3 overflow-y-auto output-card output-weave-bg">
                <p class="font-heading italic text-xs text-foreground whitespace-pre-wrap" style="line-height:1.75">{comparison.with_voice.text}</p>
              </div>
            </div>
            <div class="flex flex-col min-h-0">
              <span class="font-heading italic text-[11px] text-muted mb-1 tracking-wide">Without voice</span>
              <div class="flex-1 p-3 bg-surface border border-border rounded-lg overflow-y-auto opacity-75">
                <p class="text-xs text-foreground whitespace-pre-wrap leading-relaxed">{comparison.without_voice.text}</p>
              </div>
            </div>
          </div>
          <div class="flex items-center gap-2 pb-2">
            <span class="font-mono text-[9px] text-muted mr-auto">
              {comparison.with_voice.input_tokens + comparison.with_voice.output_tokens + comparison.without_voice.input_tokens + comparison.without_voice.output_tokens} tokens
            </span>
            <button onclick={handleCopy} class="w-8 h-8 flex items-center justify-center border border-border hover:border-secondary transition-colors cursor-pointer rounded-md" title={copied ? "Copied" : "Copy voiced"}>
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="text-muted">
                {#if copied}<path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7"/>{:else}<rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 01-2-2V4a2 2 0 012-2h9a2 2 0 012 2v1"/>{/if}
              </svg>
            </button>
            <button onclick={handleInject} class="flex items-center justify-center gap-1.5 h-8 px-3 text-xs font-semibold text-white transition-colors cursor-pointer rounded-md bg-accent">
              Use This
            </button>
            <button onclick={() => { comparison = null; compareMode = false; }} class="inline-flex items-center gap-1 text-[10px] text-muted hover:text-foreground transition-colors cursor-pointer ml-2">
              Back to draft
            </button>
          </div>
        </div>
      {:else if output || (isGenerating && !output)}
        <!-- Composition: one surface, no boxes -->
        <div class="w-composition">

          <!-- Prompt reference: inline text, not a box -->
          {#if savedPrompt}
            <div class="w-prompt-ref" class:collapsed={promptCollapsed} role="button" tabindex="0" onclick={() => { promptCollapsed = !promptCollapsed; }} onkeydown={(e) => { if (e.key === "Enter") promptCollapsed = !promptCollapsed; }}>
              <span class="w-prompt-icon">{promptCollapsed ? "\u25B6" : "\u25BC"}</span>
              <span class="w-prompt-text">{savedPrompt}</span>
            </div>
          {/if}

          <!-- Draft: text on surface, no container border -->
          <div class="w-draft" class:editing={editMode}>
            {#if isGenerating && !output}
              <div class="w-streaming-badge">
                <LoadingSpinner /> <span>Weaving...</span>
                <button class="w-stop-btn" onclick={handleCancelGeneration}>Stop</button>
              </div>
              <div class="w-draft-text" style="opacity: 0.5">{streamedText || "\u00A0"}</div>
            {:else if showInstructionPopup}
              <!-- Show text with highlight while refining -->
              <!-- svelte-ignore a11y_click_events_have_key_events -->
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <div class="w-draft-text" style="white-space: pre-wrap; cursor: text" onclick={() => closeInstructionPopup()}>{editedText.substring(0, selectionStart)}<span class="w-highlight">{editedText.substring(selectionStart, selectionEnd)}</span>{editedText.substring(selectionEnd)}</div>
            {:else}
              <textarea
                bind:this={textareaElement}
                bind:value={editedText}
                readonly={!editMode}
                onmouseup={handleTextareaMouseUp}
                class="w-draft-text"
              ></textarea>
            {/if}
          </div>

          <!-- Action bar: sticky bottom, integrated -->
          {#if output}
            <div class="w-actions">
              <button class="w-btn w-btn-primary" onclick={handleInject}>Use This</button>
              <button class="w-btn w-btn-ghost" onclick={handleCopy}>{copied ? "Copied" : "Copy"}</button>
              <button class="w-btn w-btn-ghost" class:active={editMode} onclick={handleEditModeToggle}>
                {editMode ? "Done" : "Edit"}
              </button>
              {#if savedFeedback}
                <span class="w-save-indicator">Saved</span>
              {/if}

              {#if versions.length > 1}
                <div class="w-version-compact">
                  <span>v</span>
                  <span class="w-version-badge">{currentVersionIndex >= 0 ? currentVersionIndex + 1 : versions.length} / {versions.length}</span>
                  <button class="w-version-arrow" disabled={currentVersionIndex <= 0} onclick={() => navigateVersion("prev")}>&larr;</button>
                  <button class="w-version-arrow" disabled={currentVersionIndex >= versions.length - 1} onclick={() => navigateVersion("next")}>&rarr;</button>
                </div>
              {/if}

              <span class="w-actions-spacer"></span>

              <span class="w-meta">
                {output.input_tokens + output.output_tokens} tokens
                {#if editedText !== output.text}<span style="color: var(--color-secondary); margin-left: 4px">edited</span>{/if}
              </span>
              <div class="relative">
                <button class="w-meta-link" onclick={handleCompare} disabled={isGenerating}>
                  <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="3" width="7" height="18" rx="1"/><rect x="14" y="3" width="7" height="18" rx="1"/></svg>
                  Compare
                  {#if isFree()}
                    <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="opacity: 0.5"><rect x="3" y="11" width="18" height="11" rx="2" ry="2"/><path d="M7 11V7a5 5 0 0110 0v4"/></svg>
                  {/if}
                </button>
                {#if showCompareLock}
                  <div class="absolute bottom-full mb-1 right-0 z-10 p-2 bg-tint border border-secondary/20 rounded-lg whitespace-nowrap animate-fade-in-up" style="box-shadow: var(--shadow-dropdown)">
                    <p class="text-[10px] text-muted">Compare is a <span class="text-secondary font-medium">Pro</span> feature.</p>
                    <button
                      onclick={async () => { try { const r = await createCheckout("pro"); if (r.checkout_url !== "dev://granted") await openUrl(r.checkout_url); } catch (e) { toastError(friendlyError(e)); } }}
                      class="mt-1 text-[10px] text-secondary font-medium cursor-pointer hover:text-foreground uppercase tracking-wide"
                    >Upgrade</button>
                  </div>
                {/if}
              </div>
            </div>
          {/if}
        </div>
      {/if}
    </div>

    <!-- Error -->
    {#if error}
      <div class="mx-4 mb-2 p-3 bg-tint border border-border rounded-lg text-xs text-muted leading-relaxed flex items-start gap-2 shrink-0">
        <span class="flex-1">{error}</span>
        <button
          onclick={() => { error = ""; handleGenerate(); }}
          class="shrink-0 px-2.5 py-1 text-[10px] font-medium bg-surface border border-border text-foreground rounded-md hover:border-secondary transition-colors cursor-pointer"
        >Retry</button>
      </div>
    {/if}

    <!-- Input bar -->
    <div class="w-input-bar">
      <div class="w-input-inner">
      {#if contextText}
        <div class="flex items-start gap-2 mb-2 px-3 py-2 bg-tint/50 border border-border rounded-lg text-xs">
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

      {#if attachedFiles.length > 0}
        <div class="flex items-center gap-1.5 flex-wrap mb-1.5">
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

      <div class="flex items-end gap-2">
        {#if showInstructionPopup}
          <!-- Refine mode: replaces the generate input -->
          <span class="w-refine-label" style="align-self: center">Refine</span>
          {#if selectedText}
            <span class="w-refine-selection" style="align-self: center">"{selectedText.length > 40 ? selectedText.slice(0, 40) + "..." : selectedText}"</span>
          {/if}
          <input
            type="text"
            bind:this={refineInputElement}
            bind:value={instructionInput}
            placeholder="tighten this, add humor..."
            onkeydown={(e) => {
              if (e.key === "Enter" && !isRewriting) handleInstructionSubmit();
              if (e.key === "Escape") closeInstructionPopup();
            }}
            class="w-refine-input"
            disabled={isRewriting}
          />
          <button onclick={handleInstructionSubmit} disabled={isRewriting || !instructionInput.trim()} class="w-refine-submit" title="Submit (Enter)">
            {#if isRewriting}
              <LoadingSpinner />
            {:else}
              <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 8h10M9 4l4 4-4 4"/></svg>
            {/if}
          </button>
          <button onclick={closeInstructionPopup} class="w-toolbar-pill" style="font-size: 10px; padding: 4px 8px">Esc</button>
        {:else}
          <!-- Normal generate input -->
          <textarea
            bind:value={prompt}
            onkeydown={(e) => { if (e.key === "Enter" && e.metaKey) handleGenerate(); }}
            class="flex-1 py-[10px] px-3 text-[13px] resize-none text-foreground placeholder-muted border border-border rounded-lg focus:outline-none focus:border-secondary"
            style="background: rgba(255,255,255,0.02)"
            rows={1}
            placeholder={mode === "adapt" ? "Paste content to restyle in your voice..." : output ? "Generate again with a new prompt..." : "What do you want to write?"}
            disabled={isGenerating}
          ></textarea>
          <button
            onclick={handleGenerate}
            disabled={!prompt.trim() || isGenerating || noKey}
            class="w-[38px] h-[38px] rounded-lg transition-colors cursor-pointer shrink-0 flex items-center justify-center
              {!prompt.trim() || isGenerating || noKey
                ? 'bg-surface text-muted border border-border cursor-not-allowed opacity-50'
                : 'bg-accent text-white hover:bg-accent-hover'}
              {weaveComplete ? 'animate-loom-pulse' : ''}"
            style={!(!prompt.trim() || isGenerating || noKey) ? 'box-shadow: 0 0 12px var(--color-accent-glow)' : ''}
            title="Generate (Cmd+Return)"
          >
            {#if isGenerating}
              <LoadingSpinner />
            {:else}
              <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                <path stroke-linecap="round" stroke-linejoin="round" d="M6 12L3.269 3.126A59.768 59.768 0 0121.485 12 59.77 59.77 0 013.27 20.876L5.999 12zm0 0h7.5" />
              </svg>
            {/if}
          </button>
        {/if}
      </div>
      </div>
    </div>
  </div>

  {/if}
</div>

<style>
  /* === TOOLBAR === */
  .w-toolbar {
    display: flex; align-items: center; gap: 8px;
    padding: 10px 24px; border-bottom: 1px solid var(--color-border); flex-shrink: 0;
    width: 100%; box-sizing: border-box;
  }
  .w-toolbar-title {
    font-family: "Newsreader", serif; font-style: italic;
    font-size: 17px; color: var(--color-foreground); opacity: 0.7;
  }
  .w-toolbar-pill {
    padding: 5px 10px; border: 1px solid var(--color-border); border-radius: 6px;
    background: transparent; color: var(--color-muted); font-size: 11px;
    font-family: inherit; cursor: pointer; transition: all 150ms ease;
  }
  .w-toolbar-pill:hover { border-color: var(--color-muted); color: var(--color-foreground); }
  .w-toolbar-pill.active { border-color: var(--color-secondary); color: var(--color-foreground); background: rgba(59,107,138,0.08); }
  .w-toolbar-pill:disabled { opacity: 0.4; cursor: not-allowed; }
  .w-toolbar-select {
    appearance: none; -webkit-appearance: none;
    padding-right: 26px;
    background-image: url("data:image/svg+xml;charset=utf-8,%3Csvg xmlns='http://www.w3.org/2000/svg' width='10' height='6'%3E%3Cpath d='M0 0l5 6 5-6z' fill='%23888'/%3E%3C/svg%3E");
    background-repeat: no-repeat; background-position: right 8px center; background-size: 10px 6px;
  }

  /* === NEW BUTTON === */
  .w-new-btn {
    padding: 5px 12px; border-radius: 6px; border: 1px solid var(--color-border);
    background: transparent; color: var(--color-muted); cursor: pointer;
    display: inline-flex; align-items: center; gap: 4px;
    transition: all 150ms ease; font-size: 11px; font-weight: 500; font-family: inherit;
  }
  .w-new-btn:hover { border-color: var(--color-foreground); color: var(--color-foreground); }

  /* === IDLE === */
  .idle-hero {
    display: flex; flex-direction: column; align-items: center; gap: 8px;
    animation: w-fadeDown 500ms cubic-bezier(0.16, 1, 0.3, 1) both;
  }
  @keyframes w-fadeDown { from { opacity: 0; transform: translateY(-10px); } to { opacity: 1; transform: translateY(0); } }
  .idle-hero-img { width: 80px; opacity: 0.4; filter: invert(1); }
  .idle-hero-title { font-family: "Newsreader", serif; font-style: italic; font-size: 20px; font-weight: 400; color: var(--color-foreground); opacity: 0.6; }
  .idle-hero-sub { font-size: 11px; color: var(--color-muted); opacity: 0.7; }

  /* === HISTORY === */
  .history-section { width: 100%; animation: w-fadeUp 500ms 150ms cubic-bezier(0.16, 1, 0.3, 1) both; }
  @keyframes w-fadeUp { from { opacity: 0; transform: translateY(12px); } to { opacity: 1; transform: translateY(0); } }
  .history-header { display: flex; align-items: center; justify-content: space-between; margin-bottom: 12px; padding: 0 2px; }
  .history-label { font-size: 9px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.8px; color: var(--color-muted); opacity: 0.6; }
  .history-clear { font-size: 10px; color: var(--color-muted); background: none; border: none; cursor: pointer; opacity: 0; transition: opacity 200ms ease, color 200ms ease; font-family: inherit; padding: 2px 6px; border-radius: 4px; }
  .history-section:hover .history-clear { opacity: 0.5; }
  .history-clear:hover { opacity: 1 !important; color: var(--color-accent); }
  .gen-list { display: flex; flex-direction: column; gap: 2px; }
  .gen-item { display: flex; align-items: flex-start; gap: 12px; padding: 12px 14px; border-radius: 8px; cursor: pointer; transition: all 150ms ease; border: none; background: none; text-align: left; font-family: inherit; width: 100%; position: relative; border-left: 2px solid transparent; color: var(--color-foreground); }
  .gen-item:hover { background: rgba(255, 255, 255, 0.03); }
  .gen-item.latest { border-left-color: var(--color-accent); }
  .gen-item.latest::before { content: ''; position: absolute; left: 0; top: 0; bottom: 0; width: 100%; background: linear-gradient(90deg, rgba(122,51,64,0.1), transparent 60%); border-radius: 8px; pointer-events: none; }
  .gen-item-content { flex: 1; min-width: 0; position: relative; }
  .gen-item-prompt { font-size: 12px; font-style: italic; color: var(--color-foreground); opacity: 0.85; line-height: 1.5; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; margin-bottom: 6px; }
  .gen-item:hover .gen-item-prompt { opacity: 1; }
  .gen-item-meta { display: flex; align-items: center; gap: 8px; }
  .gen-item-format { font-size: 9px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.4px; padding: 2px 7px; border-radius: 4px; background: rgba(122, 51, 64, 0.08); color: var(--color-accent); border: 1px solid rgba(122, 51, 64, 0.12); }
  .gen-item-time { font-size: 9px; color: var(--color-muted); opacity: 0.7; }
  .gen-item-tokens { font-size: 9px; color: var(--color-muted); opacity: 0.5; }
  .gen-item-dot { color: var(--color-muted); opacity: 0.3; font-size: 8px; }
  .gen-list::-webkit-scrollbar { width: 3px; }
  .gen-list::-webkit-scrollbar-track { background: transparent; }
  .gen-list::-webkit-scrollbar-thumb { background: var(--color-border); border-radius: 2px; }

  /* === COMPOSITION === */
  .w-composition { max-width: 780px; margin: 0 auto; width: 100%; padding: 32px 0 120px 0; }

  /* Prompt ref: inline italic text, no box */
  .w-prompt-ref {
    padding: 32px 0 20px 0; display: flex; align-items: baseline; gap: 8px;
    cursor: pointer; user-select: none;
  }
  .w-prompt-icon { font-size: 8px; color: var(--color-muted); opacity: 0.4; transition: transform 200ms ease; flex-shrink: 0; }
  .w-prompt-ref.collapsed .w-prompt-icon { transform: rotate(-90deg); }
  .w-prompt-text {
    font-size: 12px; font-style: italic; color: var(--color-muted); line-height: 1.5;
    transition: color 150ms ease; overflow: hidden;
    display: -webkit-box; -webkit-line-clamp: 1; line-clamp: 1; -webkit-box-orient: vertical;
  }
  .w-prompt-ref:hover .w-prompt-text { color: var(--color-foreground); }
  .w-prompt-ref:not(.collapsed) .w-prompt-text { -webkit-line-clamp: unset; line-clamp: unset; }
  .w-prompt-ref::after { content: ''; flex: 1; height: 1px; background: var(--color-border); opacity: 0.2; margin-left: 16px; align-self: center; }

  /* Draft: no box, text on surface */
  .w-draft { position: relative; padding: 4px 0; }
  .w-draft::after {
    content: 'draft'; position: absolute; top: 0; right: 0;
    font-family: "Plus Jakarta Sans", sans-serif; font-style: normal;
    font-size: 8px; font-weight: 700; text-transform: uppercase;
    letter-spacing: 1.2px; color: var(--color-muted); opacity: 0.3; pointer-events: none;
  }
  .w-draft.editing { padding-left: 20px; border-left: 2px solid var(--color-accent); margin-left: -22px; }
  .w-draft.editing::after { content: 'editing'; color: var(--color-accent); opacity: 0.7; }
  .w-draft-text {
    width: 100%; font-family: "Newsreader", serif; font-style: italic; font-size: 16px;
    line-height: 1.9; letter-spacing: -0.15px; color: var(--color-foreground);
    white-space: pre-wrap; outline: none; padding: 0; border: none;
    background: transparent; resize: none; field-sizing: content; min-height: 200px;
  }
  .w-draft-text::selection { background: rgba(122, 51, 64, 0.15); }
  .w-highlight { background: rgba(122, 51, 64, 0.2); border-radius: 2px; }

  /* Streaming */
  .w-streaming-badge {
    position: absolute; top: 0; right: 0; z-index: 2;
    display: flex; align-items: center; gap: 6px;
    font-size: 10px; color: var(--color-accent); font-weight: 600;
    text-transform: uppercase; letter-spacing: 0.3px;
  }
  .w-stop-btn {
    font-size: 10px; font-weight: 500; color: var(--color-muted);
    background: none; border: 1px solid var(--color-border); border-radius: 4px;
    padding: 2px 8px; cursor: pointer; font-family: inherit; transition: all 150ms ease;
    text-transform: none; letter-spacing: 0;
  }
  .w-stop-btn:hover { border-color: var(--color-accent); color: var(--color-accent); }

  /* Refine bar: kept for potential future use */
  @keyframes w-slideUp { from { opacity: 0; transform: translateY(8px); } to { opacity: 1; transform: translateY(0); } }
  .w-refine-label { font-size: 9px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.6px; color: var(--color-accent); white-space: nowrap; flex-shrink: 0; }
  .w-refine-selection { font-size: 11px; font-style: italic; color: var(--color-muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 180px; flex-shrink: 0; }
  .w-refine-input {
    padding: 8px 14px; border: 1px solid var(--color-border); border-radius: 20px;
    font-size: 13px; font-family: inherit; background: var(--color-surface);
    color: var(--color-foreground); min-width: 200px; flex: 1; transition: all 150ms ease;
  }
  .w-refine-input::placeholder { color: var(--color-muted); }
  .w-refine-input:focus { outline: none; border-color: var(--color-accent); box-shadow: 0 0 0 3px rgba(122,51,64,0.1); }
  .w-refine-submit {
    width: 32px; height: 32px; border: 1px solid var(--color-border); border-radius: 6px;
    background: var(--color-surface); cursor: pointer; display: flex; align-items: center;
    justify-content: center; color: var(--color-accent);
    transition: all 150ms cubic-bezier(0.16,1,0.3,1); flex-shrink: 0;
  }
  .w-refine-submit:hover:not(:disabled) { border-color: var(--color-accent); background: rgba(122,51,64,0.04); transform: translateY(-1px); }
  .w-refine-submit:disabled { opacity: 0.4; cursor: not-allowed; }

  /* Action bar: sticky bottom */
  .w-actions {
    position: sticky; bottom: 0; display: flex; align-items: center; gap: 6px;
    padding: 12px 0; margin-top: 24px;
  }
  .w-btn {
    padding: 7px 14px; border: none; border-radius: 6px; font-size: 11px; font-weight: 600;
    cursor: pointer; transition: all 150ms cubic-bezier(0.16, 1, 0.3, 1); font-family: inherit;
    display: inline-flex; align-items: center; gap: 4px;
  }
  .w-btn-primary { background: var(--color-accent); color: white; }
  .w-btn-primary:hover { filter: brightness(1.15); transform: translateY(-1px); box-shadow: 0 4px 12px rgba(122,51,64,0.2); }
  .w-btn-ghost { background: var(--color-surface); color: var(--color-muted); border: 1px solid var(--color-border); }
  .w-btn-ghost:hover { border-color: var(--color-muted); color: var(--color-foreground); }
  .w-btn-ghost.active { border-color: var(--color-accent); color: var(--color-accent); }
  .w-actions-spacer { flex: 1; }
  .w-meta { font-size: 9px; color: var(--color-muted); opacity: 0.6; }
  .w-meta-link {
    font-size: 10px; color: var(--color-muted); opacity: 0.6; background: none; border: none;
    cursor: pointer; font-family: inherit; transition: color 150ms ease;
    display: inline-flex; align-items: center; gap: 4px;
  }
  .w-meta-link:hover { color: var(--color-foreground); opacity: 1; }
  .w-save-indicator {
    font-size: 10px; font-weight: 600; color: var(--color-accent);
    animation: w-save-flash 1.5s cubic-bezier(0.16, 1, 0.3, 1) forwards; margin-left: 4px;
  }
  @keyframes w-save-flash { 0% { opacity: 0; } 15% { opacity: 1; } 70% { opacity: 1; } 100% { opacity: 0; } }

  /* Version compact (inside action bar) */
  .w-version-compact {
    display: inline-flex; align-items: center; gap: 4px;
    font-size: 9px; font-weight: 600; color: var(--color-muted); opacity: 0.6;
    text-transform: uppercase; letter-spacing: 0.3px;
  }
  .w-version-badge { background: var(--color-accent); color: white; padding: 2px 6px; border-radius: 4px; font-size: 8px; font-weight: 700; }
  .w-version-arrow {
    width: 22px; height: 22px; border: 1px solid var(--color-border); border-radius: 4px;
    background: transparent; color: var(--color-muted); cursor: pointer;
    display: flex; align-items: center; justify-content: center;
    font-size: 10px; transition: all 150ms ease; font-family: inherit;
  }
  .w-version-arrow:hover:not(:disabled) { border-color: var(--color-accent); color: var(--color-accent); }
  .w-version-arrow:disabled { opacity: 0.3; cursor: not-allowed; }

  /* === INPUT BAR === */
  .w-input-bar {
    flex-shrink: 0; border-top: 1px solid var(--color-border);
    padding: 12px 24px; width: 100%; box-sizing: border-box;
    display: flex; align-items: flex-end; justify-content: center;
  }
  .w-input-inner {
    width: 100%; max-width: 780px; display: flex; flex-direction: column; gap: 0;
  }
</style>
