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
        "Paste 2-3 full email threads. The kind you write most, not one-liners.",
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
        "Paste longer Slack messages or chat threads. Skip quick replies.",
    },
    {
      format: "linkedin",
      label: "LinkedIn",
      guidance: "Posts, comments, or articles from LinkedIn.",
    },
  ];

  let currentStep = $state(0);
  let formatSamples: Record<string, string> = $state({});
  let currentInput = $state("");

  // Scrape state
  const SCRAPABLE_FORMATS = ["twitter", "longform"];
  let scrapeHandle = $state("");
  let scrapeUrl = $state("");
  let isScraping = $state(false);
  let scrapeError = $state("");
  let scrapeInfoMap: Record<string, string> = $state({});

  let canScrape = $derived(SCRAPABLE_FORMATS.includes(FORMAT_STEPS[currentStep].format));

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
      viewState = "inputMethod";
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

  function enterStep(stepIndex: number) {
    currentStep = stepIndex;
    currentInput = formatSamples[FORMAT_STEPS[stepIndex].format] || "";
    resetScrapeState();
  }

  function startPasteFlow() {
    formatSamples = {};
    scrapeInfoMap = {};
    enterStep(0);
    viewState = "pasteStep";
  }

  function nextStep() {
    if (currentInput.trim()) {
      formatSamples[FORMAT_STEPS[currentStep].format] = currentInput.trim();
    }

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
    if (currentInput.trim()) {
      formatSamples[FORMAT_STEPS[currentStep].format] = currentInput.trim();
    }

    if (currentStep > 0) {
      enterStep(currentStep - 1);
    } else {
      viewState = "inputMethod";
    }
  }

  function goBackToStep(index: number) {
    enterStep(index);
    viewState = "pasteStep";
  }

  function countSamples(text: string): number {
    if (!text.trim()) return 0;
    return text.split(/\n\s*\n/).filter((s) => s.trim()).length;
  }

  function totalSamples(): number {
    return Object.values(formatSamples).reduce(
      (sum, text) => sum + countSamples(text),
      0,
    );
  }

  function formatGroups(): FormatGroup[] {
    return Object.entries(formatSamples)
      .filter(([_, text]) => text.trim())
      .map(([format, samples]) => ({ format, samples }));
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
      formatSamples[targetFormat] = result.format_group.samples;
      if (currentStep === targetStep) currentInput = result.format_group.samples;
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
      formatSamples[targetFormat] = result.format_group.samples;
      if (currentStep === targetStep) currentInput = result.format_group.samples;
      const label = result.meta.source_type === "rss" ? "posts" : "article";
      scrapeInfoMap["longform"] = `Fetched ${result.meta.total_kept} ${label}`;
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
            onclick={() => { /* TODO: export profile */ }}
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
    <div class="flex-1 flex flex-col items-center justify-center gap-5">
      <div class="text-center max-w-xs">
        <p class="text-heading text-foreground">Provide your writing</p>
        <p class="text-xs text-muted mt-2 leading-relaxed">We'll walk you through 5 format categories: tweets, emails, long-form, chat, and LinkedIn. Skip any you don't have.</p>
      </div>

      <button
        onclick={startPasteFlow}
        class="btn-primary py-2.5 px-8"
      >
        Get started
      </button>

      {#if error}
        <p class="text-[10px] text-error">{error}</p>
      {/if}
    </div>

  {:else if viewState === "pasteStep"}
    <!-- Format-specific paste step -->
    {@const step = FORMAT_STEPS[currentStep]}
    <div class="flex-1 flex flex-col gap-4 px-4">
      <!-- Progress bar -->
      <div class="flex items-center gap-3">
        <div class="flex gap-1 flex-1">
          {#each FORMAT_STEPS as _, i}
            <div
              class="flex-1 h-1 rounded-full transition-colors
                {i <= currentStep || formatSamples[FORMAT_STEPS[i].format]
                  ? 'bg-accent'
                  : 'bg-border'}"
            ></div>
          {/each}
        </div>
        <span class="text-micro text-muted shrink-0">{currentStep + 1}/{FORMAT_STEPS.length}</span>
      </div>

      <!-- Step header -->
      <div>
        <h3 class="text-subhead text-foreground">{step.label}</h3>
        <p class="text-xs text-muted mt-1.5 leading-relaxed">
          {canScrape
            ? step.format === "twitter"
              ? "Fetch by username, or paste tweets below."
              : "Import from a blog URL, or paste articles below."
            : step.guidance}
        </p>
      </div>

      <!-- Scrape section (scrapable formats only) -->
      {#if canScrape}
        <div class="flex items-center gap-2">
          {#if step.format === "twitter"}
            <input
              type="text"
              bind:value={scrapeHandle}
              placeholder="@username or profile link"
              disabled={isScraping}
              class="input-field flex-1"
              onkeydown={(e) => { if (e.key === "Enter") handleScrapeTwitter(); }}
            />
          {:else}
            <input
              type="url"
              bind:value={scrapeUrl}
              placeholder="Blog URL or RSS feed"
              disabled={isScraping}
              class="input-field flex-1"
              onkeydown={(e) => { if (e.key === "Enter") handleScrapeBlog(); }}
            />
          {/if}
          <button
            onclick={step.format === "twitter" ? handleScrapeTwitter : handleScrapeBlog}
            disabled={isScraping || (step.format === "twitter" ? !scrapeHandle.trim() : !scrapeUrl.trim())}
            class="btn-outline shrink-0"
          >
            {isScraping
              ? "Fetching..."
              : step.format === "twitter" ? "Fetch tweets" : "Fetch posts"}
          </button>
        </div>

        <!-- Scrape feedback -->
        {#if isScraping}
          <div class="flex items-center gap-1.5">
            <LoadingSpinner />
            <span class="text-[11px] text-muted">
              {step.format === "twitter"
                ? `Fetching tweets from @${scrapeHandle.replace(/^@/, "")}...`
                : "Fetching posts..."}
            </span>
          </div>
        {:else if scrapeError}
          <p class="text-[11px] text-error">{scrapeError}</p>
        {:else if scrapeInfoMap[step.format]}
          <div class="flex items-center gap-1.5">
            <svg class="w-3.5 h-3.5 text-signal shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2.5">
              <path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7" />
            </svg>
            <span class="text-[11px] text-signal">{scrapeInfoMap[step.format]}</span>
          </div>
        {/if}
      {/if}

      <!-- Textarea -->
      <div class="card-inset flex-1 flex flex-col p-1">
        <textarea
          bind:value={currentInput}
          class="flex-1 p-3 text-xs leading-relaxed bg-transparent text-foreground resize-none placeholder-muted focus:outline-none min-h-[180px]"
          placeholder={canScrape
            ? `Or paste your ${step.label.toLowerCase()} here, separated by blank lines...`
            : `Paste your ${step.label.toLowerCase()} here, separated by blank lines...`}
        ></textarea>
      </div>

      {#if currentInput.trim()}
        <p class="text-[11px] text-muted text-right -mt-2">~{countSamples(currentInput)} samples</p>
      {/if}

      <!-- Navigation -->
      <div class="flex items-center gap-2">
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
    <div class="flex-1 flex flex-col gap-5 px-4 max-w-md mx-auto w-full">
      <div class="text-center">
        <h3 class="text-heading text-foreground">Review samples</h3>
        <p class="text-xs text-muted mt-1.5 {totalSamples() >= 5 ? '' : 'text-warning'}">
          {totalSamples()} sample{totalSamples() !== 1 ? "s" : ""} across {formatGroups().length} format{formatGroups().length !== 1 ? "s" : ""}
        </p>
      </div>

      <!-- Format summary cards -->
      <div class="flex flex-col gap-2">
        {#each FORMAT_STEPS as step, i}
          {@const text = formatSamples[step.format]}
          {@const count = text ? countSamples(text) : 0}
          <div class="card-flat flex items-center justify-between px-4 py-3">
            <div class="flex items-center gap-2.5">
              <span class="text-xs font-medium text-foreground">{step.label}</span>
              {#if scrapeInfoMap[step.format]}
                <span class="text-micro text-muted">{scrapeInfoMap[step.format]}</span>
              {/if}
            </div>
            {#if count > 0}
              <div class="flex items-center gap-3">
                <span class="text-xs text-accent font-medium">{count} samples</span>
                <button
                  onclick={() => goBackToStep(i)}
                  class="btn-ghost text-[11px]"
                >
                  Edit
                </button>
              </div>
            {:else}
              <div class="flex items-center gap-3">
                <span class="text-[11px] text-muted/50 italic">skipped</span>
                <button
                  onclick={() => goBackToStep(i)}
                  class="btn-ghost text-[11px]"
                >
                  Add
                </button>
              </div>
            {/if}
          </div>
        {/each}
      </div>

      {#if error}
        <p class="text-[11px] text-error text-center">{error}</p>
      {/if}

      <!-- Extract CTA -->
      <div class="flex flex-col items-center gap-3 pt-2">
        <button
          onclick={handleExtract}
          disabled={totalSamples() < 5}
          class="btn-primary w-full py-2.5 text-sm"
        >
          Extract Voice Profile
        </button>

        {#if totalSamples() < 5}
          <p class="text-[11px] text-muted text-center leading-relaxed">
            Need at least 5 samples. Go back and add more to any format.
          </p>
        {/if}

        <button
          onclick={() => { viewState = "inputMethod"; }}
          class="btn-ghost text-[11px]"
        >
          Start over
        </button>
      </div>
    </div>
  {/if}
</div>
