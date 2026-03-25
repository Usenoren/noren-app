<script lang="ts">
  import { onDestroy } from "svelte";
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
    requestPasswordReset,
    requestDeleteAccount,
    confirmDeleteAccount,
    type SettingsInfo,
    type NorenProStatus,
    type SubscriptionStatus,
  } from "$lib/api/tauri";
  import { emit } from "@tauri-apps/api/event";
  import { open } from "@tauri-apps/plugin-shell";
  import { refresh as refreshSubscription, canExtract, isTrial, trialDaysLeft } from "$lib/stores/subscription.svelte";
  import { friendlyError } from "$lib/utils/errors";
  import { toastInfo } from "$lib/stores/toast.svelte";
  import LoadingSpinner from "./LoadingSpinner.svelte";

  let settings = $state<SettingsInfo | null>(null);
  let accountReady = $state(false);
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
  let cooldownInterval: ReturnType<typeof setInterval> | null = null;
  let showResendSetup = $state(false);
  let resendSetupLoading = $state(false);
  let resendSetupMessage = $state("");
  let showCouponInput = $state(false);
  let couponCode = $state("");
  let couponLoading = $state(false);
  let couponMessage = $state("");
  let showDeleteConfirm = $state(false);
  let deleteCode = $state("");
  let deleteStep = $state<"confirm" | "code">("confirm");
  let deleteLoading = $state(false);

  onDestroy(() => {
    if (cooldownInterval) clearInterval(cooldownInterval);
  });

  $effect(() => {
    loadAccount();
  });

  async function loadAccount() {
    accountReady = false;
    try {
      const s = await getSettings();

      if (s.noren_pro_logged_in) {
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
            accountReady = true;
          } catch { /* ignore */ }
          proStatus = null;
          subscription = null;
          return;
        }
      } else {
        proStatus = null;
        subscription = null;
      }
      settings = s;
    } catch (e) {
      error = friendlyError(e);
    }
    accountReady = true;
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
    if (cooldownInterval) clearInterval(cooldownInterval);
    resendCooldown = 60;
    cooldownInterval = setInterval(() => {
      resendCooldown--;
      if (resendCooldown <= 0) {
        clearInterval(cooldownInterval!);
        cooldownInterval = null;
      }
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

  async function handlePasswordReset() {
    if (!proStatus?.email) return;
    try {
      await requestPasswordReset(proStatus.email);
      toastInfo("Reset link sent to " + proStatus.email);
    } catch (e) {
      toastInfo("Reset link sent to " + proStatus.email);
    }
  }

  async function handleRequestDelete() {
    if (!proStatus?.email) return;
    deleteLoading = true;
    error = "";
    try {
      await requestDeleteAccount();
      deleteStep = "code";
    } catch (e) {
      error = friendlyError(e);
    } finally {
      deleteLoading = false;
    }
  }

  async function handleConfirmDelete() {
    deleteLoading = true;
    error = "";
    try {
      await confirmDeleteAccount(deleteCode);
      await norenProLogout();
      proStatus = null;
      subscription = null;
    } catch (e) {
      error = friendlyError(e);
    } finally {
      deleteLoading = false;
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

<div class="flex flex-col h-full overflow-y-auto animate-fade-in-up">
  <!-- View title -->
  <div class="px-6 pt-5 pb-3 shrink-0">
    <h1 class="text-heading text-foreground">Account</h1>
  </div>

  <div class="flex-1 flex flex-col gap-5 px-6 pb-6">
  {#if !accountReady}
    <div class="flex items-center justify-center h-full">
      <LoadingSpinner />
    </div>
  {:else if settings && settings.noren_pro_logged_in && proStatus}
    <!-- Logged-in state -->
    <div class="flex flex-col gap-5 max-w-sm">
      <!-- Account card -->
      <div class="card-hero p-5">
        <div class="flex items-center gap-2.5">
          <span class="text-sm font-medium text-foreground">{proStatus?.email || "Account"}</span>
          <span class="px-2 py-0.5 text-[9px] font-semibold uppercase tracking-wider rounded-full
            {subscription?.tier === 'pro' ? 'bg-accent/15 text-accent' : 'bg-border text-muted'}">
            {subscription?.tier === "pro" ? (isTrial() ? "Trial" : "Pro") : "Free"}
          </span>
        </div>
        {#if subscription?.tier === "pro"}
          <div class="divider-thread mt-3 mb-3"></div>
          <div class="flex flex-wrap gap-x-4 gap-y-1.5">
            <span class="flex items-center gap-1.5 text-[11px] text-muted"><svg class="w-3 h-3 shrink-0" fill="none" viewBox="0 0 24 24" stroke="var(--color-accent)" stroke-width="2.5"><path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7"/></svg>Inference</span>
            <span class="flex items-center gap-1.5 text-[11px] text-muted"><svg class="w-3 h-3 shrink-0" fill="none" viewBox="0 0 24 24" stroke="var(--color-accent)" stroke-width="2.5"><path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7"/></svg>Extraction</span>
            <span class="flex items-center gap-1.5 text-[11px] text-muted"><svg class="w-3 h-3 shrink-0" fill="none" viewBox="0 0 24 24" stroke="var(--color-accent)" stroke-width="2.5"><path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7"/></svg>Living profile</span>
            <span class="flex items-center gap-1.5 text-[11px] text-muted"><svg class="w-3 h-3 shrink-0" fill="none" viewBox="0 0 24 24" stroke="var(--color-accent)" stroke-width="2.5"><path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7"/></svg>Sync</span>
            <span class="flex items-center gap-1.5 text-[11px] text-muted"><svg class="w-3 h-3 shrink-0" fill="none" viewBox="0 0 24 24" stroke="var(--color-accent)" stroke-width="2.5"><path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7"/></svg>Export</span>
          </div>
        {/if}
      </div>

      {#if subscription?.tier === "pro" && subscription.active}
        <!-- Usage + Billing -->
        <div>
          <span class="text-heading block mb-2">Usage</span>
          <div class="card p-4">
            {#if proStatus.tokens_used != null && proStatus.tokens_limit != null}
              <div class="flex items-center justify-between text-[11px] text-muted mb-2">
                <span>{proStatus.tokens_used.toLocaleString()} tokens used</span>
                <span>{proStatus.tokens_limit.toLocaleString()} limit</span>
              </div>
              <div class="h-1.5 bg-border rounded-full overflow-hidden">
                <div
                  class="h-full bg-accent rounded-full transition-all"
                  style="width: {Math.min(100, (proStatus.tokens_used / proStatus.tokens_limit) * 100)}%"
                ></div>
              </div>
              <p class="text-[11px] text-muted mt-2">
                {proStatus.requests_this_month} requests this month
              </p>
              {#if subscription.current_period_end}
                <p class="text-[11px] text-muted mt-0.5">
                  Period ends {formatDate(subscription.current_period_end)}
                </p>
              {/if}
            {:else}
              <p class="text-[11px] text-muted">No usage data yet</p>
            {/if}
            <div class="divider-thread mt-3 mb-3"></div>
            <button
              onclick={handleManageBilling}
              class="btn-outline"
            >
              Manage billing
            </button>
          </div>
        </div>

        {#if isTrial()}
          {@const days = trialDaysLeft()}
          <div class="card-flat p-3 flex items-center justify-between" style="border-color: var(--color-secondary);">
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
              class="btn-primary text-[11px] py-1.5 px-3"
            >
              Upgrade
            </button>
          </div>
        {/if}

        {#if subscription.cancel_at_period_end}
          <div class="card-flat p-3" style="border-color: var(--color-warning);">
            <p class="text-xs text-warning">
              Cancels at end of period{subscription.current_period_end ? ` (${formatDate(subscription.current_period_end)})` : ""}
            </p>
          </div>
        {/if}

        <div class="divider-thread"></div>

        <!-- Security -->
        <div>
          <span class="section-label" style="display:block; margin-bottom: 8px;">Security</span>
          <div class="card-flat" style="padding: 14px 16px;">
            <div class="flex items-center justify-between">
              <div>
                <div class="text-xs font-medium text-foreground">Password</div>
                <div class="text-[11px] text-muted mt-0.5">Sends a reset link to {proStatus?.email || "your email"}</div>
              </div>
              <button
                onclick={handlePasswordReset}
                class="btn-outline"
              >
                Change password
              </button>
            </div>
          </div>
        </div>
      {:else}
        <!-- Free tier: subscribe card -->
        <div>
          <span class="section-label mb-2">Upgrade</span>
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

    <!-- Account actions -->
    <div class="max-w-sm">
      <div class="divider-thread"></div>
      <div class="pt-3">
        {#if showDeleteConfirm}
          <div class="card-flat p-4" style="border-color: var(--color-error);">
            {#if deleteStep === "confirm"}
              <p class="text-xs text-muted leading-relaxed mb-3">
                This will permanently delete your account, voice profiles, and all data. This cannot be undone.
              </p>
              <div class="flex gap-2">
                <button
                  onclick={handleRequestDelete}
                  disabled={deleteLoading}
                  class="px-3 py-1.5 text-xs font-medium text-white rounded-md cursor-pointer disabled:opacity-50 transition-colors"
                  style="background: var(--color-error);"
                >
                  {#if deleteLoading}
                    <span class="inline-flex items-center gap-1"><LoadingSpinner /> Sending...</span>
                  {:else}
                    Send verification code
                  {/if}
                </button>
                <button
                  onclick={() => { showDeleteConfirm = false; deleteStep = "confirm"; deleteCode = ""; error = ""; }}
                  class="btn-ghost"
                >
                  Cancel
                </button>
              </div>
            {:else}
              <p class="text-xs text-muted mb-3">
                Enter the code sent to <span class="text-foreground font-medium">{proStatus?.email}</span>
              </p>
              <input
                type="text"
                bind:value={deleteCode}
                class="input-field mb-3"
                style="text-align: center; font-family: 'JetBrains Mono', monospace; font-size: 16px; letter-spacing: 6px; padding: 10px;"
                placeholder="000000"
                maxlength={6}
                onkeydown={(e) => { if (e.key === "Enter" && deleteCode.trim()) handleConfirmDelete(); }}
              />
              <div class="flex gap-2">
                <button
                  onclick={handleConfirmDelete}
                  disabled={deleteLoading || !deleteCode.trim()}
                  class="px-3 py-1.5 text-xs font-medium text-white rounded-md cursor-pointer disabled:opacity-50 transition-colors"
                  style="background: var(--color-error);"
                >
                  {#if deleteLoading}
                    <span class="inline-flex items-center gap-1"><LoadingSpinner /> Deleting...</span>
                  {:else}
                    Delete permanently
                  {/if}
                </button>
                <button
                  onclick={() => { showDeleteConfirm = false; deleteStep = "confirm"; deleteCode = ""; error = ""; }}
                  class="btn-ghost"
                >
                  Cancel
                </button>
              </div>
            {/if}
          </div>
        {:else}
          <div class="flex items-center gap-1.5">
            <button
              onclick={handleProLogout}
              class="text-[11px] text-muted hover:text-error transition-colors cursor-pointer"
            >
              Sign out
            </button>
            <span class="text-[11px] text-border">&middot;</span>
            <button
              onclick={() => { showDeleteConfirm = true; }}
              class="text-[11px] text-muted hover:text-error transition-colors cursor-pointer"
            >
              Delete account
            </button>
          </div>
        {/if}
      </div>
    </div>
  {:else if pendingVerification}
    <!-- OTP Verification -->
    <div class="flex flex-col gap-4">
      <div class="card-hero">
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
    <div class="flex flex-col gap-6 max-w-sm mx-auto w-full">
      <!-- Pro pitch card -->
      <div class="card-hero p-5">
        <h3 class="text-heading text-foreground mb-4">Noren Pro</h3>
        <ul class="flex flex-col gap-2.5 text-xs text-muted">
          <li class="flex items-start gap-2.5">
            <svg class="w-3.5 h-3.5 shrink-0 mt-0.5" fill="none" viewBox="0 0 24 24" stroke="var(--color-accent)" stroke-width="2.5"><path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7"/></svg>
            Bundled inference. No API key needed.
          </li>
          <li class="flex items-start gap-2.5">
            <svg class="w-3.5 h-3.5 shrink-0 mt-0.5" fill="none" viewBox="0 0 24 24" stroke="var(--color-accent)" stroke-width="2.5"><path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7"/></svg>
            Living profile that evolves with your writing.
          </li>
          <li class="flex items-start gap-2.5">
            <svg class="w-3.5 h-3.5 shrink-0 mt-0.5" fill="none" viewBox="0 0 24 24" stroke="var(--color-accent)" stroke-width="2.5"><path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7"/></svg>
            Cloud sync across devices.
          </li>
          <li class="flex items-start gap-2.5">
            <svg class="w-3.5 h-3.5 shrink-0 mt-0.5" fill="none" viewBox="0 0 24 24" stroke="var(--color-accent)" stroke-width="2.5"><path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7"/></svg>
            Export your voice profile anytime.
          </li>
        </ul>
        <div class="divider-thread mt-4 mb-3"></div>
        <p class="text-subhead text-secondary">$7/mo <span class="text-xs text-muted font-normal" style="font-style: normal;">founding member pricing</span></p>
        {#if canExtract()}
          <p class="text-[11px] text-muted mt-2">You have extraction. Pro adds inference, living profile, sync.</p>
        {/if}
      </div>

      <!-- Auth form -->
      <div class="flex flex-col gap-4">
        <!-- Mode tabs -->
        <div class="flex gap-0 card-flat overflow-hidden" style="padding: 0;">
          <button
            onclick={() => { authMode = "login"; }}
            class="flex-1 py-2.5 text-xs font-medium cursor-pointer transition-colors
              {authMode === 'login'
                ? 'bg-primary text-white'
                : 'bg-surface text-muted hover:text-foreground'}"
            style="border-radius: 0;"
          >
            Sign In
          </button>
          <button
            onclick={() => { authMode = "signup"; }}
            class="flex-1 py-2.5 text-xs font-medium cursor-pointer transition-colors border-l border-border
              {authMode === 'signup'
                ? 'bg-primary text-white'
                : 'bg-surface text-muted hover:text-foreground'}"
            style="border-radius: 0;"
          >
            Create Account
          </button>
        </div>

        <!-- Google Sign In -->
        <button
          onclick={handleGoogleSignIn}
          disabled={googleLoading || proLoading}
          class="w-full py-2.5 text-xs font-medium bg-surface border border-border text-foreground hover:border-secondary transition-colors cursor-pointer disabled:opacity-50 rounded-lg flex items-center justify-center gap-2.5"
        >
          {#if googleLoading}
            <LoadingSpinner /> Waiting for Google...
          {:else}
            <svg class="w-4 h-4" viewBox="0 0 24 24">
              <path fill="#4285F4" d="M22.56 12.25c0-.78-.07-1.53-.2-2.25H12v4.26h5.92a5.06 5.06 0 0 1-2.2 3.32v2.77h3.57c2.08-1.92 3.28-4.74 3.28-8.1z"/>
              <path fill="#34A853" d="M12 23c2.97 0 5.46-.98 7.28-2.66l-3.57-2.77c-.98.66-2.23 1.06-3.71 1.06-2.86 0-5.29-1.93-6.16-4.53H2.18v2.84C3.99 20.53 7.7 23 12 23z"/>
              <path fill="#FBBC05" d="M5.84 14.09c-.22-.66-.35-1.36-.35-2.09s.13-1.43.35-2.09V7.07H2.18C1.43 8.55 1 10.22 1 12s.43 3.45 1.18 4.93l2.85-2.22.81-.62z"/>
              <path fill="#EA4335" d="M12 5.38c1.62 0 3.06.56 4.21 1.64l3.15-3.15C17.45 2.09 14.97 1 12 1 7.7 1 3.99 3.47 2.18 7.07l3.66 2.84c.87-2.6 3.3-4.53 6.16-4.53z"/>
            </svg>
            Continue with Google
          {/if}
        </button>

        <!-- Divider -->
        <div class="flex items-center gap-4">
          <div class="divider-thread flex-1"></div>
          <span class="text-[10px] text-muted">or</span>
          <div class="divider-thread flex-1"></div>
        </div>

        <!-- Email/Password group -->
        <div class="card-inset p-4 flex flex-col gap-3">
          <input
            type="email"
            bind:value={proEmail}
            class="input-field"
            placeholder="Email"
          />
          <input
            type="password"
            bind:value={proPassword}
            onkeydown={(e) => { if (e.key === "Enter") handleProAuth(); }}
            class="input-field"
            placeholder="Password"
          />
          <button
            onclick={handleProAuth}
            disabled={proLoading || !proEmail.trim() || !proPassword.trim()}
            class="btn-primary w-full py-2.5"
          >
            {#if proLoading}
              <span class="inline-flex items-center gap-1.5"><LoadingSpinner /> {authMode === "signup" ? "Creating..." : "Signing in..."}</span>
            {:else}
              {authMode === "signup" ? "Create account" : "Sign in"}
            {/if}
          </button>
        </div>
      </div>
    </div>

    <!-- Resend setup email -->
    <div class="max-w-sm mx-auto w-full">
      {#if showResendSetup}
        <div class="card-flat p-4 flex flex-col gap-2.5">
          <p class="text-[11px] text-muted leading-relaxed">
            Signed up on the website? Enter your email to resend the setup link.
          </p>
          <div class="flex gap-2">
            <input
              type="email"
              bind:value={proEmail}
              class="input-field flex-1"
              placeholder="Email"
            />
            <button
              onclick={handleResendSetup}
              disabled={resendSetupLoading || !proEmail.trim()}
              class="btn-primary whitespace-nowrap"
            >
              {resendSetupLoading ? "Sending..." : "Resend"}
            </button>
          </div>
          {#if resendSetupMessage}
            <p class="text-[11px] text-secondary">{resendSetupMessage}</p>
          {/if}
        </div>
      {:else}
        <button
          onclick={() => { showResendSetup = true; }}
          class="text-[11px] text-muted hover:text-foreground cursor-pointer transition-colors"
        >
          Signed up on the website? Resend setup email
        </button>
      {/if}
    </div>

    <!-- Error -->
    {#if error}
      <div class="max-w-sm mx-auto w-full card-flat p-3 text-xs text-muted leading-relaxed" style="border-color: var(--color-error);">
        {error}
      </div>
    {/if}

    <!-- Footer link -->
    <div class="mt-auto max-w-sm mx-auto w-full">
      <div class="divider-thread"></div>
      <p class="text-[11px] text-muted leading-relaxed pt-3">
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
</div>
