<script lang="ts">
  import { onMount } from "svelte";
  import { open } from "@tauri-apps/plugin-shell";
  import {
    createGuestCheckout,
    pollGuestCheckout,
    storeExtractionReceipt,
    storePendingCheckout,
    getPendingCheckout,
    clearPendingCheckout,
    restoreGuestPurchase,
    hasExtractionReceipt,
    hasUsedExtraction,
    markExtractionUsed,
    createCheckout,
    scrapeTwitter,
    scrapeBlog,
    scrapeReddit,
    exportProfile,
    type FormatGroup,
  } from "../api/tauri";
  import {
    startQueue,
    getIsExtracting,
    getProgress,
    getError as getExtractionError,
    isDone,
  } from "$lib/stores/extraction.svelte";
  import {
    canExtract,
    isPro,
    refresh as refreshSubscription,
  } from "$lib/stores/subscription.svelte";
  import { friendlyError } from "$lib/utils/errors";
  import LoadingSpinner from "./LoadingSpinner.svelte";
  import loomIdleUrl from "../../assets/loom-idle.png";

  // --- View state machine ---
  type ViewState =
    | "loading"
    | "paywall"
    | "email"
    | "processing"
    | "polling"
    | "restore"
    | "inputMethod"
    | "pasteStep"
    | "review"
    | "extracting"
    | "done";

  let viewState = $state<ViewState>("loading");
  let error = $state("");

  // Guest checkout state
  let guestEmail = $state("");
  let guestSessionId = $state("");
  let restoreEmail = $state("");
  let usedExtraction = $state(false);

  // Stepped sample input
  const FORMAT_STEPS = [
    {
      format: "twitter",
      label: "Tweets / Social",
      guidance:
        "Paste 10-20 tweets or social posts. Copy from your Twitter/X archive or timeline.",
    },
    {
      format: "email",
      label: "Emails",
      guidance:
        "Go to your Sent folder. Find 2-3 emails where you actually wrote something substantial, not quick replies.",
    },
    {
      format: "longform",
      label: "Long-form",
      guidance:
        "Blog posts, essays, articles, newsletter issues. Even one long piece helps.",
    },
    {
      format: "slack",
      label: "Slack / Chat",
      guidance:
        "In Slack, search from:me and grab your longer messages. Discord, WhatsApp, or iMessage work too. Skip one-liners.",
    },
    {
      format: "linkedin",
      label: "LinkedIn",
      guidance: "Go to your profile, click Activity, filter by Posts. Paste your longer posts here.",
    },
    {
      format: "reddit",
      label: "Reddit",
      guidance: "Fetch by username, or paste your posts and comments below.",
    },
  ];

  let currentStep = $state(0);
  let formatSamples: Record<string, string[]> = $state({});
  let bulkPasteOpen = $state(false);
  let bulkPasteText = $state("");

  // Scrape state
  const SCRAPABLE_FORMATS = ["twitter", "longform", "reddit"];
  let scrapeHandle = $state("");
  let scrapeUrl = $state("");
  let isScraping = $state(false);
  let scrapeError = $state("");
  let scrapeInfoMap: Record<string, string> = $state({});

  let canScrape = $derived(SCRAPABLE_FORMATS.includes(FORMAT_STEPS[currentStep].format));

  // --- Draft persistence ---
  const EXTRACT_DRAFT_KEY = "noren:extract_draft";

  function saveExtractDraft() {
    try {
      localStorage.setItem(EXTRACT_DRAFT_KEY, JSON.stringify({
        viewState, currentStep, formatSamples, scrapeInfoMap,
      }));
    } catch {}
  }

  function loadExtractDraft(): boolean {
    try {
      const raw = localStorage.getItem(EXTRACT_DRAFT_KEY);
      if (!raw) return false;
      const d = JSON.parse(raw);
      const resumable = ["inputMethod", "pasteStep", "review"];
      if (!resumable.includes(d.viewState)) return false;
      viewState = d.viewState;
      currentStep = d.currentStep || 0;
      scrapeInfoMap = d.scrapeInfoMap || {};
      // Migrate old string format to string[]
      const loaded = d.formatSamples || {};
      const migrated: Record<string, string[]> = {};
      for (const [k, v] of Object.entries(loaded)) {
        migrated[k] = typeof v === "string" ? (v.trim() ? [v] : []) : (v as string[]);
      }
      formatSamples = migrated;
      return true;
    } catch { return false; }
  }

  function clearExtractDraft() {
    localStorage.removeItem(EXTRACT_DRAFT_KEY);
  }

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

  // --- Lifecycle ---

  onMount(() => {
    checkAccess();
  });

  // Auto-save draft on navigation (not on every keystroke)
  $effect(() => {
    void [viewState, currentStep, scrapeInfoMap];
    saveExtractDraft();
  });

  // Watch extraction store for completion
  $effect(() => {
    if (isDone() && viewState === "extracting") {
      if (!isPro()) {
        markExtractionUsed().catch(() => {});
      }
      viewState = "done";
    }
  });

  // Watch extraction store for errors
  $effect(() => {
    const err = getExtractionError();
    if (err && viewState === "extracting") {
      error = err;
      viewState = "review";
    }
  });

  // Auto-poll payment status every 5s when waiting, with 10-min timeout
  $effect(() => {
    if (viewState !== "polling") return;

    const interval = setInterval(async () => {
      if (!guestSessionId) return;
      try {
        const status = await pollGuestCheckout(guestSessionId);
        if (status.paid) {
          await storeExtractionReceipt(guestSessionId);
          await clearPendingCheckout();
          await refreshSubscription();
          viewState = "inputMethod";
        }
        // Don't set error on "not yet" — avoid flicker
      } catch {
        // Silently retry on next interval
      }
    }, 5000);

    const timeout = setTimeout(() => {
      clearInterval(interval);
      error = "Payment verification timed out. Click 'Check status' to try again, or start over.";
    }, 10 * 60 * 1000);

    return () => {
      clearInterval(interval);
      clearTimeout(timeout);
    };
  });

  async function checkAccess() {
    viewState = "loading";
    error = "";

    // If extraction is already running (navigated away and back), show progress
    if (getIsExtracting()) {
      viewState = "extracting";
      return;
    }

    // Refresh subscription store (checks both server + local receipt)
    await refreshSubscription();

    // Check if user can extract (Pro or has valid local receipt)
    if (canExtract() || isPro()) {
      if (!loadExtractDraft()) {
        viewState = "inputMethod";
      }
      return;
    }

    // Check for pending checkout (payment recovery layer 2)
    try {
      const pending = await getPendingCheckout();
      if (pending) {
        guestSessionId = pending.session_id;
        guestEmail = pending.email;
        // Try polling to see if payment completed
        try {
          const status = await pollGuestCheckout(pending.session_id);
          if (status.paid) {
            await storeExtractionReceipt(pending.session_id);
            await clearPendingCheckout();
            await refreshSubscription();
            viewState = "inputMethod";
            return;
          }
        } catch {
          // Server unreachable, show polling UI so user can retry
        }
        viewState = "polling";
        return;
      }
    } catch {
      // No pending checkout
    }

    // Check if they previously used their extraction
    try {
      usedExtraction = await hasUsedExtraction();
    } catch {
      usedExtraction = false;
    }

    viewState = "paywall";
  }

  // --- Guest checkout flow ---

  async function handleGuestCheckout() {
    if (!guestEmail.trim() || !guestEmail.includes("@")) {
      error = "Enter a valid email address.";
      return;
    }

    error = "";
    viewState = "processing";

    try {
      const result = await createGuestCheckout(guestEmail.trim(), "extraction");

      if (result.checkout_url === "dev://granted") {
        // Dev mock: skip Stripe
        await storeExtractionReceipt(result.session_id);
        await refreshSubscription();
        viewState = "inputMethod";
        return;
      }

      guestSessionId = result.session_id;

      // Persist pending checkout before opening Stripe (recovery layer 1)
      await storePendingCheckout(result.session_id, guestEmail.trim());

      // Open Stripe in browser
      await open(result.checkout_url);
      viewState = "polling";
    } catch (e) {
      error = friendlyError(e);
      viewState = "email";
    }
  }

  async function handlePollStatus() {
    if (!guestSessionId) return;
    error = "";

    try {
      const status = await pollGuestCheckout(guestSessionId);
      if (status.paid) {
        await storeExtractionReceipt(guestSessionId);
        await clearPendingCheckout();
        await refreshSubscription();
        viewState = "inputMethod";
      } else {
        error = "Payment not yet confirmed. Complete checkout in your browser.";
      }
    } catch (e) {
      error = friendlyError(e);
    }
  }

  async function handleRestore() {
    if (!restoreEmail.trim() || !restoreEmail.includes("@")) {
      error = "Enter the email you used at checkout.";
      return;
    }

    error = "";

    try {
      const result = await restoreGuestPurchase(restoreEmail.trim());
      if (result.found && result.session_id) {
        await storeExtractionReceipt(result.session_id);
        await refreshSubscription();
        viewState = "inputMethod";
      } else {
        error = "No purchase found for that email.";
      }
    } catch (e) {
      error = friendlyError(e);
    }
  }

  // Pro upgrade (requires auth, existing flow)
  async function handleProUpgrade() {
    error = "";
    try {
      const result = await createCheckout("pro");
      if (result.checkout_url === "dev://granted") {
        await refreshSubscription();
        viewState = "inputMethod";
      } else {
        await open(result.checkout_url);
      }
    } catch (e) {
      error = friendlyError(e);
    }
  }

  // --- Sample input ---

  function currentSamples(): string[] {
    return formatSamples[FORMAT_STEPS[currentStep].format] || [];
  }

  function enterStep(stepIndex: number) {
    currentStep = stepIndex;
    bulkPasteOpen = false;
    bulkPasteText = "";
    resetScrapeState();
    // Ensure at least one card exists
    const fmt = FORMAT_STEPS[stepIndex].format;
    if (!formatSamples[fmt] || formatSamples[fmt].length === 0) {
      formatSamples[fmt] = [""];
      formatSamples = { ...formatSamples };
    }
  }

  function startPasteFlow() {
    if (Object.keys(formatSamples).length === 0) {
      scrapeInfoMap = {};
    }
    enterStep(0);
    viewState = "pasteStep";
  }

  function nextStep() {
    saveExtractDraft();
    if (currentStep < FORMAT_STEPS.length - 1) {
      enterStep(currentStep + 1);
    } else {
      viewState = "review";
    }
  }

  function skipStep() {
    if (currentStep < FORMAT_STEPS.length - 1) {
      enterStep(currentStep + 1);
    } else {
      viewState = "review";
    }
  }

  function prevStep() {
    saveExtractDraft();
    if (currentStep > 0) {
      enterStep(currentStep - 1);
    } else {
      viewState = "inputMethod";
    }
  }

  function resetPasteFlow() {
    formatSamples = {};
    scrapeInfoMap = {};
    currentStep = 0;
    bulkPasteOpen = false;
    bulkPasteText = "";
    viewState = "inputMethod";
  }

  function goBackToStep(index: number) {
    enterStep(index);
    viewState = "pasteStep";
  }

  // Card operations
  function addSample() {
    const fmt = FORMAT_STEPS[currentStep].format;
    if (!formatSamples[fmt]) formatSamples[fmt] = [];
    formatSamples[fmt] = [...formatSamples[fmt], ""];
  }

  function removeSample(index: number) {
    const fmt = FORMAT_STEPS[currentStep].format;
    formatSamples[fmt] = formatSamples[fmt].filter((_, i) => i !== index);
    if (formatSamples[fmt].length === 0) formatSamples[fmt] = [""];
    formatSamples = { ...formatSamples };
  }

  function updateSample(index: number, value: string) {
    const fmt = FORMAT_STEPS[currentStep].format;
    formatSamples[fmt][index] = value;
  }

  function handleBulkPaste() {
    if (!bulkPasteText.trim()) return;
    const items = bulkPasteText.split(/\n\s*\n\s*\n/).map(s => s.trim()).filter(s => s.length > 0);
    const fmt = FORMAT_STEPS[currentStep].format;
    const existing = (formatSamples[fmt] || []).filter(s => s.trim());
    formatSamples[fmt] = [...existing, ...items];
    formatSamples = { ...formatSamples };
    bulkPasteOpen = false;
    bulkPasteText = "";
  }

  function sampleCount(format: string): number {
    return (formatSamples[format] || []).filter(s => s.trim()).length;
  }

  function totalSamples(): number {
    return Object.keys(formatSamples).reduce((sum, fmt) => sum + sampleCount(fmt), 0);
  }

  function formatGroups(): FormatGroup[] {
    return Object.entries(formatSamples)
      .filter(([fmt]) => sampleCount(fmt) > 0)
      .map(([format, arr]) => ({
        format,
        samples: arr.filter(s => s.trim()).join("\n\n===\n\n"),
      }));
  }

  // --- Scraping ---

  function resetScrapeState() {
    scrapeHandle = "";
    scrapeUrl = "";
    isScraping = false;
    scrapeError = "";
  }

  async function handleScrapeTwitter() {
    const handle = scrapeHandle.trim();
    if (!handle) {
      scrapeError = "Enter a username or profile link.";
      return;
    }

    const targetStep = currentStep;
    const targetFormat = FORMAT_STEPS[targetStep].format;
    scrapeError = "";
    isScraping = true;

    try {
      const result = await scrapeTwitter(handle);
      const items = result.format_group.samples.split(/\n\n===\n\n/).map(s => s.trim()).filter(s => s.length > 0);
      formatSamples[targetFormat] = items;
      formatSamples = { ...formatSamples };
      scrapeInfoMap["twitter"] = `Fetched ${result.meta.total_kept} tweets`;
    } catch (e) {
      if (currentStep === targetStep) scrapeError = friendlyError(e);
    } finally {
      isScraping = false;
    }
  }

  async function handleScrapeBlog() {
    const url = scrapeUrl.trim();
    if (!url || !url.startsWith("http")) {
      scrapeError = "Enter a valid URL starting with http:// or https://";
      return;
    }

    const targetStep = currentStep;
    const targetFormat = FORMAT_STEPS[targetStep].format;
    scrapeError = "";
    isScraping = true;

    try {
      const result = await scrapeBlog(url);
      const items = result.format_group.samples.split(/\n\n===\n\n/).map(s => s.trim()).filter(s => s.length > 0);
      formatSamples[targetFormat] = items;
      formatSamples = { ...formatSamples };
      const label = result.meta.source_type === "rss" ? "posts" : "article";
      scrapeInfoMap["longform"] = `Fetched ${result.meta.total_kept} ${label}`;
    } catch (e) {
      if (currentStep === targetStep) scrapeError = friendlyError(e);
    } finally {
      isScraping = false;
    }
  }

  async function handleScrapeReddit() {
    const handle = scrapeHandle.trim();
    if (!handle) {
      scrapeError = "Enter a Reddit username or profile link.";
      return;
    }

    const targetStep = currentStep;
    const targetFormat = FORMAT_STEPS[targetStep].format;
    scrapeError = "";
    isScraping = true;

    try {
      const result = await scrapeReddit(handle);
      const items = result.format_group.samples.split(/\n\n===\n\n/).map(s => s.trim()).filter(s => s.length > 0);
      formatSamples[targetFormat] = items;
      formatSamples = { ...formatSamples };
      scrapeInfoMap["reddit"] = `Fetched ${result.meta.total_kept} posts`;
    } catch (e) {
      if (currentStep === targetStep) scrapeError = friendlyError(e);
    } finally {
      isScraping = false;
    }
  }

  // --- Extraction ---

  async function handleExtract() {
    const groups = formatGroups();
    if (groups.length === 0 || totalSamples() < 5) return;

    error = "";
    viewState = "extracting";
    clearExtractDraft();

    startQueue(groups);
  }
