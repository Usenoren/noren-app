<script lang="ts">
  import {
    getSettings,
    setInferenceMode,
    norenProLogin,
    norenProSignup,
    norenProLogout,
    getNorenProUsage,
    getSubscriptionStatus,
    createCheckout,
    openBillingPortal,
    redeemCoupon,
    googleOAuthInit,
    googleOAuthPoll,
    verifyEmail,
    resendOtp,
    resendSetupEmail,
    type SettingsInfo,
    type NorenProStatus,
    type SubscriptionStatus,
  } from "$lib/api/tauri";
  import { emit } from "@tauri-apps/api/event";
  import { open } from "@tauri-apps/plugin-shell";
  import { refresh as refreshSubscription, canExtract, isTrial, trialDaysLeft } from "$lib/stores/subscription.svelte";
  import { friendlyError } from "$lib/utils/errors";
  import LoadingSpinner from "./LoadingSpinner.svelte";

  let settings = $state<SettingsInfo | null>(null);
  let proEmail = $state("");
  let proPassword = $state("");
  let proLoading = $state(false);
  let proStatus = $state<NorenProStatus | null>(null);
  let authMode = $state<"login" | "signup">("login");
  let googleLoading = $state(false);
  let subscription = $state<SubscriptionStatus | null>(null);
  let error = $state("");
  let pendingVerification = $state(false);
  let otpCode = $state("");
  let otpLoading = $state(false);
  let otpMessage = $state("");
  let resendCooldown = $state(0);
  let showResendSetup = $state(false);
  let resendSetupLoading = $state(false);
  let resendSetupMessage = $state("");
  let showCouponInput = $state(false);
  let couponCode = $state("");
  let couponLoading = $state(false);
  let couponMessage = $state("");

  $effect(() => {
    loadAccount();
  });

  async function loadAccount() {
    try {
      settings = await getSettings();

      if (settings.noren_pro_logged_in) {
        try {
          proStatus = await getNorenProUsage();
          try {
            subscription = await getSubscriptionStatus();
          } catch {
            subscription = null;
          }
        } catch {
          // Token is likely stale, auto-logout
          try {
            await norenProLogout();
            settings = await getSettings();
          } catch { /* ignore */ }
          proStatus = null;
          subscription = null;
        }
      } else {
        proStatus = null;
        subscription = null;
      }
    } catch (e) {
      error = friendlyError(e);
    }
  }

  async function handleProAuth() {
    if (!proEmail.trim() || !proPassword.trim()) return;
    const email = proEmail.trim();
    if (!email.includes("@") || !email.includes(".")) {
      error = "Enter a valid email address.";
      return;
    }
    proLoading = true;
    error = "";
    try {
      if (authMode === "signup") {
        await norenProSignup(proEmail.trim(), proPassword.trim());
        pendingVerification = true;
        otpMessage = "Check your email for a verification code.";
        proPassword = "";
        startResendCooldown();
      } else {
        await norenProLogin(proEmail.trim(), proPassword.trim());
        proEmail = "";
        proPassword = "";
        await setInferenceMode("noren_pro");
        await loadAccount();
        await refreshSubscription();
      }
    } catch (e) {
      error = friendlyError(e);
    } finally {
      proLoading = false;
    }
  }

  async function handleVerifyOtp() {
    if (!otpCode.trim()) return;
    otpLoading = true;
    error = "";
    otpMessage = "";
    try {
      await verifyEmail(otpCode.trim());
      pendingVerification = false;
      otpCode = "";
      proEmail = "";
      await setInferenceMode("noren_pro");
      await loadAccount();
      await refreshSubscription();
    } catch (e) {
      error = friendlyError(e);
    } finally {
      otpLoading = false;
    }
  }

  function startResendCooldown() {
    resendCooldown = 60;
    const interval = setInterval(() => {
      resendCooldown--;
      if (resendCooldown <= 0) clearInterval(interval);
    }, 1000);
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

  async function handleResendSetup() {
    if (!proEmail.trim()) return;
    resendSetupLoading = true;
    resendSetupMessage = "";
    try {
      const msg = await resendSetupEmail(proEmail.trim());
      resendSetupMessage = msg;
    } catch {
      resendSetupMessage = "If that email is in our system, we've sent setup instructions.";
    } finally {
      resendSetupLoading = false;
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
            await setInferenceMode("noren_pro");
            await loadAccount();
            await refreshSubscription();
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

  async function handleProLogout() {
    error = "";
    try {
      await norenProLogout();
      proStatus = null;
      subscription = null;
      await setInferenceMode("byok");
      settings = await getSettings();
    } catch (e) {
      error = friendlyError(e);
    }
  }

  async function handleUpgrade(tier: string, promoCode?: string) {
    error = "";
    try {
      const result = await createCheckout(tier, promoCode);
      if (result.checkout_url === "dev://granted") {
        await loadAccount();
        await refreshSubscription();
      } else {
        await open(result.checkout_url);
      }
    } catch (e) {
      error = friendlyError(e);
    }
  }

  async function handleApplyCoupon() {
    const code = couponCode.trim();
    if (!code) return;
    couponLoading = true;
    couponMessage = "";
    error = "";
    try {
      await redeemCoupon(code);
      showCouponInput = false;
      couponCode = "";
      await refreshSubscription();
      await loadAccount();
    } catch (e) {
      const msg = String(e);
      // Parse "status:detail" format from Rust command
      const match = msg.match(/^(\d{3}):(.+)$/);
      if (match) {
        const status = parseInt(match[1]);
        const detail = match[2];
        if (status === 404) {
          // Not a trial coupon, try as Stripe promo code
          couponMessage = "";
          await handleUpgrade("pro", code);
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

  async function handleManageBilling() {
    error = "";
    try {
      const url = await openBillingPortal();
      await open(url);
    } catch (e) {
      error = friendlyError(e);
    }
  }

  function formatDate(iso: string): string {
    return new Date(iso).toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" });
  }
</script>

<div class="flex flex-col gap-4 h-full p-4 overflow-y-auto animate-fade-in-up">
  {#if !settings}
    <div class="flex items-center justify-center h-full">
      <LoadingSpinner />
    </div>
  {:else if settings.noren_pro_logged_in && proStatus}
    <!-- Logged-in state -->
    <div class="flex flex-col gap-4">
      <!-- Account info card -->
      <div class="p-3 bg-tint border border-secondary/30 rounded-xl">
        <div class="flex items-center justify-between mb-1">
          <div class="flex items-center gap-2">
            <span class="text-xs font-medium text-foreground">{proStatus.email}</span>
            <span class="px-1.5 py-0.5 text-[9px] font-semibold uppercase tracking-wider rounded-full
              {subscription?.tier === 'pro' ? 'bg-secondary/20 text-secondary' : 'bg-border text-muted'}">
              {subscription?.tier === "pro" ? (isTrial() ? "Trial" : "Pro") : "Free"}
            </span>
          </div>
        </div>
      </div>

      {#if subscription?.tier === "pro" && subscription.active}
        <!-- Pro: usage stats -->
        {#if proStatus.tokens_used != null && proStatus.tokens_limit != null}
          <div>
            <span class="block text-xs font-medium text-muted mb-2 uppercase tracking-wide">Usage</span>
            <div class="p-3 card">
              <div class="flex items-center justify-between text-[10px] text-muted mb-1.5">
                <span>{proStatus.tokens_used.toLocaleString()} tokens used</span>
                <span>{proStatus.tokens_limit.toLocaleString()} limit</span>
              </div>
              <div class="h-1.5 bg-border rounded-full overflow-hidden">
                <div
                  class="h-full bg-secondary rounded-full transition-all"
                  style="width: {Math.min(100, (proStatus.tokens_used / proStatus.tokens_limit) * 100)}%"
                ></div>
              </div>
              <p class="text-[10px] text-muted mt-1.5">
                {proStatus.requests_this_month} requests this month
              </p>
              {#if subscription.current_period_end}
                <p class="text-[10px] text-muted mt-0.5">
                  Period ends {formatDate(subscription.current_period_end)}
                </p>
              {/if}
            </div>
          </div>
        {/if}

        {#if isTrial()}
          {@const days = trialDaysLeft()}
          <div class="p-2.5 bg-secondary/5 border border-secondary/20 rounded-xl flex items-center justify-between">
            <p class="text-xs text-secondary">
              {#if days != null && days <= 3}
                Trial ends in {days === 0 ? "less than a day" : days === 1 ? "1 day" : `${days} days`}
              {:else if subscription.trial_expires_at}
                Trial until {formatDate(subscription.trial_expires_at)}
              {:else}
                Active trial
              {/if}
            </p>
            <button
              onclick={() => handleUpgrade("pro")}
              class="px-2 py-1 text-[10px] font-medium bg-accent text-white rounded cursor-pointer hover:bg-accent-hover transition-colors"
            >
              Upgrade
            </button>
          </div>
        {/if}

        {#if subscription.cancel_at_period_end}
          <div class="p-2.5 bg-warning/5 border border-warning/20 rounded-xl">
            <p class="text-xs text-warning">
              Cancels at end of period{subscription.current_period_end ? ` (${formatDate(subscription.current_period_end)})` : ""}
            </p>
          </div>
        {/if}

        <button
          onclick={handleManageBilling}
          class="px-3 py-1.5 text-xs border border-border hover:border-secondary transition-colors cursor-pointer text-muted hover:text-foreground rounded-md self-start"
        >
          Manage billing
        </button>
      {:else}
        <!-- Free tier: subscribe card -->
        <div>
          <span class="block text-xs font-medium text-muted mb-2 uppercase tracking-wide">Upgrade</span>
          <button
            onclick={() => handleUpgrade("pro")}
            class="w-full flex items-center justify-between p-3 card hover:border-secondary cursor-pointer text-left"
          >
            <div>
              <span class="text-xs font-medium text-foreground">Subscribe to Pro</span>
              <span class="block text-[10px] text-muted mt-0.5">Bundled inference, living profile, sync, and export</span>
            </div>
            <span class="text-xs font-medium text-secondary">$7<span class="text-[10px] text-muted font-normal">/mo</span></span>
          </button>

          {#if !showCouponInput}
            <button
              onclick={() => { showCouponInput = true; couponMessage = ""; }}
              class="text-[10px] text-muted hover:text-foreground cursor-pointer transition-colors mt-1"
            >
              Have a coupon?
            </button>
          {:else}
            <div class="mt-1 flex flex-col gap-1.5">
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
                  class="px-3 py-1.5 text-[10px] font-medium bg-accent text-white hover:bg-accent-hover transition-colors cursor-pointer disabled:opacity-50 rounded-md whitespace-nowrap"
                >
                  {couponLoading ? "..." : "Apply"}
                </button>
              </div>
              {#if couponMessage}
                <p class="text-[10px] text-muted">{couponMessage}</p>
              {/if}
              <button
                onclick={() => { showCouponInput = false; couponCode = ""; couponMessage = ""; }}
                class="text-[10px] text-muted hover:text-foreground cursor-pointer transition-colors self-start"
              >
                Cancel
              </button>
            </div>
          {/if}
        </div>
      {/if}
    </div>

    <!-- Sign out -->
    <div class="mt-auto">
      <div class="divider"></div>
      <div class="pt-3">
        <button
          onclick={handleProLogout}
          class="text-[10px] text-muted hover:text-error transition-colors cursor-pointer"
        >
          Sign out
        </button>
      </div>
    </div>
  {:else if pendingVerification}
    <!-- OTP Verification -->
    <div class="flex flex-col gap-4">
      <div class="p-4 card border-secondary/30">
        <h3 class="text-sm font-semibold text-foreground mb-2 font-heading italic">Verify your email</h3>
        <p class="text-[11px] text-muted">
          We sent a verification code to <span class="font-medium text-foreground">{proEmail}</span>. Enter it below to complete your registration.
        </p>
      </div>

      <div class="flex flex-col gap-3">
        <input
          type="text"
          bind:value={otpCode}
          onkeydown={(e) => { if (e.key === "Enter") handleVerifyOtp(); }}
          class="px-3 py-2 text-sm text-center tracking-[0.3em] border border-border bg-surface text-foreground rounded-md focus:outline-none focus:border-secondary"
          placeholder="000000"
          maxlength={6}
          autocomplete="one-time-code"
        />
        <button
          onclick={handleVerifyOtp}
          disabled={otpLoading || !otpCode.trim()}
          class="w-full py-2 text-xs font-medium bg-accent text-white hover:bg-accent-hover transition-colors cursor-pointer disabled:opacity-50 rounded-md"
        >
          {#if otpLoading}
            <span class="inline-flex items-center gap-1"><LoadingSpinner /> Verifying...</span>
          {:else}
            Verify email
          {/if}
        </button>
      </div>

      {#if otpMessage}
        <p class="text-[10px] text-secondary">{otpMessage}</p>
      {/if}

      {#if error}
        <div class="p-2 bg-tint border border-border rounded-xl text-xs text-muted leading-relaxed">
          {error}
        </div>
      {/if}

      <div class="flex items-center justify-between">
        <button
          onclick={handleResendOtp}
          disabled={resendCooldown > 0}
          class="text-[10px] transition-colors cursor-pointer {resendCooldown > 0 ? 'text-muted/50' : 'text-muted hover:text-foreground underline'}"
        >
          {resendCooldown > 0 ? `Resend in ${resendCooldown}s` : "Resend code"}
        </button>
        <button
          onclick={() => { pendingVerification = false; otpCode = ""; error = ""; otpMessage = ""; }}
          class="text-[10px] text-muted hover:text-foreground transition-colors cursor-pointer"
        >
          Back
        </button>
      </div>
    </div>
  {:else}
    <!-- Not logged in: pitch + auth -->
    <div class="flex flex-col gap-4">
      <!-- Pro pitch card -->
      <div class="p-4 card border-secondary/30">
        <h3 class="text-sm font-semibold text-foreground mb-2 font-heading italic">Noren Pro</h3>
        <ul class="flex flex-col gap-1.5 text-[11px] text-muted">
          <li class="flex items-start gap-2">
            <svg class="w-3 h-3 text-secondary shrink-0 mt-0.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2.5"><path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7"/></svg>
            Bundled inference. No API key needed.
          </li>
          <li class="flex items-start gap-2">
            <svg class="w-3 h-3 text-secondary shrink-0 mt-0.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2.5"><path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7"/></svg>
            Living profile that evolves with your writing.
          </li>
          <li class="flex items-start gap-2">
            <svg class="w-3 h-3 text-secondary shrink-0 mt-0.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2.5"><path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7"/></svg>
            Cloud sync across devices.
          </li>
          <li class="flex items-start gap-2">
            <svg class="w-3 h-3 text-secondary shrink-0 mt-0.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2.5"><path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7"/></svg>
            Export your voice profile anytime.
          </li>
        </ul>
        <p class="text-xs font-medium text-secondary mt-3">$7/mo <span class="text-[10px] text-muted font-normal">founding member pricing</span></p>
        {#if canExtract()}
          <p class="text-[10px] text-muted mt-1.5">You have extraction. Pro adds inference, living profile, sync.</p>
        {/if}
      </div>

      <!-- Auth form -->
      <div class="flex flex-col gap-3">
        <div class="flex gap-1">
          <button
            onclick={() => { authMode = "login"; }}
            class="flex-1 px-2 py-1 text-[10px] uppercase tracking-wide cursor-pointer rounded-md
              {authMode === 'login'
                ? 'bg-secondary text-white font-medium'
                : 'bg-surface text-muted border border-border'}"
          >
            Sign in
          </button>
          <button
            onclick={() => { authMode = "signup"; }}
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
          disabled={googleLoading || proLoading}
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

        <div class="relative">
          <div class="absolute inset-0 flex items-center">
            <div class="w-full border-t border-border"></div>
          </div>
          <div class="relative flex justify-center text-[10px]">
            <span class="px-2 bg-background text-muted">or</span>
          </div>
        </div>

        <input
          type="email"
          bind:value={proEmail}
          class="px-3 py-1.5 text-xs border border-border bg-surface text-foreground rounded-md focus:outline-none focus:border-secondary"
          placeholder="Email"
        />
        <input
          type="password"
          bind:value={proPassword}
          onkeydown={(e) => { if (e.key === "Enter") handleProAuth(); }}
          class="px-3 py-1.5 text-xs border border-border bg-surface text-foreground rounded-md focus:outline-none focus:border-secondary"
          placeholder="Password"
        />
        <button
          onclick={handleProAuth}
          disabled={proLoading || !proEmail.trim() || !proPassword.trim()}
          class="w-full py-2 text-xs font-medium bg-accent text-white hover:bg-accent-hover transition-colors cursor-pointer disabled:opacity-50 rounded-md"
        >
          {#if proLoading}
            <span class="inline-flex items-center gap-1"><LoadingSpinner /> {authMode === "signup" ? "Creating..." : "Signing in..."}</span>
          {:else}
            {authMode === "signup" ? "Create account" : "Sign in"}
          {/if}
        </button>
      </div>
    </div>

    <!-- Resend setup email -->
    {#if showResendSetup}
      <div class="p-3 bg-tint border border-secondary/20 rounded-xl flex flex-col gap-2">
        <p class="text-[10px] text-muted leading-relaxed">
          Signed up on the website? Enter your email to resend the setup link.
        </p>
        <div class="flex gap-1.5">
          <input
            type="email"
            bind:value={proEmail}
            class="flex-1 px-2.5 py-1.5 text-xs border border-border bg-surface text-foreground rounded-md focus:outline-none focus:border-secondary"
            placeholder="Email"
          />
          <button
            onclick={handleResendSetup}
            disabled={resendSetupLoading || !proEmail.trim()}
            class="px-3 py-1.5 text-[10px] font-medium bg-accent text-white hover:bg-accent-hover transition-colors cursor-pointer disabled:opacity-50 rounded-md whitespace-nowrap"
          >
            {resendSetupLoading ? "Sending..." : "Resend"}
          </button>
        </div>
        {#if resendSetupMessage}
          <p class="text-[10px] text-secondary">{resendSetupMessage}</p>
        {/if}
      </div>
    {:else}
      <button
        onclick={() => { showResendSetup = true; }}
        class="text-[10px] text-muted hover:text-foreground cursor-pointer"
      >
        Signed up on the website? Resend setup email
      </button>
    {/if}

    <!-- Error -->
    {#if error}
      <div class="p-2 bg-tint border border-border rounded-xl text-xs text-muted leading-relaxed">
        {error}
      </div>
    {/if}

    <!-- Footer link -->
    <div class="mt-auto">
      <div class="divider"></div>
      <p class="text-[10px] text-muted leading-relaxed pt-3">
        Already using BYOK?
        <button
          onclick={() => emit("navigate", "settings")}
          class="text-primary hover:text-foreground cursor-pointer underline"
        >
          Configure your API key in Settings.
        </button>
      </p>
    </div>
  {/if}
</div>
