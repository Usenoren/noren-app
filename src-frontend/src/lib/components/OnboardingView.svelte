<script lang="ts">
  import { listen, emit } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import { startExtraction, saveProfileEdit, type ExtractionProgress } from "../api/tauri";
  import { friendlyError } from "$lib/utils/errors";
  import LoadingSpinner from "./LoadingSpinner.svelte";

  // Events
  let { onComplete }: { onComplete: () => void } = $props();

  type Step = "welcome" | "paste" | "guided" | "guided-pairs" | "extracting" | "done" | "manual";
  let step: Step = $state("welcome");

  // Paste path
  let samples = $state("");
  let format = $state("twitter");
  const formats = ["twitter", "email", "longform", "slack", "linkedin"];

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
  let wasManual = $state(false);

  // Extraction progress
  let progress = $state<ExtractionProgress | null>(null);
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

  // --- Progress listener ---
  onMount(() => {
    const cleanups: (() => void)[] = [];

    listen<ExtractionProgress>("extraction-progress", (event) => {
      progress = event.payload;
      if (progress.status === "saved") {
        step = "done";
      } else if (progress.status === "failed") {
        error = progress.error || "Extraction failed";
        step = "paste"; // Let them retry
      }
    }).then((fn) => cleanups.push(fn));

    return () => cleanups.forEach((fn) => fn());
  });

  // --- Actions ---

  function sampleCount(): number {
    const text = samples.trim();
    if (!text) return 0;
    return text.split(/\n\s*\n/).filter((s) => s.trim()).length;
  }

  async function handleStartExtraction(text: string, fmt: string) {
    step = "extracting";
    error = "";
    progress = null;
    try {
      await startExtraction({ samples: text.trim(), format: fmt });
    } catch (e) {
      error = friendlyError(e);
      step = "paste";
    }
  }

  function submitGuidedAnswer() {
    if (!currentAnswer.trim()) return;
    guidedAnswers = [...guidedAnswers, currentAnswer.trim()];
    currentAnswer = "";
    if (currentQuestion < questions.length - 1) {
      currentQuestion++;
    } else {
      // Move to calibration pairs
      step = "guided-pairs";
      currentPair = 0;
    }
  }

  function choosePair(choice: "a" | "b") {
    pairChoices = [...pairChoices, choice];
    if (currentPair < pairs.length - 1) {
      currentPair++;
    } else {
      // Build samples from guided answers + pair preferences
      const guidedSamples = guidedAnswers.join("\n\n");
      const pairContext = pairs
        .map((p, i) => (pairChoices[i] === "a" ? p.a : p.b))
        .join("\n\n");
      samples = guidedSamples + "\n\n" + pairContext;
      format = "general";
      handleStartExtraction(samples, format);
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
      wasManual = true;
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
      <div class="text-center">
        <h1 class="text-lg font-heading font-semibold text-foreground">Welcome to Noren</h1>
        <p class="text-xs text-muted mt-2 leading-relaxed">
          Noren learns how you write and helps you stay consistent. Let's build your voice profile.
        </p>
      </div>

      <div class="flex flex-col gap-2 w-full">
        <!-- Extraction paths (premium) -->
        <button
          onclick={() => { step = "paste"; }}
          class="w-full py-2.5 px-4 text-sm font-semibold bg-secondary text-white rounded-md hover:bg-secondary/90 transition-colors cursor-pointer text-left"
        >
          <span class="flex items-center gap-2">
            AI-powered extraction
            <span class="text-[10px] font-normal bg-white/20 px-1.5 py-0.5 rounded uppercase tracking-wide">$29 or Pro</span>
          </span>
          <span class="block text-[10px] font-normal text-white/70 mt-0.5">4-pass deep analysis of your writing patterns, vocabulary, and rhetorical style</span>
        </button>
        <button
          onclick={() => { step = "guided"; currentQuestion = 0; guidedAnswers = []; }}
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

        <!-- Free path -->
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

      <!-- AI extraction nudge -->
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
    <!-- Paste samples -->
    <div class="flex flex-col gap-3 flex-1">
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

      <div class="flex-1 flex flex-col min-h-0">
        <div class="flex items-center justify-between mb-1.5">
          <span class="text-xs font-medium text-muted uppercase tracking-wide">Writing samples</span>
          {#if samples.trim()}
            <span class="text-[10px] text-muted">~{sampleCount()} samples</span>
          {/if}
        </div>
        <textarea
          bind:value={samples}
          class="flex-1 p-3 text-xs leading-relaxed border border-border bg-surface text-foreground resize-none placeholder-muted rounded-md focus:outline-none focus:border-secondary"
          placeholder="Paste 10+ writing samples here, separated by blank lines..."
        ></textarea>
      </div>

      <button
        onclick={() => handleStartExtraction(samples, format)}
        disabled={sampleCount() < 5}
        class="w-full py-2.5 px-4 text-sm font-semibold transition-colors cursor-pointer rounded-md
          {sampleCount() < 5
            ? 'bg-surface text-muted border border-border cursor-not-allowed opacity-50'
            : 'bg-primary text-white hover:bg-primary-hover'}"
      >
        Extract Voice Profile
      </button>

      {#if samples.trim() && sampleCount() < 5}
        <p class="text-[10px] text-muted text-center">
          Need at least 5 samples. Currently: ~{sampleCount()}
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

  {:else if step === "extracting"}
    <!-- Extraction progress -->
    <div class="flex-1 flex flex-col items-center justify-center gap-4">
      <LoadingSpinner />
      {#if progress}
        <div class="text-center">
          <p class="text-sm font-medium text-foreground">
            {#if progress.status === "pending"}Starting extraction...
            {:else if progress.status === "preprocessing"}Preprocessing samples...
            {:else if progress.status === "pass_1_core_identity"}Analyzing core identity...
            {:else if progress.status === "pass_2_surface_patterns"}Extracting surface patterns...
            {:else if progress.status === "pass_3_structural_patterns"}Mapping structural patterns...
            {:else if progress.status === "pass_4_rhetorical_patterns"}Identifying rhetorical moves...
            {:else if progress.status === "assembling"}Assembling voice profile...
            {:else if progress.status === "quality_check"}Running quality check...
            {:else}{progress.status}
            {/if}
          </p>
          <p class="text-xs text-muted mt-1">{progress.progress}% complete</p>
        </div>
        <div class="w-48 h-1.5 bg-tint rounded-full overflow-hidden">
          <div
            class="h-full bg-primary rounded-full transition-all duration-500 ease-out"
            style="width: {progress.progress}%"
          ></div>
        </div>
      {:else}
        <p class="text-sm text-muted">Starting...</p>
      {/if}
    </div>

  {:else if step === "done"}
    <!-- Done -->
    <div class="flex-1 flex flex-col items-center justify-center gap-4">
      <div class="w-12 h-12 rounded-full bg-signal/10 flex items-center justify-center">
        <svg class="w-6 h-6 text-signal" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
          <path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7" />
        </svg>
      </div>
      <div class="text-center">
        <p class="text-sm font-semibold text-foreground">
          {wasManual ? "Basic profile saved" : "Voice profile created"}
        </p>
        <p class="text-xs text-muted mt-1 leading-relaxed">
          {#if wasManual}
            Good start. Noren will use your description to match your tone.
          {:else}
            Noren now knows how you write. Start generating and it'll match your voice.
          {/if}
        </p>
      </div>

      {#if wasManual}
        <!-- Upgrade nudge for manual profiles -->
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
      {/if}

      <button
        onclick={onComplete}
        class="px-6 py-2.5 text-sm font-semibold bg-primary text-white rounded-md hover:bg-primary-hover transition-colors cursor-pointer"
      >
        Start writing
      </button>
    </div>
  {/if}
</div>