</script>

<div class="flex flex-col gap-3 h-full p-4 overflow-y-auto animate-fade-in-up">

  {#if viewState === "loading"}
    <!-- Loading -->
    <div class="flex-1 flex items-center justify-center">
      <LoadingSpinner />
    </div>

  {:else if viewState === "done"}
    <!-- Success -->
    {@const p = getProgress()}
    <div class="flex-1 flex flex-col items-center justify-center gap-4">
      <div class="w-12 h-12 rounded-full bg-signal/10 flex items-center justify-center">
        <svg class="w-6 h-6 text-signal" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
          <path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7" />
        </svg>
      </div>
      <div class="text-center">
        <p class="text-display text-foreground">Voice profile created</p>
        <p class="text-xs text-muted mt-1">
          {p?.status === "stored_server"
            ? "Your profile is stored on Noren servers and ready to use."
            : "Your profile has been saved and is ready to use."}
        </p>
      </div>
      {#if !isPro()}
        <div class="p-3 card-hero text-center max-w-[280px]">
          <p class="text-[10px] text-muted leading-relaxed">
            Save a backup of your voice profile. If you lose it, you'll need to extract again.
          </p>
          <button
            onclick={async () => { try { await exportProfile(); } catch {} }}
            class="mt-2 text-[10px] text-secondary hover:text-primary transition-colors cursor-pointer"
          >
            Export profile
          </button>
        </div>
      {/if}
    </div>

  {:else if viewState === "extracting"}
    <!-- Extraction progress -->
    {@const p = getProgress()}
    <div class="flex-1 flex flex-col items-center justify-center gap-4">
      <LoadingSpinner />
      <div class="text-center">
        <p class="text-sm font-medium text-foreground">
          {p ? (statusLabels[p.status] || p.status) : "Starting extraction..."}
        </p>
        <p class="text-xs text-muted mt-1">
          {p ? `${p.progress}% complete` : "Preparing..."}
        </p>
      </div>
      {#if p}
        <div class="w-48 h-1.5 bg-tint rounded-full overflow-hidden">
          <div
            class="h-full bg-primary rounded-full transition-all duration-500 ease-out"
            style="width: {p.progress}%"
          ></div>
        </div>
      {/if}
      <p class="text-[10px] text-muted">Extraction runs in the background. You can continue using the app.</p>
      {#if error}
        <p class="text-[10px] text-error">{error}</p>
      {/if}
    </div>

  {:else if viewState === "paywall"}
    <!-- Paywall -->
    <div class="flex-1 flex flex-col items-center justify-center">
      <div class="p-5 card-hero text-center max-w-[280px]">
        <p class="text-xs font-medium text-secondary font-heading italic">Voice Extraction</p>
        <p class="text-[10px] text-muted mt-1.5 leading-relaxed">
          AI-powered 4-pass analysis of your writing patterns, vocabulary, and rhetorical style.
        </p>

        <div class="flex flex-col gap-1.5 mt-4">
          <button
            onclick={() => { error = ""; viewState = "email"; }}
            class="w-full py-2 text-[11px] font-medium bg-accent text-white hover:bg-accent-hover transition-colors cursor-pointer rounded"
          >
            {usedExtraction ? "Extract again $19" : "$19 one-time"}
          </button>
          <button
            onclick={handleProUpgrade}
            class="w-full py-1.5 text-[10px] text-secondary hover:text-primary transition-colors cursor-pointer"
          >
            Or get Pro ($7/mo)
          </button>
        </div>

        {#if error}
          <p class="text-[10px] text-error mt-2">{error}</p>
        {/if}

        <button
          onclick={() => { error = ""; restoreEmail = ""; viewState = "restore"; }}
          class="mt-3 text-[9px] text-muted hover:text-secondary transition-colors cursor-pointer"
        >
          Already paid? Restore purchase
        </button>
      </div>
    </div>

  {:else if viewState === "email"}
    <!-- Email input for guest checkout -->
    <div class="flex-1 flex flex-col items-center justify-center">
      <div class="p-5 card-hero text-center max-w-[280px]">
        <p class="text-xs font-medium text-secondary font-heading italic">Almost there</p>
        <p class="text-[10px] text-muted mt-1.5 leading-relaxed">
          Enter your email for the receipt. No account will be created.
        </p>

        <input
          type="email"
          bind:value={guestEmail}
          placeholder="you@example.com"
          class="w-full mt-3 px-3 py-2 text-xs border border-border bg-surface text-foreground rounded-md placeholder-muted focus:outline-none focus:border-secondary"
          onkeydown={(e) => { if (e.key === "Enter") handleGuestCheckout(); }}
        />

        <button
          onclick={handleGuestCheckout}
          disabled={!guestEmail.trim()}
          class="w-full mt-2 py-2 text-[11px] font-medium rounded transition-colors cursor-pointer
            {!guestEmail.trim()
              ? 'bg-surface text-muted border border-border cursor-not-allowed opacity-50'
              : 'bg-accent text-white hover:bg-accent-hover'}"
        >
          Continue to checkout
        </button>

        {#if error}
          <p class="text-[10px] text-error mt-2">{error}</p>
        {/if}

        <button
          onclick={() => { error = ""; viewState = "paywall"; }}
          class="mt-2 text-[10px] text-muted hover:text-secondary transition-colors cursor-pointer"
        >
          Back
        </button>
      </div>
    </div>

  {:else if viewState === "processing"}
    <!-- Creating checkout session -->
    <div class="flex-1 flex flex-col items-center justify-center gap-3">
      <LoadingSpinner />
      <p class="text-xs text-muted">Opening checkout...</p>
    </div>

  {:else if viewState === "polling"}
    <!-- Waiting for payment -->
    <div class="flex-1 flex flex-col items-center justify-center gap-3">
      <LoadingSpinner />
      <div class="text-center">
        <p class="text-sm font-medium text-foreground">Complete payment in your browser</p>
        <p class="text-xs text-muted mt-1">Waiting for confirmation...</p>
      </div>
      <button
        onclick={handlePollStatus}
        class="px-4 py-1.5 text-[10px] font-medium bg-surface border border-border text-foreground rounded-md hover:border-secondary transition-colors cursor-pointer"
      >
        Check status
      </button>
      {#if error}
        <p class="text-[10px] text-error text-center max-w-[260px]">{error}</p>
      {/if}
      <button
        onclick={() => { error = ""; guestEmail = ""; guestSessionId = ""; clearPendingCheckout(); viewState = "paywall"; }}
        class="text-[10px] text-muted hover:text-secondary transition-colors cursor-pointer"
      >
        Start over
      </button>
    </div>

  {:else if viewState === "restore"}
    <!-- Restore purchase by email -->
    <div class="flex-1 flex flex-col items-center justify-center">
      <div class="p-5 card-hero text-center max-w-[280px]">
        <p class="text-xs font-medium text-secondary font-heading italic">Restore purchase</p>
        <p class="text-[10px] text-muted mt-1.5 leading-relaxed">
          Enter the email you used at checkout.
        </p>

        <input
          type="email"
          bind:value={restoreEmail}
          placeholder="you@example.com"
          class="w-full mt-3 px-3 py-2 text-xs border border-border bg-surface text-foreground rounded-md placeholder-muted focus:outline-none focus:border-secondary"
          onkeydown={(e) => { if (e.key === "Enter") handleRestore(); }}
        />

        <button
          onclick={handleRestore}
          disabled={!restoreEmail.trim()}
          class="w-full mt-2 py-2 text-[11px] font-medium rounded transition-colors cursor-pointer
            {!restoreEmail.trim()
              ? 'bg-surface text-muted border border-border cursor-not-allowed opacity-50'
              : 'bg-accent text-white hover:bg-accent-hover'}"
        >
          Restore
        </button>

        {#if error}
          <p class="text-[10px] text-error mt-2">{error}</p>
        {/if}

        <button
          onclick={() => { error = ""; viewState = "paywall"; }}
          class="mt-2 text-[10px] text-muted hover:text-secondary transition-colors cursor-pointer"
        >
          Back
        </button>
      </div>
    </div>

  {:else if viewState === "inputMethod"}
    <!-- Provide writing samples -->
    <div class="flex-1 flex flex-col items-center justify-center -m-4 overflow-hidden">
      <div class="relative flex flex-col items-center gap-8 animate-fade-in-up" style="animation-duration: 0.6s">
        <img src={loomIdleUrl} alt="" class="w-[120px] opacity-30 dark:opacity-50" />

        <div class="text-center max-w-[260px]">
          <h2 class="font-heading text-[21px] italic font-normal text-foreground leading-snug tracking-[-0.3px]">
            Provide your writing
          </h2>
          <p class="text-[11px] text-muted leading-[1.7] mt-3">
            We'll walk you through six format categories. Paste what you have, skip what you don't.
          </p>
          <div class="flex gap-1.5 flex-wrap justify-center mt-2">
            <span class="text-[10px] text-muted bg-tint px-2 py-0.5 rounded">tweets</span>
            <span class="text-[10px] text-muted bg-tint px-2 py-0.5 rounded">emails</span>
            <span class="text-[10px] text-muted bg-tint px-2 py-0.5 rounded">long-form</span>
            <span class="text-[10px] text-muted bg-tint px-2 py-0.5 rounded">chat</span>
            <span class="text-[10px] text-muted bg-tint px-2 py-0.5 rounded">linkedin</span>
            <span class="text-[10px] text-muted bg-tint px-2 py-0.5 rounded">reddit</span>
          </div>
        </div>

        <button
          onclick={startPasteFlow}
          class="px-7 py-2.5 text-[13px] font-semibold bg-accent text-white hover:bg-accent-hover transition-all duration-200 cursor-pointer rounded-lg hover:-translate-y-px"
          style="box-shadow: 0 2px 8px var(--color-accent-glow)"
        >
          Get started
        </button>

        {#if error}
          <p class="text-[10px] text-error">{error}</p>
        {/if}
      </div>
    </div>

  {:else if viewState === "pasteStep"}
    <!-- Format-specific paste step -->
    {@const step = FORMAT_STEPS[currentStep]}
    <div class="flex-1 flex flex-col px-4 pt-4 pb-3" style="animation: view-enter 0.35s ease-out both">
      <!-- Progress bar -->
      <div class="flex items-center gap-2.5 mb-4">
        <div class="flex gap-[3px] flex-1">
          {#each FORMAT_STEPS as _, i}
            <div
              class="flex-1 h-[3px] rounded-full transition-all duration-300
                {i < currentStep || (i !== currentStep && sampleCount(FORMAT_STEPS[i].format) > 0)
                  ? 'bg-accent'
                  : i === currentStep
                    ? 'bg-accent'
                    : 'bg-border'}"
              style={i === currentStep ? 'box-shadow: 0 0 6px var(--color-accent-glow)' : ''}
            ></div>
          {/each}
        </div>
        <span class="text-[10px] text-muted shrink-0 tabular-nums">{currentStep + 1} / {FORMAT_STEPS.length}</span>
      </div>

      <!-- Step header -->
      <div class="mb-3.5">
        <h3 class="text-[15px] font-heading italic font-normal text-foreground flex items-center gap-2">
          <span class="w-1.5 h-1.5 rounded-full bg-accent shrink-0"></span>
          {step.label}
        </h3>
        <p class="text-[11px] text-muted mt-1 leading-relaxed pl-3.5">
          {canScrape
            ? step.format === "twitter"
              ? "Fetch by username, or paste tweets below."
              : step.format === "reddit"
                ? "Fetch by username, or paste posts below."
                : "Import from a blog URL, or paste articles below."
            : step.guidance}
        </p>
      </div>

      <!-- Scrape section (scrapable formats only) -->
      {#if canScrape}
        <div class="flex items-center gap-2 mb-3">
          {#if step.format === "twitter" || step.format === "reddit"}
            <input
              type="text"
              bind:value={scrapeHandle}
              placeholder={step.format === "twitter" ? "@username or profile link" : "u/username or profile link"}
              disabled={isScraping}
              class="input-field flex-1 !py-[7px] !text-xs"
              onkeydown={(e) => { if (e.key === "Enter") { step.format === "twitter" ? handleScrapeTwitter() : handleScrapeReddit(); } }}
            />
          {:else}
            <input
              type="url"
              bind:value={scrapeUrl}
              placeholder="Blog URL or RSS feed"
              disabled={isScraping}
              class="input-field flex-1 !py-[7px] !text-xs"
              onkeydown={(e) => { if (e.key === "Enter") handleScrapeBlog(); }}
            />
          {/if}
          <button
            onclick={step.format === "twitter" ? handleScrapeTwitter : step.format === "reddit" ? handleScrapeReddit : handleScrapeBlog}
            disabled={isScraping || (step.format === "twitter" || step.format === "reddit" ? !scrapeHandle.trim() : !scrapeUrl.trim())}
            class="btn-outline shrink-0 !text-[11px]"
          >
            {isScraping
              ? "Fetching..."
              : step.format === "twitter" ? "Fetch tweets" : step.format === "reddit" ? "Fetch posts" : "Fetch posts"}
          </button>
        </div>

        <!-- Scrape feedback -->
        {#if isScraping}
          <div class="flex items-center gap-1.5 mb-3">
            <LoadingSpinner />
            <span class="text-[11px] text-muted">
              {step.format === "twitter"
                ? `Fetching tweets from @${scrapeHandle.replace(/^@/, "")}...`
                : step.format === "reddit"
                  ? `Fetching posts from u/${scrapeHandle.replace(/^u\//, "")}...`
                  : "Fetching posts..."}
            </span>
          </div>
        {:else if scrapeError}
          <div class="flex gap-2 items-start mb-3 px-3 py-2.5 rounded-lg" style="background: rgba(184,134,11,0.06); border-left: 2px solid var(--color-warning)">
            <svg class="w-3.5 h-3.5 text-warning shrink-0 mt-px" viewBox="0 0 16 16" fill="none">
              <circle cx="8" cy="8" r="7" stroke="currentColor" stroke-width="1.2"/>
              <path d="M8 5v3.5" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>
              <circle cx="8" cy="11" r="0.6" fill="currentColor"/>
            </svg>
            <div class="text-[11px] leading-relaxed text-muted">
              <span class="text-foreground font-medium">Couldn't pull articles from that site.</span>
              Some sites block automated access.
              <span class="block mt-0.5 text-secondary font-medium">Paste your posts below instead.</span>
            </div>
          </div>
        {:else if scrapeInfoMap[step.format]}
          <div class="flex items-center gap-1.5 mb-3">
            <svg class="w-3.5 h-3.5 text-signal shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2.5">
              <path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7" />
            </svg>
            <span class="text-[11px] text-signal">{scrapeInfoMap[step.format]}</span>
          </div>
        {/if}
      {/if}

      <!-- Sample cards or bulk paste -->
      {#if bulkPasteOpen}
        <div class="flex-1 flex flex-col gap-2 min-h-0">
          <div class="flex-1 flex flex-col bg-background border border-border rounded-[10px] p-0.5 min-h-0 transition-colors focus-within:border-secondary" style="box-shadow: var(--shadow-inset)">
            <textarea
              bind:value={bulkPasteText}
              class="flex-1 px-3 py-2.5 text-xs leading-relaxed bg-transparent text-foreground resize-none placeholder-muted focus:outline-none min-h-[180px]"
              placeholder="Paste all samples here. Hit Enter twice between samples."
            ></textarea>
          </div>
          <div class="flex items-center gap-2">
            <button onclick={() => { bulkPasteOpen = false; bulkPasteText = ""; }} class="btn-ghost">Cancel</button>
            <div class="flex-1"></div>
            <button onclick={handleBulkPaste} disabled={!bulkPasteText.trim()} class="btn-primary">Split into cards</button>
          </div>
        </div>
      {:else}
        <div class="flex-1 flex flex-col gap-2 overflow-y-auto min-h-0">
          {#each currentSamples() as sample, i}
            <div class="relative bg-background border border-border rounded-[10px] p-0.5 shrink-0 transition-colors focus-within:border-secondary group" style="box-shadow: var(--shadow-inset)">
              <div class="flex items-center justify-between px-2.5 pt-1.5">
                <span class="text-[9px] font-semibold text-muted uppercase tracking-[0.5px]">Sample {i + 1}</span>
                {#if currentSamples().length > 1}
                  <button
                    onclick={() => removeSample(i)}
                    class="w-[18px] h-[18px] flex items-center justify-center bg-transparent border-none text-muted rounded cursor-pointer opacity-0 group-hover:opacity-60 hover:!opacity-100 transition-opacity"
                  >
                    <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M2 2l6 6M8 2l-6 6"/></svg>
                  </button>
                {/if}
              </div>
              <textarea
                value={sample}
                oninput={(e) => updateSample(i, e.currentTarget.value)}
                onblur={() => saveExtractDraft()}
                class="w-full px-2.5 py-1.5 text-xs leading-relaxed bg-transparent text-foreground resize-none placeholder-muted focus:outline-none"
                style="min-height: 64px; field-sizing: content;"
                placeholder={i === 0 ? `Paste a ${step.label.toLowerCase()} sample...` : "Another sample..."}
              ></textarea>
            </div>
          {/each}

          <!-- Add buttons -->
          <button
            onclick={addSample}
            class="w-full py-2.5 flex items-center justify-center gap-1.5 text-[11px] text-muted bg-transparent rounded-[10px] cursor-pointer transition-all duration-150 shrink-0 hover:text-accent hover:bg-accent-wash"
            style="border: 1px dashed var(--color-border)"
          >
            <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M6 2v8M2 6h8"/></svg>
            Add another sample
          </button>
          <button
            onclick={() => { bulkPasteOpen = true; }}
            class="text-[10px] text-muted hover:text-secondary transition-colors cursor-pointer text-center shrink-0"
          >
            Bulk paste
          </button>
        </div>

        {#if sampleCount(step.format) > 0}
          <div class="flex items-center justify-end gap-1 pt-1.5 pb-0.5 shrink-0">
            <span class="text-[11px] text-accent font-medium">{sampleCount(step.format)}</span>
            <span class="text-[11px] text-muted">sample{sampleCount(step.format) !== 1 ? 's' : ''}</span>
          </div>
        {/if}
      {/if}

      <!-- Navigation -->
      <div class="flex items-center gap-2 pt-2 mt-2 border-t border-border">
        <button
          onclick={prevStep}
          class="btn-ghost"
        >
          Back
        </button>
        <div class="flex-1"></div>
        <button
          onclick={skipStep}
          class="btn-ghost"
        >
          Skip
        </button>
        <button
          onclick={nextStep}
          class="btn-primary"
        >
          {currentStep === FORMAT_STEPS.length - 1 ? "Review" : "Next"}
        </button>
      </div>
    </div>

  {:else if viewState === "review"}
    <!-- Review collected samples -->
    <div class="flex-1 flex flex-col px-4 pt-5 pb-4 max-w-md mx-auto w-full animate-fade-in-up" style="animation-duration: 0.4s">
      <div class="text-center mb-5">
        <h3 class="text-heading text-foreground">Review samples</h3>
        <div class="flex items-center justify-center gap-1.5 mt-1.5">
          <span class="inline-flex items-center gap-1 text-[11px] font-semibold text-accent px-2 py-0.5 rounded" style="background: var(--color-accent-wash)">
            {totalSamples()} samples
          </span>
          <span class="text-[11px] {totalSamples() >= 5 ? 'text-muted' : 'text-warning'}">
            across {formatGroups().length} format{formatGroups().length !== 1 ? "s" : ""}
          </span>
        </div>
      </div>

      <!-- Format summary cards -->
      <div class="flex flex-col gap-1.5 flex-1 overflow-y-auto">
        {#each FORMAT_STEPS as step, i}
          {@const count = sampleCount(step.format)}
          <div
            class="flex items-center justify-between px-3.5 py-2.5 bg-surface border border-border rounded-[10px] transition-all duration-150 hover:shadow-sm"
            style={count > 0 ? 'border-left: 3px solid var(--color-accent)' : ''}
          >
            <div class="flex items-center gap-2">
              <span class="text-xs font-medium text-foreground">{step.label}</span>
              {#if scrapeInfoMap[step.format]}
                <span class="text-[9px] text-muted">{scrapeInfoMap[step.format]}</span>
              {/if}
            </div>
            <div class="flex items-center gap-2">
              {#if count > 0}
                <span class="text-[11px] text-accent font-semibold">{count}</span>
                <button
                  onclick={() => goBackToStep(i)}
                  class="btn-ghost !text-[11px] !py-1 !px-2"
                >
                  Edit
                </button>
              {:else}
                <span class="text-[10px] text-muted/50 italic">skipped</span>
                <button
                  onclick={() => goBackToStep(i)}
                  class="btn-ghost !text-[11px] !py-1 !px-2"
                >
                  Add
                </button>
              {/if}
            </div>
          </div>
        {/each}
      </div>

      {#if error}
        <p class="text-[11px] text-error text-center mt-3">{error}</p>
      {/if}

      <!-- Accent thread divider -->
      <div class="divider-thread my-4"></div>

      <!-- Extract CTA -->
      <div class="flex flex-col items-center gap-2.5">
        <button
          onclick={handleExtract}
          disabled={totalSamples() < 5}
          class="w-full py-3 text-[13px] font-semibold text-white bg-accent rounded-lg cursor-pointer transition-all duration-150 disabled:opacity-50 disabled:cursor-not-allowed hover:bg-accent-hover hover:-translate-y-px relative overflow-hidden"
          style="box-shadow: 0 2px 8px var(--color-accent-glow)"
        >
          Extract Voice Profile
        </button>

        {#if totalSamples() < 5}
          <p class="text-[10px] text-muted text-center leading-relaxed">
            Need at least 5 samples. Go back and add more to any format.
          </p>
        {/if}

        <button
          onclick={resetPasteFlow}
          class="btn-ghost text-[11px]"
        >
          Start over
        </button>
      </div>
    </div>
  {/if}
</div>
