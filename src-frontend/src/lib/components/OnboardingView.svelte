<script lang="ts">
  import { emit } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import {
    saveProfileEdit,
    getSettings,
    norenProLogin,
    norenProSignup,
    googleOAuthInit,
    googleOAuthPoll,
    createCheckout,
  } from "$lib/api/tauri";
  import { open } from "@tauri-apps/plugin-shell";
  import { canExtract, refresh as refreshSubscription } from "$lib/stores/subscription.svelte";
  import { startQueue as startExtractionQueue } from "$lib/stores/extraction.svelte";
  import { friendlyError } from "$lib/utils/errors";
  import LoadingSpinner from "./LoadingSpinner.svelte";

  // Events
  let { onComplete }: { onComplete: () => void } = $props();

  type Step = "welcome" | "auth" | "paywall" | "paste" | "guided" | "guided-pairs" | "done" | "manual";
  let step: Step = $state("welcome");
  let pendingPath: "paste" | "guided" = $state("paste");

  // Auth state
  let authMode = $state<"login" | "signup">("login");
  let authEmail = $state("");
  let authPassword = $state("");
  let authLoading = $state(false);
  let googleLoading = $state(false);
  let isLoggedIn = $state(false);

  // Multi-format paste
  const formats = ["twitter", "email", "longform", "slack", "linkedin"];
  let formatSamples = $state<Record<string, string>>({
    twitter: "", email: "", longform: "", slack: "", linkedin: "",
  });
  let activeFormat = $state("twitter");

  // Guided path
  let currentQuestion = $state(0);
  let guidedAnswers = $state<string[]>([]);
  let currentAnswer = $state("");

  // Calibration pairs
  let currentPair = $state(0);
  let pairChoices = $state<string[]>([]);

  // Manual profile
  let manualProfile = $state("");
  let isSavingManual = $state(false);

  // Error display
  let error = $state("");

  // --- Guided session questions ---
  const questions = [
    {
      prompt: "What's a hill you'd die on that most people wouldn't care about?",
      hint: "Answer like you're texting a friend — be opinionated",
    },
    {
      prompt: "Describe something you changed your mind about recently. What convinced you?",
      hint: "Walk through your actual thought process",
    },
    {
      prompt: "What's the worst advice that sounds good on the surface?",
      hint: "Be specific — name the advice and tear it apart",
    },
    {
      prompt: "Explain something you're good at to someone who knows nothing about it.",
      hint: "Don't dumb it down — make it vivid",
    },
    {
      prompt: "What pattern do you notice that most people miss?",
      hint: "This could be about work, people, culture — anything",
    },
    {
      prompt: "Tell me about a time you were wrong and how you figured it out.",
      hint: "The interesting part is HOW you realized, not just that you did",
    },
    {
      prompt: "What do you wish people understood about what you do?",
      hint: "The frustration is the voice — let it out",
    },
  ];

  // --- Calibration pairs ---
  const pairs = [
    {
      question: "Which sounds more like you?",
      a: "The problem isn't the technology. It's that we keep solving the wrong problems with increasingly sophisticated tools.",
      b: "I think we might be overcomplicating things? Like, maybe simpler solutions would work better in a lot of cases.",
      dimension: "directness",
    },
    {
      question: "Which sounds more like you?",
      a: "This is broken and here's exactly why — three reasons, ranked by severity.",
      b: "Something feels off about this approach. I can't quite put my finger on it, but there's a disconnect.",
      dimension: "diagnostic",
    },
    {
      question: "Which sounds more like you?",
      a: "I don't know the answer yet. That's fine. I'll figure it out.",
      b: "Based on what I've seen so far, I'd lean toward option B, though I could be wrong.",
      dimension: "vulnerability",
    },
    {
      question: "Which sounds more like you?",
      a: "Ship it. Fix it later. Perfection is the enemy.",
      b: "Let's get this right. Another day won't kill us, but shipping broken code might.",
      dimension: "conviction",
    },
    {
      question: "Which sounds more like you?",
      a: "Here's the thing nobody talks about: most success is just showing up consistently while others quit.",
      b: "Success has a lot of components — timing, effort, luck, connections — and pretending otherwise is naive.",
      dimension: "framing",
    },
    {
      question: "Which sounds more like you?",
      a: "You ever notice how the loudest people in the room usually know the least? There's a pattern there.",
      b: "There's an inverse correlation between confidence and competence that I find genuinely fascinating.",
      dimension: "register",
    },
    {
      question: "Which sounds more like you?",
      a: "Look. I've tried every productivity system. The only one that works is writing stuff down and doing it.",
      b: "After years of experimentation, I've concluded that simplicity in systems tends to outperform complexity.",
      dimension: "formality",
    },
    {
      question: "Which sounds more like you?",
      a: "Delete the code. Seriously. Three thousand lines replaced by forty that actually work. That's progress.",
      b: "We managed to significantly reduce the codebase while maintaining all functionality, which improved maintainability.",
      dimension: "sentence_length",
    },
  ];

  onMount(() => {
    // Check initial auth state
    getSettings().then((settings) => {
      isLoggedIn = settings.noren_pro_logged_in;
      if (isLoggedIn) refreshSubscription();
    }).catch(() => {});
  });

  // --- Auth + entitlement gate ---

  async function checkAndProceed(path: "paste" | "guided") {
    pendingPath = path;
    error = "";

    // Check auth state
    const settings = await getSettings();
    isLoggedIn = settings.noren_pro_logged_in;

    if (!isLoggedIn) {
      step = "auth";
      return;
    }

    // Refresh subscription and check entitlement
    await refreshSubscription();
    if (canExtract()) {
      step = path;
      if (path === "guided") {
        currentQuestion = 0;
        guidedAnswers = [];
      }
    } else {
      step = "paywall";
    }
  }

  async function handleProAuth() {
    if (!authEmail.trim() || !authPassword.trim()) return;
    authLoading = true;
    error = "";
    try {
      if (authMode === "signup") {
        await norenProSignup(authEmail.trim(), authPassword.trim());
      } else {
        await norenProLogin(authEmail.trim(), authPassword.trim());
      }
      isLoggedIn = true;
      authEmail = "";
      authPassword = "";

      // Check entitlement after login
      await refreshSubscription();
      if (canExtract()) {
        step = pendingPath;
        if (pendingPath === "guided") {
          currentQuestion = 0;
          guidedAnswers = [];
        }
      } else {
        step = "paywall";
      }
    } catch (e) {
      error = friendlyError(e);
    } finally {
      authLoading = false;
    }
  }

  async function handleGoogleSignIn() {
    googleLoading = true;
    error = "";
    try {
      const { auth_url, session_id } = await googleOAuthInit();
      await open(auth_url);

      for (let i = 0; i < 150; i++) {
        await new Promise((r) => setTimeout(r, 2000));
        if (!googleLoading) return;
        try {
          const result = await googleOAuthPoll(session_id);
          if (result.complete) {
            isLoggedIn = true;
            await refreshSubscription();
            if (canExtract()) {
              step = pendingPath;
              if (pendingPath === "guided") {
                currentQuestion = 0;
                guidedAnswers = [];
              }
            } else {
              step = "paywall";
            }
            return;
          }
        } catch (e) {
          error = friendlyError(e);
          return;
        }
      }
      error = "Sign-in timed out. Please try again.";
    } catch (e) {
      error = friendlyError(e);
    } finally {
      googleLoading = false;
    }
  }

  async function handleUpgrade(tier: string) {
    error = "";
    try {
      const result = await createCheckout(tier);
      if (result.checkout_url === "dev://granted") {
        await refreshSubscription();
        if (canExtract()) {
          step = pendingPath;
          if (pendingPath === "guided") {
            currentQuestion = 0;
            guidedAnswers = [];
          }
        }
      } else {
        await open(result.checkout_url);
        // Poll for subscription change
        for (let i = 0; i < 150; i++) {
          await new Promise((r) => setTimeout(r, 2000));
          await refreshSubscription();
          if (canExtract()) {
            step = pendingPath;
            if (pendingPath === "guided") {
              currentQuestion = 0;
              guidedAnswers = [];
            }
            return;
          }
        }
      }
    } catch (e) {
      error = friendlyError(e);
    }
  }

  // --- Sample helpers ---

  function sampleCount(text: string): number {
    const t = text.trim();
    if (!t) return 0;
    return t.split(/\n\s*\n/).filter((s) => s.trim()).length;
  }

  function filledFormats(): string[] {
    return formats.filter((f) => sampleCount(formatSamples[f]) >= 5);
  }

  function totalFilledFormats(): number {
    return filledFormats().length;
  }

  function hasAnySamples(fmt: string): boolean {
    return formatSamples[fmt].trim().length > 0;
  }

  // --- Extraction ---

  function handleStartExtraction() {
    const filled = filledFormats();
    if (filled.length === 0) return;

    // Fire off background extraction and move to main app
    startExtractionQueue(
      filled.map((f) => ({ samples: formatSamples[f].trim(), format: f }))
    );
    onComplete();
  }

  function handleGuidedExtraction() {
    const guidedSamples = guidedAnswers.join("\n\n");
    const pairContext = pairs
      .map((p, i) => (pairChoices[i] === "a" ? p.a : p.b))
      .join("\n\n");
    const combinedSamples = guidedSamples + "\n\n" + pairContext;

    // Fire off background extraction and move to main app
    startExtractionQueue([{ samples: combinedSamples.trim(), format: "general" }]);
    onComplete();
  }

  function submitGuidedAnswer() {
    if (!currentAnswer.trim()) return;
    guidedAnswers = [...guidedAnswers, currentAnswer.trim()];
    currentAnswer = "";
    if (currentQuestion < questions.length - 1) {
      currentQuestion++;
    } else {
      step = "guided-pairs";
      currentPair = 0;
    }
  }

  async function choosePair(choice: "a" | "b") {
    pairChoices = [...pairChoices, choice];
    if (currentPair < pairs.length - 1) {
      currentPair++;
    } else {
      // Check auth before extraction
      const settings = await getSettings();
      isLoggedIn = settings.noren_pro_logged_in;
      if (!isLoggedIn) {
        pendingPath = "guided";
        step = "auth";
        return;
      }
      await refreshSubscription();
      if (!canExtract()) {
        pendingPath = "guided";
        step = "paywall";
        return;
      }
      handleGuidedExtraction();
    }
  }

  function goToSettings() {
    onComplete();
    emit("navigate", "settings");
  }

  async function handleSaveManualProfile() {
    if (!manualProfile.trim()) return;
    isSavingManual = true;
    error = "";
    try {
      await saveProfileEdit({ coreIdentity: manualProfile.trim() });
      step = "done";
    } catch (e) {
      error = friendlyError(e);
    } finally {
      isSavingManual = false;
    }
  }
