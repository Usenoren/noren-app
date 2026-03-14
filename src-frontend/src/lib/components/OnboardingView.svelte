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
    redeemCoupon,
    createGuestCheckout,
    pollGuestCheckout,
    storePendingCheckout,
    clearPendingCheckout,
    storeExtractionReceipt,
    readFileAsText,
    verifyEmail,
    resendOtp,
    setInferenceMode,
    scrapeTwitter,
    scrapeBlog,
    type FormatGroup,
  } from "$lib/api/tauri";
  import { open } from "@tauri-apps/plugin-shell";
  import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
  import { canExtract, refresh as refreshSubscription } from "$lib/stores/subscription.svelte";
  import { startQueue as startExtractionQueue } from "$lib/stores/extraction.svelte";
  import { friendlyError } from "$lib/utils/errors";
  import LoadingSpinner from "./LoadingSpinner.svelte";
  import NorenMark from "./NorenMark.svelte";

  // Events
  let { onComplete }: { onComplete: () => void } = $props();

  type Step = "welcome" | "auth" | "otp" | "paywall" | "guest-checkout" | "awaiting-payment" | "payment-confirmed" | "input-method" | "paste" | "review" | "guided" | "guided-pairs" | "done" | "manual";
  let step: Step = $state("welcome");
  let pendingPath: "paste" | "guided" = $state("paste");

  // OTP verification state
  let otpCode = $state("");
  let otpLoading = $state(false);
  let otpMessage = $state("");
  let resendCooldown = $state(0);

  // Guest checkout state
  let guestEmail = $state("");
  let guestSessionId = $state("");
  let checkoutLoading = $state(false);

  // Coupon state
  let showCouponInput = $state(false);
  let couponCode = $state("");
  let couponLoading = $state(false);
  let couponMessage = $state("");
  let pendingCoupon = $state("");

  // Pro intent: auto-trigger upgrade after auth
  let proIntent = $state(false);

  // Auth state
  let authMode = $state<"login" | "signup">("login");
  let authEmail = $state("");
  let authPassword = $state("");
  let authLoading = $state(false);
  let googleLoading = $state(false);
  let isLoggedIn = $state(false);

  // Stepped sample input (wizard)
  const FORMAT_STEPS = [
    { format: "twitter", label: "Tweets / Social", guidance: "Paste 10-20 tweets or social posts. Copy from your Twitter/X archive or timeline." },
    { format: "email", label: "Emails", guidance: "Paste 2-3 full email threads. The kind you write most, not one-liners." },
    { format: "longform", label: "Long-form", guidance: "Blog posts, essays, articles, newsletter issues. Even one long piece helps." },
    { format: "slack", label: "Slack / Chat", guidance: "Paste longer Slack messages or chat threads. Skip quick replies." },
    { format: "linkedin", label: "LinkedIn", guidance: "Posts, comments, or articles from LinkedIn." },
  ];
  let currentFormatStep = $state(0);
  let formatSamples = $state<Record<string, string>>({});
  let currentInput = $state("");

  // Scrape state
  const SCRAPABLE_FORMATS = ["twitter", "longform"];
  let scrapeHandle = $state("");
  let scrapeUrl = $state("");
  let isScraping = $state(false);
  let scrapeError = $state("");
  let scrapeInfoMap: Record<string, string> = $state({});
  let canScrapeStep = $derived(SCRAPABLE_FORMATS.includes(FORMAT_STEPS[currentFormatStep].format));

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

    // Check entitlement (works for both authed and non-authed users)
    await refreshSubscription();
    if (canExtract()) {
      if (path === "guided") {
        step = "guided";
        currentQuestion = 0;
        guidedAnswers = [];
      } else {
        step = "input-method";
      }
    } else {
      step = "paywall";
    }
  }

  async function handleProAuth() {
    if (!authEmail.trim() || !authPassword.trim()) return;
    const email = authEmail.trim();
    if (!email.includes("@") || !email.includes(".")) {
      error = "Enter a valid email address.";
      return;
    }
    authLoading = true;
    error = "";
    try {
      if (authMode === "signup") {
        await norenProSignup(authEmail.trim(), authPassword.trim());
        authPassword = "";
        otpMessage = "Check your email for a verification code.";
        step = "otp";
        startResendCooldown();
      } else {
        await norenProLogin(authEmail.trim(), authPassword.trim());
        isLoggedIn = true;
        authEmail = "";
        authPassword = "";
        await afterAuth();
      }
    } catch (e) {
      error = friendlyError(e);
    } finally {
      authLoading = false;
    }
  }

  function startResendCooldown() {
    resendCooldown = 60;
    const interval = setInterval(() => {
      resendCooldown--;
      if (resendCooldown <= 0) clearInterval(interval);
    }, 1000);
  }

  async function handleVerifyOtp() {
    if (!otpCode.trim()) return;
    otpLoading = true;
    error = "";
    otpMessage = "";
    try {
      await verifyEmail(otpCode.trim());
      isLoggedIn = true;
      otpCode = "";
      authEmail = "";
      await setInferenceMode("noren_pro");
      await afterAuth();
    } catch (e) {
      error = friendlyError(e);
    } finally {
      otpLoading = false;
    }
  }

  async function handleResendOtp() {
    if (resendCooldown > 0) return;
    error = "";
    otpMessage = "";
    try {
      const msg = await resendOtp();
      otpMessage = msg;
      startResendCooldown();
    } catch (e) {
      error = friendlyError(e);
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
            await afterAuth();
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

  async function handleUpgrade(tier: string, promoCode?: string) {
    error = "";
    try {
      const result = await createCheckout(tier, promoCode);
      if (result.checkout_url === "dev://granted") {
        await refreshSubscription();
        if (canExtract()) {
          proceedAfterPayment();
        }
      } else {
        await open(result.checkout_url);
        // Poll for subscription change
        for (let i = 0; i < 150; i++) {
          await new Promise((r) => setTimeout(r, 2000));
          await refreshSubscription();
          if (canExtract()) {
            proceedAfterPayment();
            return;
          }
        }
      }
    } catch (e) {
      error = friendlyError(e);
    }
  }

  async function handleApplyCoupon() {
    const code = couponCode.trim();
    if (!code) return;

    // Coupon requires auth. If not logged in, stash code and redirect to auth.
    const settings = await getSettings();
    isLoggedIn = settings.noren_pro_logged_in;
    if (!isLoggedIn) {
      pendingCoupon = code;
      step = "auth";
      return;
    }

    await applyCouponCode(code);
  }

  async function applyCouponCode(code: string) {
    couponLoading = true;
    showCouponInput = true;
    couponMessage = "";
    error = "";
    try {
      await redeemCoupon(code);
      showCouponInput = false;
      couponCode = "";
      pendingCoupon = "";
      await refreshSubscription();
      if (canExtract()) {
        proceedAfterPayment();
      }
    } catch (e) {
      const msg = String(e);
      const match = msg.match(/^(\d{3}):(.+)$/);
      if (match) {
        const status = parseInt(match[1]);
        const detail = match[2];
        if (status === 404) {
          // Not a trial coupon, try as Stripe promo code via checkout
          couponMessage = "";
          couponLoading = false;
          step = "awaiting-payment";
          await handleUpgrade("pro", code);
          return;
        } else {
          // 400 (expired/limit), 409 (already redeemed)
          couponMessage = detail;
        }
      } else {
        error = friendlyError(e);
      }
    } finally {
      couponLoading = false;
    }
  }

  // --- Post-auth routing ---

  async function afterAuth() {
    await refreshSubscription();
    if (canExtract()) {
      proIntent = false;
      pendingCoupon = "";
      if (pendingPath === "guided") {
        step = "guided";
        currentQuestion = 0;
        guidedAnswers = [];
      } else {
        step = "input-method";
      }
    } else if (pendingCoupon) {
      // User entered a coupon on the paywall before auth. Apply it now.
      const code = pendingCoupon;
      pendingCoupon = "";
      proIntent = false;
      couponCode = code;
      couponLoading = true;
      showCouponInput = true;
      step = "paywall";
      await applyCouponCode(code);
    } else if (proIntent) {
      proIntent = false;
      // Kick off Pro checkout and show the waiting screen
      step = "awaiting-payment";
      handleUpgrade("pro");
    } else {
      step = "paywall";
    }
  }

  // --- Pro path (requires auth) ---

  async function handleStartPro() {
    error = "";
    const settings = await getSettings();
    isLoggedIn = settings.noren_pro_logged_in;
    if (!isLoggedIn) {
      proIntent = true;
      step = "auth";
      return;
    }
    handleUpgrade("pro");
  }

  // --- Guest checkout ---

  async function handleGuestCheckout() {
    if (!guestEmail.trim()) return;
    const email = guestEmail.trim();
    if (!email.includes("@") || !email.includes(".")) {
      error = "Enter a valid email address.";
      return;
    }
    checkoutLoading = true;
    error = "";
    try {
      const result = await createGuestCheckout(guestEmail.trim(), "extraction");
      guestSessionId = result.session_id;

      // Persist pending before opening Stripe
      await storePendingCheckout(result.session_id, guestEmail.trim());

      if (result.checkout_url === "dev://granted") {
        // Dev mode: skip Stripe
        await storeExtractionReceipt(result.session_id);
        await clearPendingCheckout();
        await refreshSubscription();
        proceedAfterPayment();
        return;
      }

      await open(result.checkout_url);
      step = "awaiting-payment";
    } catch (e) {
      error = friendlyError(e);
    } finally {
      checkoutLoading = false;
    }
  }

  async function handleCheckPayment() {
    if (!guestSessionId) return;
    checkoutLoading = true;
    error = "";
    try {
      const status = await pollGuestCheckout(guestSessionId);
      if (status.paid) {
        await storeExtractionReceipt(guestSessionId);
        await clearPendingCheckout();
        await refreshSubscription();
        proceedAfterPayment();
      } else {
        error = "Payment not yet received. Complete checkout in your browser, then check again.";
      }
    } catch (e) {
      error = friendlyError(e);
    } finally {
      checkoutLoading = false;
    }
  }

  function proceedAfterPayment() {
    step = "payment-confirmed";
  }

  function continueAfterPayment() {
    if (pendingPath === "guided") {
      step = "guided";
      currentQuestion = 0;
      guidedAnswers = [];
    } else {
      step = "input-method";
    }
  }

  // --- File upload ---

  async function handleFileUpload() {
    error = "";
    try {
      const selected = await openFileDialog({
        filters: [{ name: "Text", extensions: ["txt", "md"] }],
        multiple: false,
      });
      if (!selected) return;
      const content = await readFileAsText(selected);
      if (!content.trim()) {
        error = "File is empty.";
        return;
      }
      formatSamples = { longform: content };
      scrapeInfoMap = {};
      step = "review";
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

  function totalSamples(): number {
    return Object.values(formatSamples).reduce(
      (sum, text) => sum + sampleCount(text),
      0,
    );
  }

  function formatGroups(): FormatGroup[] {
    return Object.entries(formatSamples)
      .filter(([_, text]) => text.trim())
      .map(([format, samples]) => ({ format, samples }));
  }

  // --- Stepped wizard navigation ---

  function enterFormatStep(stepIndex: number) {
    currentFormatStep = stepIndex;
    currentInput = formatSamples[FORMAT_STEPS[stepIndex].format] || "";
    resetScrapeState();
  }

  function startPasteFlow() {
    formatSamples = {};
    scrapeInfoMap = {};
    enterFormatStep(0);
    step = "paste";
  }

  function nextFormatStep() {
    if (currentInput.trim()) {
      formatSamples[FORMAT_STEPS[currentFormatStep].format] = currentInput.trim();
    }
    if (currentFormatStep < FORMAT_STEPS.length - 1) {
      enterFormatStep(currentFormatStep + 1);
    } else {
      step = "review";
    }
  }

  function skipFormatStep() {
    if (currentFormatStep < FORMAT_STEPS.length - 1) {
      enterFormatStep(currentFormatStep + 1);
    } else {
      step = "review";
    }
  }

  function prevFormatStep() {
    if (currentInput.trim()) {
      formatSamples[FORMAT_STEPS[currentFormatStep].format] = currentInput.trim();
    }
    if (currentFormatStep > 0) {
      enterFormatStep(currentFormatStep - 1);
    } else {
      step = "input-method";
    }
  }

  function goBackToFormatStep(index: number) {
    enterFormatStep(index);
    step = "paste";
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
    if (!handle) { scrapeError = "Enter a username or profile link."; return; }
    const targetStep = currentFormatStep;
    const targetFormat = FORMAT_STEPS[targetStep].format;
    scrapeError = "";
    isScraping = true;
    try {
      const result = await scrapeTwitter(handle);
      formatSamples[targetFormat] = result.format_group.samples;
      if (currentFormatStep === targetStep) currentInput = result.format_group.samples;
      scrapeInfoMap["twitter"] = `Fetched ${result.meta.total_kept} tweets`;
    } catch (e) {
      if (currentFormatStep === targetStep) scrapeError = friendlyError(e);
    } finally {
      isScraping = false;
    }
  }

  async function handleScrapeBlog() {
    const url = scrapeUrl.trim();
    if (!url || !url.startsWith("http")) { scrapeError = "Enter a valid URL starting with http:// or https://"; return; }
    const targetStep = currentFormatStep;
    const targetFormat = FORMAT_STEPS[targetStep].format;
    scrapeError = "";
    isScraping = true;
    try {
      const result = await scrapeBlog(url);
      formatSamples[targetFormat] = result.format_group.samples;
      if (currentFormatStep === targetStep) currentInput = result.format_group.samples;
      const label = result.meta.source_type === "rss" ? "posts" : "article";
      scrapeInfoMap["longform"] = `Fetched ${result.meta.total_kept} ${label}`;
    } catch (e) {
      if (currentFormatStep === targetStep) scrapeError = friendlyError(e);
    } finally {
      isScraping = false;
    }
  }

  // --- Extraction ---

  function handleStartExtraction() {
    const groups = formatGroups();
    if (groups.length === 0 || totalSamples() < 5) return;
    startExtractionQueue(groups);
    step = "done";
  }

  function handleGuidedExtraction() {
    const guidedSamples = guidedAnswers.join("\n\n");
    const pairContext = pairs
      .map((p, i) => (pairChoices[i] === "a" ? p.a : p.b))
      .join("\n\n");
    const combinedSamples = guidedSamples + "\n\n" + pairContext;

    // Fire off background extraction and show done screen
    startExtractionQueue([{ samples: combinedSamples.trim(), format: "general" }]);
    step = "done";
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
      // Verify extraction access before running
      await refreshSubscription();
      if (!canExtract()) {
        pendingPath = "guided";
        step = "paywall";
        return;
      }
      handleGuidedExtraction();
    }
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
    <!-- Welcome screen: v3 two-zone layout -->
    <div class="flex-1 flex flex-col -m-4 overflow-y-auto">

      <!-- Zone 1: Brand header (warm background with woven grid) -->
      <div
        class="relative text-center shrink-0"
        style="
          padding: 40px 40px 32px;
          background-color: var(--color-background);
          background-image:
            repeating-linear-gradient(0deg, transparent, transparent 27px, rgba(30,49,72,0.025) 27px, rgba(30,49,72,0.025) 28px),
            repeating-linear-gradient(90deg, transparent, transparent 27px, rgba(30,49,72,0.025) 27px, rgba(30,49,72,0.025) 28px);
        "
      >
        <div class="flex items-center justify-center mb-3.5" style="color: var(--color-primary)">
          <NorenMark width={40} height={48} />
        </div>
        <h1 class="font-heading" style="font-size:34px; font-weight:300; letter-spacing:3px; line-height:1; color:var(--color-primary)">noren</h1>
        <p class="text-muted mx-auto" style="font-size:12px; margin-top:12px; line-height:1.6; max-width:220px">
          Learn how you write. Stay consistent across everything.
        </p>
        <!-- Bottom divider gradient -->
        <div class="absolute bottom-0 left-0 right-0 h-px" style="background: linear-gradient(90deg, transparent, rgba(30,49,72,0.1) 30%, rgba(30,49,72,0.1) 70%, transparent)"></div>
      </div>

      <!-- Zone 2: Method selection (white background) -->
      <div class="flex-1 flex flex-col gap-2.5 bg-surface" style="padding: 24px 32px 20px">

        <!-- Primary card: Extract my voice -->
        <button
          onclick={() => checkAndProceed("paste")}
          class="relative overflow-hidden rounded-[10px] flex gap-3.5 items-start text-left w-full cursor-pointer border-none transition-all duration-200"
          style="
            padding: 16px 18px;
            background: var(--color-primary);
            color: white;
            box-shadow: 0 2px 8px rgba(30,49,72,0.15), 0 8px 24px rgba(30,49,72,0.1);
          "
          onmouseenter={(e) => { e.currentTarget.style.background = '#243A54'; e.currentTarget.style.boxShadow = '0 4px 12px rgba(30,49,72,0.2), 0 12px 32px rgba(30,49,72,0.15)'; e.currentTarget.style.transform = 'translateY(-1px)'; }}
          onmouseleave={(e) => { e.currentTarget.style.background = 'var(--color-primary)'; e.currentTarget.style.boxShadow = '0 2px 8px rgba(30,49,72,0.15), 0 8px 24px rgba(30,49,72,0.1)'; e.currentTarget.style.transform = 'translateY(0)'; }}
        >
          <!-- Thread texture overlay -->
          <div class="absolute inset-0 pointer-events-none" style="background-image: repeating-linear-gradient(90deg, transparent, transparent 11px, rgba(255,255,255,0.02) 11px, rgba(255,255,255,0.02) 12px)"></div>

          <div class="shrink-0 flex items-center justify-center rounded-lg" style="width:34px; height:34px; background:rgba(255,255,255,0.1); margin-top:1px">
            <svg class="w-[17px] h-[17px]" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8">
              <path d="M15 4V2M15 16v-2M8 9h10M8 5h2m-2 8h2m4 6l-6-6 6-6"/>
            </svg>
          </div>
          <div class="flex-1 min-w-0 relative z-[1]">
            <div style="font-size:13px; font-weight:600; line-height:1.3">Extract my voice</div>
            <div style="font-size:10.5px; line-height:1.5; margin-top:3px; opacity:0.5">AI analyzes your writing patterns, vocabulary, and rhetorical style</div>
          </div>
        </button>

        <!-- Secondary card: Guided interview -->
        <button
          onclick={() => checkAndProceed("guided")}
          class="rounded-[10px] flex gap-3.5 items-start text-left w-full cursor-pointer bg-surface text-foreground transition-all duration-200"
          style="
            padding: 16px 18px;
            border: 1px solid var(--color-border);
          "
          onmouseenter={(e) => { e.currentTarget.style.borderColor = 'var(--color-secondary)'; e.currentTarget.style.boxShadow = '0 2px 8px rgba(59,107,138,0.08)'; e.currentTarget.style.transform = 'translateY(-1px)'; }}
          onmouseleave={(e) => { e.currentTarget.style.borderColor = 'var(--color-border)'; e.currentTarget.style.boxShadow = 'none'; e.currentTarget.style.transform = 'translateY(0)'; }}
        >
          <div class="shrink-0 flex items-center justify-center rounded-lg bg-tint" style="width:34px; height:34px; margin-top:1px">
            <svg class="w-[17px] h-[17px] text-secondary" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8">
              <path d="M21 15a2 2 0 01-2 2H7l-4 4V5a2 2 0 012-2h14a2 2 0 012 2z"/>
            </svg>
          </div>
          <div class="flex-1 min-w-0">
            <div style="font-size:13px; font-weight:600; line-height:1.3">Guided interview</div>
            <div style="font-size:10.5px; line-height:1.5; margin-top:3px; opacity:0.65">7 questions + style calibration builds your profile</div>
          </div>
        </button>

        <!-- Tertiary section (pushed to bottom) -->
        <div class="mt-auto flex flex-col items-center gap-1.5" style="padding-top:14px; border-top: 1px solid var(--color-tint)">
          <button
            onclick={() => { step = "manual"; }}
            class="cursor-pointer bg-transparent border-none text-secondary hover:text-primary transition-colors"
            style="font-size:11px; font-weight:500; padding:4px"
          >
            Write my own profile
          </button>
          <button
            onclick={() => { proIntent = false; step = "auth"; }}
            class="cursor-pointer bg-transparent border-none text-muted hover:text-foreground transition-colors"
            style="font-size:10.5px; padding:4px"
          >
            Already have an account? Sign in
          </button>
          <button
            onclick={onComplete}
            class="cursor-pointer bg-transparent border-none text-muted opacity-50 transition-opacity hover:opacity-100"
            style="font-size:10px; padding:4px"
          >
            Skip for now
          </button>
        </div>
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
        onclick={() => { step = "paywall"; error = ""; proIntent = false; }}
        class="text-xs text-muted hover:text-foreground text-center cursor-pointer"
      >
        &larr; Back
      </button>
    </div>

  {:else if step === "otp"}
    <!-- OTP Verification -->
    <div class="flex-1 flex flex-col items-center justify-center gap-4 max-w-[300px] mx-auto w-full">
      <div class="text-center mb-2">
        <h2 class="text-lg font-heading font-semibold text-foreground">Verify your email</h2>
        <p class="text-xs text-muted mt-1">
          We sent a verification code to <span class="font-medium text-foreground">{authEmail}</span>
        </p>
      </div>

      <input
        type="text"
        bind:value={otpCode}
        onkeydown={(e) => { if (e.key === "Enter") handleVerifyOtp(); }}
        class="w-full px-3 py-2 text-sm text-center tracking-[0.3em] border border-border bg-surface text-foreground rounded-md focus:outline-none focus:border-secondary"
        placeholder="000000"
        maxlength={6}
        autocomplete="one-time-code"
      />

      <button
        onclick={handleVerifyOtp}
        disabled={otpLoading || !otpCode.trim()}
        class="w-full py-2 text-xs font-medium bg-secondary text-white hover:bg-secondary/90 transition-colors cursor-pointer disabled:opacity-50 rounded-md"
      >
        {#if otpLoading}
          <span class="inline-flex items-center gap-1"><LoadingSpinner /> Verifying...</span>
        {:else}
          Verify email
        {/if}
      </button>

      {#if otpMessage}
        <p class="text-[10px] text-secondary">{otpMessage}</p>
      {/if}

      {#if error}
        <div class="w-full p-2 bg-tint border border-border rounded-lg text-xs text-muted leading-relaxed">{error}</div>
      {/if}

      <div class="flex items-center justify-between w-full">
        <button
          onclick={handleResendOtp}
          disabled={resendCooldown > 0}
          class="text-[10px] transition-colors cursor-pointer {resendCooldown > 0 ? 'text-muted/50' : 'text-muted hover:text-foreground underline'}"
        >
          {resendCooldown > 0 ? `Resend in ${resendCooldown}s` : "Resend code"}
        </button>
        <button
          onclick={() => { step = "auth"; otpCode = ""; error = ""; otpMessage = ""; }}
          class="text-[10px] text-muted hover:text-foreground transition-colors cursor-pointer"
        >
          Back
        </button>
      </div>
    </div>

  {:else if step === "paywall"}
    <!-- Extraction gate -->
    <div class="flex-1 flex flex-col -m-4 overflow-y-auto">

      <!-- Header -->
      <div class="shrink-0 text-center bg-surface" style="padding: 32px 32px 24px">
        <div class="flex items-center justify-center mb-3" style="color: var(--color-primary)">
          <NorenMark width={28} height={34} />
        </div>
        <h2 class="font-heading text-foreground" style="font-size:18px; font-weight:600; line-height:1.3">Choose your path</h2>
        <p class="text-muted mx-auto" style="font-size:11.5px; margin-top:8px; line-height:1.5; max-width:260px">
          AI extraction analyzes your writing patterns. Included with Pro, or available as a one-time purchase.
        </p>
      </div>

      <!-- Cards -->
      <div class="flex-1 flex flex-col gap-2.5 bg-surface" style="padding: 0 32px 20px">

        <!-- Pro card (primary) -->
        <button
          onclick={() => handleStartPro()}
          class="relative overflow-hidden rounded-[10px] flex gap-3.5 items-start text-left w-full cursor-pointer border-none transition-all duration-200"
          style="
            padding: 16px 18px;
            background: var(--color-primary);
            color: white;
            box-shadow: 0 2px 8px rgba(30,49,72,0.15), 0 8px 24px rgba(30,49,72,0.1);
          "
          onmouseenter={(e) => { e.currentTarget.style.background = '#243A54'; e.currentTarget.style.boxShadow = '0 4px 12px rgba(30,49,72,0.2), 0 12px 32px rgba(30,49,72,0.15)'; e.currentTarget.style.transform = 'translateY(-1px)'; }}
          onmouseleave={(e) => { e.currentTarget.style.background = 'var(--color-primary)'; e.currentTarget.style.boxShadow = '0 2px 8px rgba(30,49,72,0.15), 0 8px 24px rgba(30,49,72,0.1)'; e.currentTarget.style.transform = 'translateY(0)'; }}
        >
          <div class="absolute inset-0 pointer-events-none" style="background-image: repeating-linear-gradient(90deg, transparent, transparent 11px, rgba(255,255,255,0.02) 11px, rgba(255,255,255,0.02) 12px)"></div>

          <div class="shrink-0 flex items-center justify-center rounded-lg" style="width:34px; height:34px; background:rgba(255,255,255,0.1); margin-top:1px">
            <svg class="w-[17px] h-[17px]" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8">
              <path d="M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01L12 2z"/>
            </svg>
          </div>
          <div class="flex-1 min-w-0 relative z-[1]">
            <div class="flex items-center justify-between">
              <span style="font-size:13px; font-weight:600; line-height:1.3">Start Pro</span>
              <span style="font-size:12px; font-weight:400">$7<span style="font-size:10px; opacity:0.6">/mo</span></span>
            </div>
            <div style="font-size:10.5px; line-height:1.5; margin-top:3px; opacity:0.5">Extraction, inference, living profile, sync. Everything.</div>
          </div>
        </button>

        <!-- One-time card (secondary) -->
        <button
          onclick={() => { step = "guest-checkout"; error = ""; }}
          class="rounded-[10px] flex gap-3.5 items-start text-left w-full cursor-pointer bg-surface text-foreground transition-all duration-200"
          style="
            padding: 16px 18px;
            border: 1px solid var(--color-border);
          "
          onmouseenter={(e) => { e.currentTarget.style.borderColor = 'var(--color-secondary)'; e.currentTarget.style.boxShadow = '0 2px 8px rgba(59,107,138,0.08)'; e.currentTarget.style.transform = 'translateY(-1px)'; }}
          onmouseleave={(e) => { e.currentTarget.style.borderColor = 'var(--color-border)'; e.currentTarget.style.boxShadow = 'none'; e.currentTarget.style.transform = 'translateY(0)'; }}
        >
          <div class="shrink-0 flex items-center justify-center rounded-lg bg-tint" style="width:34px; height:34px; margin-top:1px">
            <svg class="w-[17px] h-[17px] text-secondary" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8">
              <path d="M21 4H3a1 1 0 00-1 1v14a1 1 0 001 1h18a1 1 0 001-1V5a1 1 0 00-1-1zM2 9h20M7 15h4"/>
            </svg>
          </div>
          <div class="flex-1 min-w-0">
            <div class="flex items-center justify-between">
              <span style="font-size:13px; font-weight:600; line-height:1.3">One-time extraction</span>
              <span class="text-secondary" style="font-size:12px; font-weight:500">$19</span>
            </div>
            <div style="font-size:10.5px; line-height:1.5; margin-top:3px; opacity:0.65">AI extraction without a subscription. No account needed.</div>
          </div>
        </button>

        {#if error}
          <div class="w-full p-2 bg-tint border border-border rounded-lg text-xs text-muted leading-relaxed">{error}</div>
        {/if}

        <!-- Coupon input -->
        {#if !showCouponInput}
          <button
            onclick={() => { showCouponInput = true; couponMessage = ""; error = ""; }}
            class="cursor-pointer bg-transparent border-none text-muted hover:text-foreground transition-colors self-center"
            style="font-size:10.5px; padding:4px"
          >
            Have a coupon?
          </button>
        {:else}
          <div class="flex flex-col gap-1.5" style="padding: 0 4px">
            <div class="flex gap-1.5">
              <input
                type="text"
                bind:value={couponCode}
                onkeydown={(e) => { if (e.key === "Enter") handleApplyCoupon(); }}
                class="flex-1 px-2.5 py-1.5 text-xs border border-border bg-surface text-foreground rounded-md focus:outline-none focus:border-secondary"
                placeholder="Coupon or promo code"
              />
              <button
                onclick={handleApplyCoupon}
                disabled={couponLoading || !couponCode.trim()}
                class="px-3 py-1.5 text-[10px] font-medium bg-secondary text-white hover:bg-secondary/90 transition-colors cursor-pointer disabled:opacity-50 rounded-md whitespace-nowrap"
              >
                {couponLoading ? "..." : "Apply"}
              </button>
            </div>
            {#if couponMessage}
              <p class="text-[10px] text-muted">{couponMessage}</p>
            {/if}
            <button
              onclick={() => { showCouponInput = false; couponCode = ""; couponMessage = ""; }}
              class="cursor-pointer bg-transparent border-none text-[10px] text-muted hover:text-foreground transition-colors self-start"
            >
              Cancel
            </button>
          </div>
        {/if}

        <!-- Tertiary -->
        <div class="mt-auto flex flex-col items-center gap-1.5" style="padding-top:14px; border-top: 1px solid var(--color-tint)">
          <button
            onclick={() => { step = "welcome"; error = ""; }}
            class="cursor-pointer bg-transparent border-none text-muted opacity-50 transition-opacity hover:opacity-100"
            style="font-size:10px; padding:4px"
          >
            &larr; Back
          </button>
        </div>
      </div>
    </div>

  {:else if step === "guest-checkout"}
    <!-- Guest checkout: email input -->
    <div class="flex-1 flex flex-col -m-4 overflow-y-auto">
      <div class="shrink-0 text-center bg-surface" style="padding: 32px 32px 24px">
        <h2 class="font-heading text-foreground" style="font-size:18px; font-weight:600; line-height:1.3">One-time extraction</h2>
        <p class="text-muted mx-auto" style="font-size:11.5px; margin-top:8px; line-height:1.5; max-width:260px">
          Enter your email for the receipt. No account created.
        </p>
      </div>

      <div class="flex-1 flex flex-col gap-3 bg-surface" style="padding: 0 32px 20px">
        <div class="rounded-[10px] p-4" style="background: var(--color-tint); border: 1px solid var(--color-border)">
          <div class="flex items-center justify-between mb-3">
            <span class="text-foreground" style="font-size:13px; font-weight:600">AI voice extraction</span>
            <span class="text-secondary" style="font-size:14px; font-weight:600">$19</span>
          </div>
          <div class="flex flex-col gap-1">
            <div class="flex items-center gap-2">
              <span class="text-secondary" style="font-size:10px">+</span>
              <span class="text-muted" style="font-size:10.5px">4-pass analysis of your writing patterns</span>
            </div>
            <div class="flex items-center gap-2">
              <span class="text-secondary" style="font-size:10px">+</span>
              <span class="text-muted" style="font-size:10.5px">50+ named voice patterns extracted</span>
            </div>
            <div class="flex items-center gap-2">
              <span class="text-secondary" style="font-size:10px">+</span>
              <span class="text-muted" style="font-size:10.5px">One-time purchase, no subscription</span>
            </div>
          </div>
        </div>

        <input
          type="email"
          bind:value={guestEmail}
          onkeydown={(e) => { if (e.key === "Enter") handleGuestCheckout(); }}
          class="w-full px-3 py-2.5 text-xs border border-border bg-surface text-foreground rounded-[10px] focus:outline-none focus:border-secondary"
          placeholder="your@email.com"
        />

        <button
          onclick={handleGuestCheckout}
          disabled={checkoutLoading || !guestEmail.trim()}
          class="w-full py-2.5 text-xs font-semibold transition-colors cursor-pointer rounded-[10px] disabled:opacity-50 disabled:cursor-not-allowed"
          style="background: var(--color-primary); color: white"
          onmouseenter={(e) => { if (!e.currentTarget.disabled) e.currentTarget.style.background = '#243A54'; }}
          onmouseleave={(e) => { e.currentTarget.style.background = 'var(--color-primary)'; }}
        >
          {#if checkoutLoading}
            <span class="inline-flex items-center gap-1"><LoadingSpinner /> Opening checkout...</span>
          {:else}
            Continue to checkout
          {/if}
        </button>

        {#if error}
          <div class="w-full p-2 bg-tint border border-border rounded-lg text-xs text-muted leading-relaxed">{error}</div>
        {/if}

        <div class="mt-auto flex flex-col items-center gap-1.5" style="padding-top:14px; border-top: 1px solid var(--color-tint)">
          <button
            onclick={() => { step = "paywall"; error = ""; }}
            class="cursor-pointer bg-transparent border-none text-muted opacity-50 transition-opacity hover:opacity-100"
            style="font-size:10px; padding:4px"
          >
            &larr; Back
          </button>
        </div>
      </div>
    </div>

  {:else if step === "awaiting-payment"}
    <!-- Awaiting payment: polling Stripe -->
    <div class="flex-1 flex flex-col -m-4 overflow-y-auto">
      <div class="flex-1 flex flex-col items-center justify-center gap-5 bg-surface" style="padding: 32px">
        <div class="flex items-center justify-center" style="color: var(--color-secondary)">
          <svg class="w-10 h-10 animate-spin" style="animation-duration: 3s" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
            <path d="M12 2a10 10 0 0 1 10 10" stroke-linecap="round"/>
          </svg>
        </div>
        <div class="text-center">
          <h2 class="font-heading text-foreground" style="font-size:16px; font-weight:600; line-height:1.3">Complete payment in your browser</h2>
          <p class="text-muted mx-auto" style="font-size:11px; margin-top:8px; line-height:1.5; max-width:240px">
            Stripe checkout is open in your browser. Come back here when you're done.
          </p>
        </div>

        <button
          onclick={handleCheckPayment}
          disabled={checkoutLoading}
          class="py-2.5 px-6 text-xs font-semibold transition-colors cursor-pointer rounded-[10px] disabled:opacity-50"
          style="background: var(--color-primary); color: white"
          onmouseenter={(e) => { if (!e.currentTarget.disabled) e.currentTarget.style.background = '#243A54'; }}
          onmouseleave={(e) => { e.currentTarget.style.background = 'var(--color-primary)'; }}
        >
          {#if checkoutLoading}
            <span class="inline-flex items-center gap-1"><LoadingSpinner /> Checking...</span>
          {:else}
            Check payment status
          {/if}
        </button>

        {#if error}
          <div class="w-full p-2 bg-tint border border-border rounded-lg text-xs text-muted leading-relaxed max-w-[280px]">{error}</div>
        {/if}

        <button
          onclick={() => { step = "paywall"; error = ""; guestSessionId = ""; }}
          class="cursor-pointer bg-transparent border-none text-muted opacity-50 transition-opacity hover:opacity-100"
          style="font-size:10px; padding:4px"
        >
          Start over
        </button>
      </div>
    </div>

  {:else if step === "payment-confirmed"}
    <!-- Payment confirmed -->
    <div class="flex-1 flex flex-col -m-4 overflow-y-auto">
      <div class="flex-1 flex flex-col items-center justify-center gap-5 bg-surface" style="padding: 32px">
        <div class="w-12 h-12 rounded-full flex items-center justify-center" style="background: rgba(45,122,79,0.1)">
          <svg class="w-6 h-6 text-signal" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
            <path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7" />
          </svg>
        </div>
        <div class="text-center">
          <h2 class="font-heading text-foreground" style="font-size:18px; font-weight:600; line-height:1.3">Payment confirmed</h2>
          <p class="text-muted mx-auto" style="font-size:11.5px; margin-top:8px; line-height:1.5; max-width:240px">
            {#if pendingPath === "guided"}
              Now let's learn how you write through a short interview.
            {:else}
              Now paste your writing samples so we can extract your voice.
            {/if}
          </p>
        </div>

        <button
          onclick={continueAfterPayment}
          class="py-2.5 px-8 text-xs font-semibold transition-all duration-200 cursor-pointer rounded-[10px]"
          style="background: var(--color-primary); color: white"
          onmouseenter={(e) => { e.currentTarget.style.background = '#243A54'; e.currentTarget.style.transform = 'translateY(-1px)'; }}
          onmouseleave={(e) => { e.currentTarget.style.background = 'var(--color-primary)'; e.currentTarget.style.transform = 'translateY(0)'; }}
        >
          Continue
        </button>
      </div>
    </div>

  {:else if step === "input-method"}
    <!-- Input method choice -->
    <div class="flex-1 flex flex-col -m-4 overflow-y-auto">
      <div class="shrink-0 text-center bg-surface" style="padding: 32px 32px 24px">
        <h2 class="font-heading text-foreground" style="font-size:18px; font-weight:600; line-height:1.3">Provide your writing</h2>
        <p class="text-muted mx-auto" style="font-size:11.5px; margin-top:8px; line-height:1.5; max-width:260px">
          The more samples you provide, the better the extraction.
        </p>
      </div>

      <div class="flex-1 flex flex-col gap-2.5 bg-surface" style="padding: 0 32px 20px">

        <!-- Upload file -->
        <button
          onclick={handleFileUpload}
          class="rounded-[10px] flex gap-3.5 items-start text-left w-full cursor-pointer bg-surface text-foreground transition-all duration-200"
          style="padding: 16px 18px; border: 1px solid var(--color-border)"
          onmouseenter={(e) => { e.currentTarget.style.borderColor = 'var(--color-secondary)'; e.currentTarget.style.boxShadow = '0 2px 8px rgba(59,107,138,0.08)'; e.currentTarget.style.transform = 'translateY(-1px)'; }}
          onmouseleave={(e) => { e.currentTarget.style.borderColor = 'var(--color-border)'; e.currentTarget.style.boxShadow = 'none'; e.currentTarget.style.transform = 'translateY(0)'; }}
        >
          <div class="shrink-0 flex items-center justify-center rounded-lg bg-tint" style="width:34px; height:34px; margin-top:1px">
            <svg class="w-[17px] h-[17px] text-secondary" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8">
              <path d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8z"/><polyline points="14 2 14 8 20 8"/><line x1="12" y1="18" x2="12" y2="12"/><polyline points="9 15 12 12 15 15"/>
            </svg>
          </div>
          <div class="flex-1 min-w-0">
            <div style="font-size:13px; font-weight:600; line-height:1.3">Upload a file</div>
            <div style="font-size:10.5px; line-height:1.5; margin-top:3px; opacity:0.65">.txt or .md with writing samples separated by blank lines</div>
          </div>
        </button>

        <!-- Paste step by step -->
        <button
          onclick={startPasteFlow}
          class="rounded-[10px] flex gap-3.5 items-start text-left w-full cursor-pointer bg-surface text-foreground transition-all duration-200"
          style="padding: 16px 18px; border: 1px solid var(--color-border)"
          onmouseenter={(e) => { e.currentTarget.style.borderColor = 'var(--color-secondary)'; e.currentTarget.style.boxShadow = '0 2px 8px rgba(59,107,138,0.08)'; e.currentTarget.style.transform = 'translateY(-1px)'; }}
          onmouseleave={(e) => { e.currentTarget.style.borderColor = 'var(--color-border)'; e.currentTarget.style.boxShadow = 'none'; e.currentTarget.style.transform = 'translateY(0)'; }}
        >
          <div class="shrink-0 flex items-center justify-center rounded-lg bg-tint" style="width:34px; height:34px; margin-top:1px">
            <svg class="w-[17px] h-[17px] text-secondary" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8">
              <rect x="8" y="2" width="8" height="4" rx="1"/><path d="M16 4h2a2 2 0 012 2v14a2 2 0 01-2 2H6a2 2 0 01-2-2V6a2 2 0 012-2h2"/><line x1="9" y1="12" x2="15" y2="12"/><line x1="9" y1="16" x2="13" y2="16"/>
            </svg>
          </div>
          <div class="flex-1 min-w-0">
            <div style="font-size:13px; font-weight:600; line-height:1.3">Paste step by step</div>
            <div style="font-size:10.5px; line-height:1.5; margin-top:3px; opacity:0.65">Guided, format-by-format: tweets, emails, long-form, and more</div>
          </div>
        </button>

        {#if error}
          <div class="w-full p-2 bg-tint border border-border rounded-lg text-xs text-muted leading-relaxed">{error}</div>
        {/if}

        <div class="mt-auto flex flex-col items-center gap-1.5" style="padding-top:14px; border-top: 1px solid var(--color-tint)">
          <button
            onclick={() => { step = "welcome"; error = ""; }}
            class="cursor-pointer bg-transparent border-none text-muted opacity-50 transition-opacity hover:opacity-100"
            style="font-size:10px; padding:4px"
          >
            &larr; Back
          </button>
        </div>
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

      <div class="p-2.5 bg-tint border border-secondary/20 rounded-lg flex flex-col gap-1.5">
        <p class="text-[10px] text-muted leading-relaxed">
          <span class="text-secondary font-medium">AI extraction</span> analyzes your actual writing — detecting sentence rhythm, vocabulary fingerprint, rhetorical moves, and format-specific adaptations.
        </p>
        <div class="flex gap-2 items-center">
          <button
            onclick={() => { pendingPath = "paste"; step = "paywall"; }}
            class="text-[10px] text-secondary font-medium cursor-pointer hover:text-foreground uppercase tracking-wide"
          >
            One-time $19
          </button>
          <span class="text-[10px] text-muted">or</span>
          <button
            onclick={() => { pendingPath = "paste"; step = "paywall"; }}
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
    <!-- Step-by-step format wizard -->
    {@const fmtStep = FORMAT_STEPS[currentFormatStep]}
    <div class="flex-1 flex flex-col gap-3 p-4 -m-4">
      <!-- Progress dots -->
      <div class="flex items-center justify-center gap-1.5">
        {#each FORMAT_STEPS as _, i}
          <div
            class="w-1.5 h-1.5 rounded-full transition-colors
              {i === currentFormatStep
                ? 'bg-accent'
                : i < currentFormatStep || formatSamples[FORMAT_STEPS[i].format]
                  ? 'bg-secondary/50'
                  : 'bg-border'}"
          ></div>
        {/each}
        <span class="text-[9px] text-muted ml-1.5">{currentFormatStep + 1}/{FORMAT_STEPS.length}</span>
      </div>

      <!-- Step header -->
      <div>
        <p class="text-xs font-medium text-foreground uppercase tracking-wide">{fmtStep.label}</p>
        <p class="text-[10px] text-muted mt-0.5">
          {canScrapeStep
            ? fmtStep.format === "twitter"
              ? "Fetch by username, or paste tweets below."
              : "Import from a blog URL, or paste articles below."
            : fmtStep.guidance}
        </p>
      </div>

      <!-- Unified input container -->
      <div class="flex-1 flex flex-col border border-border rounded-lg bg-surface overflow-hidden focus-within:border-secondary transition-colors">

        <!-- Import bar (scrapable formats only) -->
        {#if canScrapeStep}
          <div class="px-3 py-2 bg-tint/50 border-b border-border flex items-center gap-2">
            {#if fmtStep.format === "twitter"}
              <input
                type="text"
                bind:value={scrapeHandle}
                placeholder="@username or profile link"
                disabled={isScraping}
                class="flex-1 text-xs bg-transparent text-foreground placeholder-muted focus:outline-none disabled:opacity-50"
                onkeydown={(e) => { if (e.key === "Enter") handleScrapeTwitter(); }}
              />
            {:else}
              <input
                type="url"
                bind:value={scrapeUrl}
                placeholder="Blog URL or RSS feed"
                disabled={isScraping}
                class="flex-1 text-xs bg-transparent text-foreground placeholder-muted focus:outline-none disabled:opacity-50"
                onkeydown={(e) => { if (e.key === "Enter") handleScrapeBlog(); }}
              />
            {/if}
            <button
              onclick={fmtStep.format === "twitter" ? handleScrapeTwitter : handleScrapeBlog}
              disabled={isScraping || (fmtStep.format === "twitter" ? !scrapeHandle.trim() : !scrapeUrl.trim())}
              class="shrink-0 px-2.5 py-1 text-[10px] font-medium rounded-md transition-colors cursor-pointer
                {isScraping || (fmtStep.format === 'twitter' ? !scrapeHandle.trim() : !scrapeUrl.trim())
                  ? 'text-muted cursor-not-allowed opacity-50'
                  : 'bg-surface border border-border text-foreground hover:border-secondary'}"
            >
              {isScraping
                ? "Fetching..."
                : fmtStep.format === "twitter" ? "Fetch tweets" : "Fetch posts"}
            </button>
          </div>

          <!-- Scrape feedback -->
          {#if isScraping}
            <div class="px-3 py-1.5 border-b border-border flex items-center gap-1.5">
              <LoadingSpinner />
              <span class="text-[10px] text-muted">
                {fmtStep.format === "twitter"
                  ? `Fetching tweets from @${scrapeHandle.replace(/^@/, "")}...`
                  : "Fetching posts..."}
              </span>
            </div>
          {:else if scrapeError}
            <div class="px-3 py-1.5 border-b border-border">
              <p class="text-[10px] text-error">{scrapeError}</p>
            </div>
          {:else if scrapeInfoMap[fmtStep.format]}
            <div class="px-3 py-1.5 border-b border-border bg-signal/5 flex items-center gap-1.5">
              <svg class="w-3 h-3 text-signal shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2.5">
                <path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7" />
              </svg>
              <span class="text-[10px] text-signal">{scrapeInfoMap[fmtStep.format]}</span>
            </div>
          {/if}
        {/if}

        <!-- Textarea -->
        <textarea
          bind:value={currentInput}
          class="flex-1 p-3 text-xs leading-relaxed bg-transparent text-foreground resize-none placeholder-muted focus:outline-none min-h-[200px]"
          placeholder={canScrapeStep
            ? `Or paste your ${fmtStep.label.toLowerCase()} here, separated by blank lines...`
            : `Paste your ${fmtStep.label.toLowerCase()} here, separated by blank lines...`}
        ></textarea>
      </div>

      {#if currentInput.trim()}
        <p class="text-[10px] text-muted text-right">~{sampleCount(currentInput)} samples</p>
      {/if}

      <!-- Navigation buttons -->
      <div class="flex gap-2">
        <button
          onclick={prevFormatStep}
          class="px-3 py-2 text-[10px] text-muted hover:text-foreground transition-colors cursor-pointer"
        >
          Back
        </button>
        <div class="flex-1"></div>
        <button
          onclick={skipFormatStep}
          class="px-3 py-2 text-[10px] text-muted hover:text-foreground transition-colors cursor-pointer"
        >
          Skip
        </button>
        <button
          onclick={nextFormatStep}
          class="px-4 py-2 text-[11px] font-medium bg-accent text-white hover:bg-accent-hover transition-colors cursor-pointer rounded-md"
        >
          {currentFormatStep === FORMAT_STEPS.length - 1 ? "Review" : "Next"}
        </button>
      </div>
    </div>

  {:else if step === "review"}
    <!-- Review collected samples -->
    <div class="flex-1 flex flex-col gap-4 p-4 -m-4">
      <div class="text-center">
        <p class="text-sm font-medium text-foreground">Ready to extract</p>
      </div>

      <!-- Sample summary -->
      <div class="space-y-1.5">
        {#each FORMAT_STEPS as fmtItem, i}
          {@const text = formatSamples[fmtItem.format]}
          {@const count = text ? sampleCount(text) : 0}
          <div class="flex items-center justify-between px-3 py-2 bg-surface border border-border rounded-md">
            <span class="text-xs text-foreground">{fmtItem.label}</span>
            {#if count > 0}
              <div class="flex items-center gap-2">
                {#if scrapeInfoMap[fmtItem.format]}
                  <span class="text-[10px] text-muted">{scrapeInfoMap[fmtItem.format]}</span>
                  <span class="text-[10px] text-secondary/30">|</span>
                {/if}
                <span class="text-[10px] text-secondary">{count} samples</span>
                <button
                  onclick={() => goBackToFormatStep(i)}
                  class="text-[9px] text-muted hover:text-secondary transition-colors cursor-pointer"
                >
                  Edit
                </button>
              </div>
            {:else}
              <div class="flex items-center gap-2">
                <span class="text-[10px] text-muted">skipped</span>
                <button
                  onclick={() => goBackToFormatStep(i)}
                  class="text-[9px] text-muted hover:text-secondary transition-colors cursor-pointer"
                >
                  Add
                </button>
              </div>
            {/if}
          </div>
        {/each}
      </div>

      <p class="text-xs text-center {totalSamples() >= 5 ? 'text-secondary' : 'text-error'}">
        {totalSamples()} samples across {formatGroups().length} format{formatGroups().length !== 1 ? "s" : ""}
      </p>

      {#if error}
        <p class="text-[10px] text-error text-center">{error}</p>
      {/if}

      <!-- Extract button -->
      <button
        onclick={handleStartExtraction}
        disabled={totalSamples() < 5}
        class="w-full py-2.5 px-4 text-sm font-semibold tracking-wide transition-colors cursor-pointer rounded-md
          {totalSamples() < 5
            ? 'bg-surface text-muted border border-border cursor-not-allowed opacity-50'
            : 'bg-accent text-white hover:bg-accent-hover'}"
      >
        Extract Voice Profile
      </button>

      {#if totalSamples() < 5}
        <p class="text-[10px] text-muted text-center">
          Need at least 5 samples. Go back and add more to any format.
        </p>
      {/if}

      <button
        onclick={() => { step = "input-method"; }}
        class="text-[10px] text-muted hover:text-secondary transition-colors cursor-pointer text-center"
      >
        Start over
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
    <!-- Done -->
    <div class="flex-1 flex flex-col -m-4 overflow-y-auto">
      <div class="flex-1 flex flex-col items-center justify-center gap-5 bg-surface" style="padding: 32px">
        <div class="w-12 h-12 rounded-full flex items-center justify-center" style="background: rgba(45,122,79,0.1)">
          <svg class="w-6 h-6 text-signal" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
            <path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7" />
          </svg>
        </div>
        <div class="text-center">
          {#if manualProfile.trim()}
            <h2 class="font-heading text-foreground" style="font-size:18px; font-weight:600">Profile saved</h2>
            <p class="text-muted mx-auto" style="font-size:11.5px; margin-top:8px; line-height:1.5; max-width:240px">
              Good start. Noren will use your description to match your tone.
            </p>
          {:else}
            <h2 class="font-heading text-foreground" style="font-size:18px; font-weight:600">Extraction started</h2>
            <p class="text-muted mx-auto" style="font-size:11.5px; margin-top:8px; line-height:1.5; max-width:240px">
              Your voice profile is being built in the background. You can start writing right away.
            </p>
          {/if}
        </div>

        {#if manualProfile.trim()}
          <div class="w-full max-w-[260px] rounded-[10px] p-3" style="background: var(--color-tint); border: 1px solid var(--color-border)">
            <p class="text-secondary" style="font-size:10px; font-weight:500; margin-bottom:6px">Want a deeper profile?</p>
            <div class="flex flex-col gap-1">
              <div class="flex items-center gap-2">
                <span class="text-secondary" style="font-size:10px">+</span>
                <span class="text-muted" style="font-size:10px">AI extraction from your actual writing</span>
              </div>
              <div class="flex items-center gap-2">
                <span class="text-secondary" style="font-size:10px">+</span>
                <span class="text-muted" style="font-size:10px">Format-specific voice contexts</span>
              </div>
            </div>
            <div class="flex gap-2 items-center mt-2">
              <button
                onclick={() => { pendingPath = "paste"; step = "paywall"; }}
                class="text-secondary cursor-pointer hover:text-foreground uppercase tracking-wide"
                style="font-size:10px; font-weight:500"
              >
                Extraction $19
              </button>
              <span class="text-muted" style="font-size:10px">or</span>
              <button
                onclick={() => { pendingPath = "paste"; step = "paywall"; }}
                class="text-secondary cursor-pointer hover:text-foreground uppercase tracking-wide"
                style="font-size:10px; font-weight:500"
              >
                Pro $7/mo
              </button>
            </div>
          </div>
        {/if}

        <button
          onclick={onComplete}
          class="py-2.5 px-8 text-xs font-semibold transition-all duration-200 cursor-pointer rounded-[10px]"
          style="background: var(--color-primary); color: white"
          onmouseenter={(e) => { e.currentTarget.style.background = '#243A54'; e.currentTarget.style.transform = 'translateY(-1px)'; }}
          onmouseleave={(e) => { e.currentTarget.style.background = 'var(--color-primary)'; e.currentTarget.style.transform = 'translateY(0)'; }}
        >
          Start writing
        </button>
      </div>
    </div>
  {/if}
</div>