</script>

<div class="flex flex-col h-full p-4 overflow-y-auto animate-fade-in-up">

  {#if step === "welcome"}
    <!-- Welcome screen -->
    <div class="flex-1 flex flex-col items-center justify-center gap-6 max-w-[280px] mx-auto">
      <div class="w-16 h-16 rounded-2xl flex items-center justify-center">
        <img src="/noren-logo.png" alt="Noren" class="w-full h-full object-contain" />
      </div>
      <div class="text-center">
        <h1 class="text-lg font-heading font-semibold text-foreground">Welcome to Noren</h1>
        <p class="text-xs text-muted mt-2 leading-relaxed">
          Noren learns how you write and helps you stay consistent. Let's build your voice profile.
        </p>
      </div>

      <div class="flex flex-col gap-2 w-full">
        <button
          onclick={() => checkAndProceed("paste")}
          class="w-full py-2.5 px-4 text-sm font-semibold bg-secondary text-white rounded-md hover:bg-secondary/90 transition-colors cursor-pointer text-left"
        >
          <span class="flex items-center gap-2">
            AI-powered extraction
            <span class="text-[10px] font-normal bg-white/20 px-1.5 py-0.5 rounded uppercase tracking-wide">$29 or Pro</span>
          </span>
          <span class="block text-[10px] font-normal text-white/70 mt-0.5">4-pass deep analysis of your writing patterns, vocabulary, and rhetorical style</span>
        </button>
        <button
          onclick={() => checkAndProceed("guided")}
          class="w-full py-2.5 px-4 text-sm font-medium bg-surface border border-secondary/30 text-foreground rounded-md hover:border-secondary transition-colors cursor-pointer text-left"
        >
          <span class="flex items-center gap-2">
            Guided interview
            <span class="text-[10px] font-normal text-secondary bg-secondary/10 px-1.5 py-0.5 rounded uppercase tracking-wide">$29 or Pro</span>
          </span>
          <span class="block text-[10px] font-normal text-muted mt-0.5">7 questions + style calibration, then AI builds your profile</span>
        </button>

        <div class="relative my-1">
          <div class="absolute inset-0 flex items-center">
            <div class="w-full border-t border-border"></div>
          </div>
          <div class="relative flex justify-center text-[10px]">
            <span class="px-2 bg-background text-muted">or start free</span>
          </div>
        </div>

        <button
          onclick={() => { step = "manual"; }}
          class="w-full py-2.5 px-4 text-sm font-medium bg-surface border border-border text-foreground rounded-md hover:border-secondary transition-colors cursor-pointer text-left"
        >
          Describe my voice manually
          <span class="block text-[10px] font-normal text-muted mt-0.5">Write your own profile description</span>
        </button>
        <button
          onclick={onComplete}
          class="w-full py-2 px-4 text-xs text-muted hover:text-foreground transition-colors cursor-pointer"
        >
          Skip for now
        </button>
      </div>
    </div>

  {:else if step === "auth"}
    <!-- Sign in / Sign up -->
    <div class="flex-1 flex flex-col items-center justify-center gap-4 max-w-[300px] mx-auto w-full">
      <div class="text-center mb-2">
        <h2 class="text-lg font-heading font-semibold text-foreground">Sign in to continue</h2>
        <p class="text-xs text-muted mt-1">Extraction requires a Noren account</p>
      </div>

      <div class="flex gap-1 w-full">
        <button
          onclick={() => { authMode = "login"; error = ""; }}
          class="flex-1 px-2 py-1 text-[10px] uppercase tracking-wide cursor-pointer rounded-md
            {authMode === 'login'
              ? 'bg-secondary text-white font-medium'
              : 'bg-surface text-muted border border-border'}"
        >
          Sign in
        </button>
        <button
          onclick={() => { authMode = "signup"; error = ""; }}
          class="flex-1 px-2 py-1 text-[10px] uppercase tracking-wide cursor-pointer rounded-md
            {authMode === 'signup'
              ? 'bg-secondary text-white font-medium'
              : 'bg-surface text-muted border border-border'}"
        >
          Create account
        </button>
      </div>

      <!-- Google Sign In -->
      <button
        onclick={handleGoogleSignIn}
        disabled={googleLoading || authLoading}
        class="w-full py-2 text-xs font-medium bg-surface border border-border text-foreground hover:border-secondary transition-colors cursor-pointer disabled:opacity-50 rounded-md flex items-center justify-center gap-2"
      >
        {#if googleLoading}
          <LoadingSpinner /> Waiting for Google...
        {:else}
          <svg class="w-3.5 h-3.5" viewBox="0 0 24 24">
            <path fill="#4285F4" d="M22.56 12.25c0-.78-.07-1.53-.2-2.25H12v4.26h5.92a5.06 5.06 0 0 1-2.2 3.32v2.77h3.57c2.08-1.92 3.28-4.74 3.28-8.1z"/>
            <path fill="#34A853" d="M12 23c2.97 0 5.46-.98 7.28-2.66l-3.57-2.77c-.98.66-2.23 1.06-3.71 1.06-2.86 0-5.29-1.93-6.16-4.53H2.18v2.84C3.99 20.53 7.7 23 12 23z"/>
            <path fill="#FBBC05" d="M5.84 14.09c-.22-.66-.35-1.36-.35-2.09s.13-1.43.35-2.09V7.07H2.18C1.43 8.55 1 10.22 1 12s.43 3.45 1.18 4.93l2.85-2.22.81-.62z"/>
            <path fill="#EA4335" d="M12 5.38c1.62 0 3.06.56 4.21 1.64l3.15-3.15C17.45 2.09 14.97 1 12 1 7.7 1 3.99 3.47 2.18 7.07l3.66 2.84c.87-2.6 3.3-4.53 6.16-4.53z"/>
          </svg>
          Sign in with Google
        {/if}
      </button>

      <div class="relative w-full">
        <div class="absolute inset-0 flex items-center">
          <div class="w-full border-t border-border"></div>
        </div>
        <div class="relative flex justify-center text-[10px]">
          <span class="px-2 bg-background text-muted">or</span>
        </div>
      </div>

      <input
        type="email"
        bind:value={authEmail}
        class="w-full px-3 py-1.5 text-xs border border-border bg-surface text-foreground rounded-md focus:outline-none focus:border-secondary"
        placeholder="Email"
      />
      <input
        type="password"
        bind:value={authPassword}
        onkeydown={(e) => { if (e.key === "Enter") handleProAuth(); }}
        class="w-full px-3 py-1.5 text-xs border border-border bg-surface text-foreground rounded-md focus:outline-none focus:border-secondary"
        placeholder="Password"
      />
      <button
        onclick={handleProAuth}
        disabled={authLoading || !authEmail.trim() || !authPassword.trim()}
        class="w-full py-2 text-xs font-medium bg-secondary text-white hover:bg-secondary/90 transition-colors cursor-pointer disabled:opacity-50 rounded-md"
      >
        {#if authLoading}
          <span class="inline-flex items-center gap-1"><LoadingSpinner /> {authMode === "signup" ? "Creating..." : "Signing in..."}</span>
        {:else}
          {authMode === "signup" ? "Create account" : "Sign in"}
        {/if}
      </button>

      {#if error}
        <div class="w-full p-2 bg-tint border border-border rounded-lg text-xs text-muted leading-relaxed">{error}</div>
      {/if}

      <button
        onclick={() => { step = "welcome"; error = ""; }}
        class="text-xs text-muted hover:text-foreground text-center cursor-pointer"
      >
        &larr; Back
      </button>
    </div>

  {:else if step === "paywall"}
    <!-- Paywall — authenticated but no extraction entitlement -->
    <div class="flex-1 flex flex-col items-center justify-center gap-4 max-w-[300px] mx-auto w-full">
      <div class="text-center mb-2">
        <h2 class="text-lg font-heading font-semibold text-foreground">Unlock extraction</h2>
        <p class="text-xs text-muted mt-1">AI extraction requires Pro or a one-time purchase</p>
      </div>

      <div class="flex flex-col gap-2 w-full">
        <button
          onclick={() => handleUpgrade("pro")}
          class="w-full py-3 px-4 text-sm font-semibold bg-secondary text-white rounded-md hover:bg-secondary/90 transition-colors cursor-pointer text-left"
        >
          <span class="flex items-center justify-between">
            <span>Noren Pro</span>
            <span class="text-xs font-normal">$19<span class="text-[10px] text-white/70">/mo</span></span>
          </span>
          <span class="block text-[10px] font-normal text-white/70 mt-0.5">Extraction, inference, living profile, sync — everything</span>
        </button>

        <button
          onclick={() => handleUpgrade("extraction")}
          class="w-full py-3 px-4 text-sm font-medium bg-surface border border-secondary/30 text-foreground rounded-md hover:border-secondary transition-colors cursor-pointer text-left"
        >
          <span class="flex items-center justify-between">
            <span>Voice extraction only</span>
            <span class="text-xs font-medium text-secondary">$29<span class="text-[10px] text-muted font-normal"> one-time</span></span>
          </span>
          <span class="block text-[10px] font-normal text-muted mt-0.5">AI extraction without a subscription</span>
        </button>
      </div>

      {#if error}
        <div class="w-full p-2 bg-tint border border-border rounded-lg text-xs text-muted leading-relaxed">{error}</div>
      {/if}

      <button
        onclick={() => { step = "welcome"; error = ""; }}
        class="text-xs text-muted hover:text-foreground text-center cursor-pointer"
      >
        &larr; Back
      </button>
    </div>

  {:else if step === "manual"}
    <!-- Manual profile creation -->
    <div class="flex flex-col gap-3 flex-1">
      <div>
        <span class="block text-xs font-medium text-muted mb-1 uppercase tracking-wide">Describe your voice</span>
        <p class="text-[10px] text-muted leading-relaxed">
          Write how you'd describe your writing style to someone. The more specific, the better Noren can match you.
        </p>
      </div>

      <div class="flex-1 flex flex-col min-h-0">
        <textarea
          bind:value={manualProfile}
          class="flex-1 p-3 text-xs leading-relaxed border border-border bg-surface text-foreground resize-none placeholder-muted rounded-md focus:outline-none focus:border-secondary"
          placeholder={"Example:\n\nI write casually and directly. Short sentences. I use contractions, avoid jargon, and get to the point fast. I'm opinionated but not aggressive — more like a friend giving honest advice. I occasionally use humor and rhetorical questions. I prefer active voice and concrete examples over abstract theory."}
        ></textarea>
      </div>

      <div class="p-2.5 bg-tint border border-secondary/20 rounded-lg flex flex-col gap-1.5">
        <p class="text-[10px] text-muted leading-relaxed">
          <span class="text-secondary font-medium">AI extraction</span> analyzes your actual writing — detecting sentence rhythm, vocabulary fingerprint, rhetorical moves, and format-specific adaptations.
        </p>
        <div class="flex gap-2 items-center">
          <button
            onclick={goToSettings}
            class="text-[10px] text-secondary font-medium cursor-pointer hover:text-foreground uppercase tracking-wide"
          >
            One-time $29
          </button>
          <span class="text-[10px] text-muted">or</span>
          <button
            onclick={goToSettings}
            class="text-[10px] text-secondary font-medium cursor-pointer hover:text-foreground uppercase tracking-wide"
          >
            Included with Pro
          </button>
        </div>
      </div>

      <button
        onclick={handleSaveManualProfile}
        disabled={!manualProfile.trim() || isSavingManual}
        class="w-full py-2.5 px-4 text-sm font-semibold transition-colors cursor-pointer rounded-md
          {!manualProfile.trim() || isSavingManual
            ? 'bg-surface text-muted border border-border cursor-not-allowed opacity-50'
            : 'bg-primary text-white hover:bg-primary-hover'}"
      >
        {isSavingManual ? "Saving..." : "Save Profile"}
      </button>

      {#if error}
        <div class="p-2 bg-tint border border-border rounded-lg text-xs text-muted leading-relaxed">{error}</div>
      {/if}

      <button
        onclick={() => { step = "welcome"; }}
        class="text-xs text-muted hover:text-foreground text-center cursor-pointer"
      >
        &larr; Back
      </button>
    </div>

  {:else if step === "paste"}
    <!-- Multi-format paste samples -->
    <div class="flex flex-col gap-3 flex-1">
      <div>
        <span class="block text-xs font-medium text-muted mb-1.5 uppercase tracking-wide">Paste samples by format</span>
        <p class="text-[10px] text-muted leading-relaxed">
          Switch between tabs and paste your writing for each format. Fill at least one format with 5+ samples.
        </p>
      </div>

      <!-- Format tabs -->
      <div class="flex flex-wrap gap-1">
        {#each formats as fmt}
          <button
            onclick={() => { activeFormat = fmt; }}
            class="relative px-2.5 py-1 text-xs transition-colors cursor-pointer uppercase tracking-wide rounded-md
              {activeFormat === fmt
                ? 'bg-primary text-white font-medium'
                : 'bg-surface text-muted border border-border hover:border-secondary hover:text-foreground'}"
          >
            {fmt}
            {#if hasAnySamples(fmt)}
              <span class="absolute -top-1 -right-1 w-2 h-2 rounded-full {sampleCount(formatSamples[fmt]) >= 5 ? 'bg-signal' : 'bg-secondary'}"></span>
            {/if}
          </button>
        {/each}
      </div>

      <!-- Textarea for active format -->
      <div class="flex-1 flex flex-col min-h-0">
        <div class="flex items-center justify-between mb-1.5">
          <span class="text-xs font-medium text-muted uppercase tracking-wide">{activeFormat} samples</span>
          {#if formatSamples[activeFormat].trim()}
            <span class="text-[10px] {sampleCount(formatSamples[activeFormat]) >= 5 ? 'text-signal' : 'text-muted'}">
              ~{sampleCount(formatSamples[activeFormat])} samples
            </span>
          {/if}
        </div>
        {#each formats as fmt}
          <textarea
            bind:value={formatSamples[fmt]}
            class="flex-1 p-3 text-xs leading-relaxed border border-border bg-surface text-foreground resize-none placeholder-muted rounded-md focus:outline-none focus:border-secondary
              {activeFormat !== fmt ? 'hidden' : ''}"
            placeholder="Paste your {fmt} writing samples here, separated by blank lines..."
          ></textarea>
        {/each}
      </div>

      <!-- Extract button -->
      <button
        onclick={handleStartExtraction}
        disabled={totalFilledFormats() === 0}
        class="w-full py-2.5 px-4 text-sm font-semibold transition-colors cursor-pointer rounded-md
          {totalFilledFormats() === 0
            ? 'bg-surface text-muted border border-border cursor-not-allowed opacity-50'
            : 'bg-primary text-white hover:bg-primary-hover'}"
      >
        {#if totalFilledFormats() > 0}
          Extract Voice Profile ({totalFilledFormats()} {totalFilledFormats() === 1 ? 'format' : 'formats'})
        {:else}
          Extract Voice Profile
        {/if}
      </button>

      {#if formatSamples[activeFormat].trim() && sampleCount(formatSamples[activeFormat]) < 5}
        <p class="text-[10px] text-muted text-center">
          Need at least 5 samples for {activeFormat}. Currently: ~{sampleCount(formatSamples[activeFormat])}
        </p>
      {/if}

      {#if error}
        <div class="p-2 bg-tint border border-border rounded-lg text-xs text-muted leading-relaxed">{error}</div>
      {/if}

      <button
        onclick={() => { step = "welcome"; }}
        class="text-xs text-muted hover:text-foreground text-center cursor-pointer"
      >
        &larr; Back
      </button>
    </div>

  {:else if step === "guided"}
    <!-- Guided questions -->
    <div class="flex-1 flex flex-col gap-4">
      <div class="flex items-center justify-between">
        <span class="text-[10px] text-muted uppercase tracking-wide">
          Question {currentQuestion + 1} of {questions.length}
        </span>
        <div class="flex gap-0.5">
          {#each questions as _, i}
            <div class="w-4 h-1 rounded-full {i < guidedAnswers.length ? 'bg-primary' : i === currentQuestion ? 'bg-secondary' : 'bg-border'}"></div>
          {/each}
        </div>
      </div>

      <div>
        <p class="text-sm font-medium text-foreground leading-relaxed">
          {questions[currentQuestion].prompt}
        </p>
        <p class="text-[10px] text-muted mt-1">
          {questions[currentQuestion].hint}
        </p>
      </div>

      <textarea
        bind:value={currentAnswer}
        onkeydown={(e) => { if (e.key === "Enter" && e.metaKey) submitGuidedAnswer(); }}
        class="flex-1 p-3 text-xs leading-relaxed border border-border bg-surface text-foreground resize-none placeholder-muted rounded-md focus:outline-none focus:border-secondary"
        placeholder="Type your answer..."
      ></textarea>

      <button
        onclick={submitGuidedAnswer}
        disabled={!currentAnswer.trim()}
        class="w-full py-2.5 px-4 text-sm font-semibold transition-colors cursor-pointer rounded-md
          {!currentAnswer.trim()
            ? 'bg-surface text-muted border border-border cursor-not-allowed opacity-50'
            : 'bg-primary text-white hover:bg-primary-hover'}"
      >
        {currentQuestion < questions.length - 1 ? "Next" : "Continue to calibration"}
      </button>

      <button
        onclick={() => { step = "welcome"; guidedAnswers = []; currentQuestion = 0; }}
        class="text-xs text-muted hover:text-foreground text-center cursor-pointer"
      >
        &larr; Back
      </button>
    </div>

  {:else if step === "guided-pairs"}
    <!-- Calibration pairs -->
    <div class="flex-1 flex flex-col gap-4">
      <div class="flex items-center justify-between">
        <span class="text-[10px] text-muted uppercase tracking-wide">
          Calibration {currentPair + 1} of {pairs.length}
        </span>
        <div class="flex gap-0.5">
          {#each pairs as _, i}
            <div class="w-4 h-1 rounded-full {i < pairChoices.length ? 'bg-primary' : i === currentPair ? 'bg-secondary' : 'bg-border'}"></div>
          {/each}
        </div>
      </div>

      <p class="text-xs font-medium text-foreground">
        {pairs[currentPair].question}
      </p>

      <div class="flex flex-col gap-2 flex-1">
        <button
          onclick={() => choosePair("a")}
          class="flex-1 p-3 text-xs text-left leading-relaxed border border-border bg-surface text-foreground rounded-md hover:border-secondary transition-colors cursor-pointer"
        >
          {pairs[currentPair].a}
        </button>
        <button
          onclick={() => choosePair("b")}
          class="flex-1 p-3 text-xs text-left leading-relaxed border border-border bg-surface text-foreground rounded-md hover:border-secondary transition-colors cursor-pointer"
        >
          {pairs[currentPair].b}
        </button>
      </div>
    </div>

  {:else if step === "done"}
    <!-- Done (manual profile only) -->
    <div class="flex-1 flex flex-col items-center justify-center gap-4">
      <div class="w-12 h-12 rounded-full bg-signal/10 flex items-center justify-center">
        <svg class="w-6 h-6 text-signal" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
          <path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7" />
        </svg>
      </div>
      <div class="text-center">
        <p class="text-sm font-semibold text-foreground">Basic profile saved</p>
        <p class="text-xs text-muted mt-1 leading-relaxed">
          Good start. Noren will use your description to match your tone.
        </p>
      </div>

      <div class="w-full max-w-[280px] p-3 bg-tint border border-secondary/20 rounded-lg">
        <p class="text-[10px] font-medium text-secondary mb-1.5">Want a deeper profile?</p>
        <div class="flex flex-col gap-1">
          <div class="flex items-start gap-1.5">
            <span class="text-secondary text-[10px] mt-0.5 shrink-0">+</span>
            <span class="text-[10px] text-muted">AI extraction from your actual writing</span>
          </div>
          <div class="flex items-start gap-1.5">
            <span class="text-secondary text-[10px] mt-0.5 shrink-0">+</span>
            <span class="text-[10px] text-muted">Format-specific contexts (Twitter, email, Slack...)</span>
          </div>
          <div class="flex items-start gap-1.5">
            <span class="text-secondary text-[10px] mt-0.5 shrink-0">+</span>
            <span class="text-[10px] text-muted">Living profile that evolves with your edits</span>
          </div>
        </div>
        <div class="flex gap-2 items-center mt-2">
          <button
            onclick={goToSettings}
            class="text-[10px] text-secondary font-medium cursor-pointer hover:text-foreground uppercase tracking-wide"
          >
            Extraction $29
          </button>
          <span class="text-[10px] text-muted">or</span>
          <button
            onclick={goToSettings}
            class="text-[10px] text-secondary font-medium cursor-pointer hover:text-foreground uppercase tracking-wide"
          >
            Pro $19/mo
          </button>
        </div>
      </div>

      <button
        onclick={onComplete}
        class="px-6 py-2.5 text-sm font-semibold bg-primary text-white rounded-md hover:bg-primary-hover transition-colors cursor-pointer"
      >
        Start writing
      </button>
    </div>
  {/if}
</div>
