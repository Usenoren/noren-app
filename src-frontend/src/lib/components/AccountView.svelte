<script lang="ts">
  import { onDestroy, onMount } from "svelte";
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
  import { friendlyError, isAuthSessionError } from "$lib/utils/errors";
  import { toastInfo, toastWarning } from "$lib/stores/toast.svelte";
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
  let deleteCode = $state("");
  let deleteStep = $state<"confirm" | "code">("confirm");
  let deleteLoading = $state(false);
  let dangerOpen = $state(false);
  let usageAnimated = $state(false);

  // Tier helpers
  let hasInference = $derived((proStatus?.generations_limit ?? 0) > 0);
  let effectivelyPro = $derived(subscription?.tier === "pro");
  let effectivelyFree = $derived(!effectivelyPro);

  onDestroy(() => {
    if (cooldownInterval) clearInterval(cooldownInterval);
  });

  onMount(() => {
    setTimeout(() => { usageAnimated = true; }, 400);
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
        } catch (e) {
          if (isAuthSessionError(e)) {
            try {
              await norenProLogout();
              settings = await getSettings();
              accountReady = true;
            } catch { /* ignore */ }
            proStatus = null;
            subscription = null;
            return;
          }
          error = friendlyError(e);
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
    setTimeout(() => { usageAnimated = true; }, 400);

    // Soft warning at 80% usage (once per session)
    if (proStatus?.generations_used != null && proStatus?.generations_limit != null && proStatus.generations_limit > 0) {
      const pct = proStatus.generations_used / proStatus.generations_limit;
      const warningKey = `gen-warning-${new Date().toISOString().slice(0, 7)}`;
      if (pct >= 0.8 && pct < 1 && !sessionStorage.getItem(warningKey)) {
        sessionStorage.setItem(warningKey, "1");
        toastWarning(`You've used ${proStatus.generations_used} of your ${proStatus.generations_limit} monthly generations.`);
      }
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
        } catch {
          // Silently retry — transient errors (rate limits, network blips)
          // should not abort the polling loop
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
      const match = msg.match(/^(\d{3}):(.+)$/);
      if (match) {
        const status = parseInt(match[1]);
        const detail = match[2];
        if (status === 404) {
          couponMessage = "";
          await handleUpgrade("pro", code);
        } else {
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

  function closeDangerZone() {
    dangerOpen = false;
    deleteStep = "confirm";
    deleteCode = "";
    error = "";
  }
</script>

<div class="av-page animate-fade-in-up">
  <h1 class="text-heading" style="margin-bottom: 6px;">Account</h1>

  {#if !accountReady}
    <div class="flex items-center justify-center" style="min-height: 200px;">
      <LoadingSpinner />
    </div>
  {:else if settings && settings.noren_pro_logged_in && proStatus}
    <!-- ═══ LOGGED IN ═══ -->
    <div class="av-sections av-stagger">

      <!-- Identity bar -->
      <div class="av-identity">
        <div class="av-identity-left">
          <span class="av-email">{proStatus?.email || "Account"}</span>
          <span class="av-badge {effectivelyPro ? (isTrial() ? 'av-badge-trial' : 'av-badge-pro') : 'av-badge-free'}">
            {effectivelyPro ? (isTrial() ? "Trial" : "Pro") : "Free"}
          </span>
        </div>
        <button class="av-signout" onclick={handleProLogout}>Sign out</button>
      </div>

      <!-- Trial callout -->
      {#if effectivelyPro && subscription?.active && isTrial()}
        {@const days = trialDaysLeft()}
        <div class="av-trial-callout card-flat">
          <span class="av-trial-text">
            {#if days != null && days <= 3}
              Trial ends in {days === 0 ? "less than a day" : days === 1 ? "1 day" : `${days} days`}
            {:else if subscription.trial_expires_at}
              Trial until {formatDate(subscription.trial_expires_at)}
            {:else}
              Active trial
            {/if}
          </span>
          <button class="btn-primary av-btn-sm" onclick={() => handleUpgrade("pro")}>Upgrade</button>
        </div>
      {/if}

      <!-- Cancellation notice -->
      {#if subscription?.cancel_at_period_end}
        <div class="av-cancel-notice card-flat">
          Cancels at end of period{subscription.current_period_end ? ` (${formatDate(subscription.current_period_end)})` : ""}
        </div>
      {/if}

      {#if effectivelyPro}
        <!-- Usage + Features grid -->
        <div class="av-grid">
          <!-- Usage -->
          <div class="card-hero av-card-pad">
            <span class="section-label" style="display:block; margin-bottom: 14px;">Usage</span>
            {#if proStatus.generations_used != null && proStatus.generations_limit != null}
              <div class="av-usage-head">
                <span>{proStatus.generations_used} / {proStatus.generations_limit} generations</span>
                <span>Resets {new Date(new Date().getFullYear(), new Date().getMonth() + 1, 1).toLocaleDateString('en-US', { month: 'short', day: 'numeric', year: 'numeric' })}</span>
              </div>
              <div class="av-bar">
                <div
                  class="av-bar-fill"
                  class:go={usageAnimated}
                  style="--pct: {Math.min(100, (proStatus.generations_used / (proStatus.generations_limit || 1)) * 100)}%"
                ></div>
              </div>
              <div class="av-usage-meta">
                <span>Chat and autocomplete don't count toward your limit.</span>
              </div>
              {#if proStatus.generations_used / (proStatus.generations_limit || 1) >= 0.8}
                <div class="av-usage-warning">
                  {proStatus.generations_used >= proStatus.generations_limit
                    ? "Generation limit reached. Resets next month."
                    : `${proStatus.generations_limit - proStatus.generations_used} generations remaining this month.`}
                </div>
              {/if}
            {:else}
              <p class="text-[11px] text-muted">No usage data yet</p>
            {/if}
          </div>

          <!-- Features -->
          <div class="card av-card-pad">
            <span class="section-label" style="display:block; margin-bottom: 10px;">Included</span>
            <div class="av-features">
              <span class="av-feat"><svg class="av-check-icon" fill="none" viewBox="0 0 24 24" stroke="var(--color-accent)" stroke-width="2.5"><path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7"/></svg>Inference</span>
              <span class="av-feat"><svg class="av-check-icon" fill="none" viewBox="0 0 24 24" stroke="var(--color-accent)" stroke-width="2.5"><path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7"/></svg>Extraction</span>
              <span class="av-feat"><svg class="av-check-icon" fill="none" viewBox="0 0 24 24" stroke="var(--color-accent)" stroke-width="2.5"><path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7"/></svg>Living profile</span>
              <span class="av-feat"><svg class="av-check-icon" fill="none" viewBox="0 0 24 24" stroke="var(--color-accent)" stroke-width="2.5"><path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7"/></svg>Sync</span>
              <span class="av-feat"><svg class="av-check-icon" fill="none" viewBox="0 0 24 24" stroke="var(--color-accent)" stroke-width="2.5"><path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7"/></svg>Export</span>
            </div>
          </div>
        </div>

        <div class="divider-thread"></div>

        <!-- Billing row -->
        <div class="card-flat">
          <div class="av-setting-row">
            <div>
              <div class="av-setting-label">Billing</div>
              <div class="av-setting-desc">Manage payment method, invoices, and plan</div>
            </div>
            <button class="btn-outline" onclick={handleManageBilling}>Manage</button>
          </div>
        </div>

        <!-- Password row -->
        <div class="card-flat">
          <div class="av-setting-row">
            <div>
              <div class="av-setting-label">Password</div>
              <div class="av-setting-desc">Send a reset link to {proStatus?.email || "your email"}</div>
            </div>
            <button class="btn-outline" onclick={handlePasswordReset}>Reset</button>
          </div>
        </div>

      {:else if hasInference}
        <!-- Free tier with generations: usage only -->
        <div class="card-hero av-card-pad">
          <span class="section-label" style="display:block; margin-bottom: 14px;">Usage</span>
          {#if proStatus.generations_used != null && proStatus.generations_limit != null}
            <div class="av-usage-head">
              <span>{proStatus.generations_used} / {proStatus.generations_limit} generations</span>
              <span>Resets {new Date(new Date().getFullYear(), new Date().getMonth() + 1, 1).toLocaleDateString('en-US', { month: 'short', day: 'numeric', year: 'numeric' })}</span>
            </div>
            <div class="av-bar">
              <div
                class="av-bar-fill"
                class:go={usageAnimated}
                style="--pct: {Math.min(100, (proStatus.generations_used / (proStatus.generations_limit || 1)) * 100)}%"
              ></div>
            </div>
            <div class="av-usage-meta">
              <span>Chat and autocomplete don't count toward your limit.</span>
            </div>
          {:else}
            <p class="text-[11px] text-muted">No usage data yet</p>
          {/if}
        </div>
      {/if}

      {#if effectivelyFree}
        <!-- Free tier: upgrade card -->
        <button class="card av-upgrade-card" onclick={() => handleUpgrade("pro")}>
          <div>
            <div class="av-upgrade-title">Subscribe to Pro</div>
            <div class="av-upgrade-sub">Bundled inference, living profile, sync, and export</div>
          </div>
          <div class="av-upgrade-price">$7<span class="av-upgrade-per">/mo</span></div>
        </button>

        <div class="av-coupon">
          {#if !showCouponInput}
            <button class="av-coupon-toggle" onclick={() => { showCouponInput = true; couponMessage = ""; }}>Have a coupon?</button>
          {:else}
            <div class="av-coupon-form">
              <input
                type="text"
                bind:value={couponCode}
                onkeydown={(e) => { if (e.key === "Enter") handleApplyCoupon(); }}
                class="input-field av-coupon-input"
                placeholder="Coupon or promo code"
              />
              <button
                class="btn-primary av-btn-sm"
                onclick={handleApplyCoupon}
                disabled={couponLoading || !couponCode.trim()}
              >
                {couponLoading ? "..." : "Apply"}
              </button>
            </div>
            {#if couponMessage}
              <p class="text-[10px] text-muted mt-1">{couponMessage}</p>
            {/if}
            <button class="av-coupon-toggle" style="margin-top: 4px;" onclick={() => { showCouponInput = false; couponCode = ""; couponMessage = ""; }}>Cancel</button>
          {/if}
        </div>

        <div class="divider-thread"></div>
      {/if}

      <!-- Error -->
      {#if error}
        <div class="av-error">{error}</div>
      {/if}

      <!-- Danger zone -->
      <div class="av-danger-zone" class:open={dangerOpen}>
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="av-danger-header" onclick={() => { dangerOpen = !dangerOpen; }}>
          <span class="av-danger-label">Delete account</span>
          <span class="av-danger-arrow">&#9654;</span>
        </div>
        {#if dangerOpen}
          <div class="av-danger-body">
            <div class="card-flat av-danger-confirm">
              {#if deleteStep === "confirm"}
                <p class="av-danger-text">
                  This permanently deletes your account, voice profiles, and all data. This cannot be undone.
                </p>
                <div class="av-danger-actions">
                  <button
                    class="av-btn-danger"
                    onclick={handleRequestDelete}
                    disabled={deleteLoading}
                  >
                    {#if deleteLoading}
                      <span class="inline-flex items-center gap-1"><LoadingSpinner /> Sending...</span>
                    {:else}
                      Send verification code
                    {/if}
                  </button>
                  <button class="btn-outline" onclick={closeDangerZone}>Cancel</button>
                </div>
              {:else}
                <p class="av-danger-text">
                  Enter the code sent to <span class="font-medium text-foreground">{proStatus?.email}</span>
                </p>
                <input
                  type="text"
                  bind:value={deleteCode}
                  class="input-field av-otp-input"
                  style="margin-bottom: 14px;"
                  placeholder="000000"
                  maxlength={6}
                  onkeydown={(e) => { if (e.key === "Enter" && deleteCode.trim()) handleConfirmDelete(); }}
                />
                <div class="av-danger-actions">
                  <button
                    class="av-btn-danger"
                    onclick={handleConfirmDelete}
                    disabled={deleteLoading || !deleteCode.trim()}
                  >
                    {#if deleteLoading}
                      <span class="inline-flex items-center gap-1"><LoadingSpinner /> Deleting...</span>
                    {:else}
                      Delete permanently
                    {/if}
                  </button>
                  <button class="btn-outline" onclick={closeDangerZone}>Cancel</button>
                </div>
              {/if}
            </div>
          </div>
        {/if}
      </div>
    </div>

  {:else if pendingVerification}
    <!-- ═══ OTP VERIFICATION ═══ -->
    <div class="av-sections av-stagger">
      <div class="card-hero av-otp-card">
        <h3 class="text-subhead text-foreground mb-2 font-heading italic">Verify your email</h3>
        <p class="av-otp-desc">
          We sent a verification code to <span class="font-medium text-foreground">{proEmail}</span>. Enter it below to complete registration.
        </p>

        <div class="av-otp-form">
          <input
            type="text"
            bind:value={otpCode}
            onkeydown={(e) => { if (e.key === "Enter") handleVerifyOtp(); }}
            class="input-field av-otp-input"
            placeholder="000000"
            maxlength={6}
            autocomplete="one-time-code"
          />
          <button
            class="btn-primary"
            style="width: 100%; padding: 11px;"
            onclick={handleVerifyOtp}
            disabled={otpLoading || !otpCode.trim()}
          >
            {#if otpLoading}
              <span class="inline-flex items-center gap-1"><LoadingSpinner /> Verifying...</span>
            {:else}
              Verify email
            {/if}
          </button>
        </div>

        {#if otpMessage}
          <p class="text-[10px] text-secondary mt-2">{otpMessage}</p>
        {/if}

        <div class="av-otp-actions">
          <button
            class="av-resend-btn"
            onclick={handleResendOtp}
            disabled={resendCooldown > 0}
          >
            {resendCooldown > 0 ? `Resend in ${resendCooldown}s` : "Resend code"}
          </button>
          <button
            class="av-text-btn"
            onclick={() => { pendingVerification = false; otpCode = ""; error = ""; otpMessage = ""; }}
          >
            Back
          </button>
        </div>
      </div>

      {#if error}
        <div class="av-error">{error}</div>
      {/if}
    </div>

  {:else}
    <!-- ═══ NOT LOGGED IN ═══ -->
    <div class="av-sections av-stagger">
      <div class="av-auth-split">
        <!-- Pro pitch -->
        <div class="card-hero av-pitch">
          <h3 class="text-heading text-foreground" style="margin-bottom: 18px;">Noren Pro</h3>
          <ul class="av-pitch-list">
            <li class="av-pitch-item">
              <svg class="av-pitch-check" fill="none" viewBox="0 0 24 24" stroke="var(--color-accent)" stroke-width="2.5"><path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7"/></svg>
              Bundled inference. No API key needed.
            </li>
            <li class="av-pitch-item">
              <svg class="av-pitch-check" fill="none" viewBox="0 0 24 24" stroke="var(--color-accent)" stroke-width="2.5"><path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7"/></svg>
              Living profile that evolves with your writing.
            </li>
            <li class="av-pitch-item">
              <svg class="av-pitch-check" fill="none" viewBox="0 0 24 24" stroke="var(--color-accent)" stroke-width="2.5"><path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7"/></svg>
              Cloud sync across devices.
            </li>
            <li class="av-pitch-item">
              <svg class="av-pitch-check" fill="none" viewBox="0 0 24 24" stroke="var(--color-accent)" stroke-width="2.5"><path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7"/></svg>
              Export your voice profile anytime.
            </li>
          </ul>
          <div class="divider-thread" style="margin-top: 20px; margin-bottom: 14px;"></div>
          <p class="text-subhead text-secondary">$7/mo <span class="text-xs text-muted font-normal" style="font-style: normal;">founding member pricing</span></p>
          {#if canExtract()}
            <p class="text-[11px] text-muted mt-2">You have extraction. Pro adds inference, living profile, sync.</p>
          {/if}
        </div>

        <!-- Auth form -->
        <div class="card av-auth-card">
          <div class="av-auth-tabs">
            <button
              class="av-auth-tab"
              class:on={authMode === "login"}
              onclick={() => { authMode = "login"; }}
            >Sign In</button>
            <button
              class="av-auth-tab"
              class:on={authMode === "signup"}
              onclick={() => { authMode = "signup"; }}
            >Create Account</button>
          </div>

          <div class="av-auth-body">
            <!-- Google -->
            <button
              class="av-google-btn"
              onclick={handleGoogleSignIn}
              disabled={googleLoading || proLoading}
            >
              {#if googleLoading}
                <LoadingSpinner /> Waiting for Google...
              {:else}
                <svg class="av-google-icon" viewBox="0 0 24 24">
                  <path fill="#4285F4" d="M22.56 12.25c0-.78-.07-1.53-.2-2.25H12v4.26h5.92a5.06 5.06 0 0 1-2.2 3.32v2.77h3.57c2.08-1.92 3.28-4.74 3.28-8.1z"/>
                  <path fill="#34A853" d="M12 23c2.97 0 5.46-.98 7.28-2.66l-3.57-2.77c-.98.66-2.23 1.06-3.71 1.06-2.86 0-5.29-1.93-6.16-4.53H2.18v2.84C3.99 20.53 7.7 23 12 23z"/>
                  <path fill="#FBBC05" d="M5.84 14.09c-.22-.66-.35-1.36-.35-2.09s.13-1.43.35-2.09V7.07H2.18C1.43 8.55 1 10.22 1 12s.43 3.45 1.18 4.93l2.85-2.22.81-.62z"/>
                  <path fill="#EA4335" d="M12 5.38c1.62 0 3.06.56 4.21 1.64l3.15-3.15C17.45 2.09 14.97 1 12 1 7.7 1 3.99 3.47 2.18 7.07l3.66 2.84c.87-2.6 3.3-4.53 6.16-4.53z"/>
                </svg>
                Continue with Google
              {/if}
            </button>

            <!-- Or divider -->
            <div class="av-or">
              <div class="divider-thread" style="flex: 1;"></div>
              <span class="text-[10px] text-muted" style="text-transform: uppercase; letter-spacing: 0.08em;">or</span>
              <div class="divider-thread" style="flex: 1;"></div>
            </div>

            <!-- Email/Password -->
            <div class="card-inset av-form-inset">
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
                class="btn-primary"
                style="width: 100%; padding: 11px;"
                onclick={handleProAuth}
                disabled={proLoading || !proEmail.trim() || !proPassword.trim()}
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
      </div>

      <!-- Footer links -->
      <div class="av-auth-bottom">
        <div class="divider-thread" style="margin-bottom: 12px;"></div>
        {#if showResendSetup}
          <div class="card-flat" style="padding: 14px 16px;">
            <p class="text-[11px] text-muted leading-relaxed mb-2">
              Enter your email to resend the setup link.
            </p>
            <div class="flex gap-2">
              <input type="email" bind:value={proEmail} class="input-field flex-1" placeholder="Email" />
              <button class="btn-primary" style="white-space: nowrap;" onclick={handleResendSetup} disabled={resendSetupLoading || !proEmail.trim()}>
                {resendSetupLoading ? "Sending..." : "Resend"}
              </button>
            </div>
            {#if resendSetupMessage}
              <p class="text-[11px] text-secondary mt-1">{resendSetupMessage}</p>
            {/if}
          </div>
        {:else}
          <p class="text-[11px] text-muted leading-relaxed">
            Signed up on the website?
            <button class="av-link-btn" onclick={() => { showResendSetup = true; }}>Resend setup email.</button>
          </p>
        {/if}
        <p class="text-[11px] text-muted leading-relaxed" style="margin-top: 4px;">
          Already using BYOK?
          <button class="av-link-btn" onclick={() => emit("navigate", "settings")}>Configure your API key in Settings.</button>
        </p>
      </div>

      <!-- Error -->
      {#if error}
        <div class="av-error">{error}</div>
      {/if}
    </div>
  {/if}

</div>

<style>
  /* ── Page container ── */
  .av-page {
    padding: clamp(20px, 4vw, 40px);
    padding-top: clamp(16px, 3vw, 28px);
    max-width: 680px;
    height: 100%;
    overflow-y: auto;
  }

  /* ── Sections with gap ── */
  .av-sections {
    display: flex;
    flex-direction: column;
    gap: clamp(16px, 2.5vw, 24px);
  }

  /* ── Staggered entry ── */
  .av-stagger > :global(*) {
    animation: av-enter 0.4s cubic-bezier(0.16, 1, 0.3, 1) both;
  }
  .av-stagger > :global(*:nth-child(1)) { animation-delay: 0ms; }
  .av-stagger > :global(*:nth-child(2)) { animation-delay: 60ms; }
  .av-stagger > :global(*:nth-child(3)) { animation-delay: 120ms; }
  .av-stagger > :global(*:nth-child(4)) { animation-delay: 180ms; }
  .av-stagger > :global(*:nth-child(5)) { animation-delay: 240ms; }
  .av-stagger > :global(*:nth-child(6)) { animation-delay: 300ms; }
  .av-stagger > :global(*:nth-child(7)) { animation-delay: 360ms; }
  .av-stagger > :global(*:nth-child(8)) { animation-delay: 420ms; }

  @keyframes av-enter {
    from { opacity: 0; transform: translateY(10px); }
    to { opacity: 1; transform: translateY(0); }
  }

  /* ── Identity bar ── */
  .av-identity {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    flex-wrap: wrap;
  }
  .av-identity-left {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }
  .av-email { font-size: 15px; font-weight: 600; }
  .av-badge {
    display: inline-flex; align-items: center;
    padding: 3px 10px; font-size: 9px; font-weight: 700;
    text-transform: uppercase; letter-spacing: 0.08em; border-radius: 100px;
  }
  .av-badge-pro { background: rgba(122,51,64,0.12); color: var(--color-accent); }
  .av-badge-free { background: var(--color-border); color: var(--color-muted); }
  .av-badge-trial { background: rgba(59,107,138,0.12); color: var(--color-secondary); }
  .av-signout {
    font-size: 11px; font-family: inherit; color: var(--color-muted);
    background: none; border: none; cursor: pointer;
    padding: 4px 8px; border-radius: 6px;
    transition: background 0.15s, color 0.15s;
  }
  .av-signout:hover { background: rgba(43,39,37,0.05); color: var(--color-foreground); }

  /* ── Two-column grid ── */
  .av-grid {
    display: grid;
    gap: clamp(14px, 2.5vw, 20px);
    grid-template-columns: 1fr;
  }
  @media (min-width: 540px) {
    .av-grid { grid-template-columns: 1fr 1fr; }
  }

  .av-card-pad { padding: clamp(16px, 3vw, 22px); }

  /* ── Features ── */
  .av-features { display: flex; flex-wrap: wrap; gap: 6px; }
  .av-feat {
    display: inline-flex; align-items: center; gap: 5px;
    padding: 4px 10px; font-size: 11px; color: var(--color-muted);
    background: rgba(30,49,72,0.03); border-radius: 6px;
    transition: background 0.15s, color 0.15s;
  }
  .av-feat:hover { background: rgba(30,49,72,0.06); color: var(--color-foreground); }
  .av-check-icon { width: 11px; height: 11px; flex-shrink: 0; }

  /* ── Usage bar ── */
  .av-usage-head {
    display: flex; justify-content: space-between; align-items: baseline;
    font-size: 11px; color: var(--color-muted); margin-bottom: 10px;
  }
  .av-bar {
    height: 6px; background: var(--color-border);
    border-radius: 100px; overflow: hidden;
  }
  .av-bar-fill {
    height: 100%; border-radius: 100px;
    background: linear-gradient(90deg, var(--color-accent), var(--color-secondary));
    width: 0; transition: width 1s cubic-bezier(0.16, 1, 0.3, 1);
  }
  .av-bar-fill.go { width: var(--pct); }
  .av-usage-meta {
    font-size: 11px; color: var(--color-muted); margin-top: 10px;
    display: flex; flex-direction: column; gap: 3px;
  }
  .av-usage-warning {
    font-size: 11px; color: var(--color-warning); font-weight: 500;
    margin-top: 6px;
  }

  /* ── Setting rows ── */
  .av-setting-row {
    display: flex; align-items: center; justify-content: space-between;
    padding: clamp(12px, 2vw, 16px) clamp(14px, 2.5vw, 20px);
    gap: 12px; flex-wrap: wrap;
  }
  .av-setting-label { font-size: 13px; font-weight: 600; }
  .av-setting-desc { font-size: 11px; color: var(--color-muted); margin-top: 2px; }

  /* ── Trial callout ── */
  .av-trial-callout {
    display: flex; align-items: center; justify-content: space-between;
    padding: 14px clamp(14px, 2.5vw, 20px);
    border-color: var(--color-secondary); gap: 12px; flex-wrap: wrap;
  }
  .av-trial-text { font-size: 13px; color: var(--color-secondary); font-weight: 500; }

  /* ── Cancel notice ── */
  .av-cancel-notice {
    padding: 12px clamp(14px, 2.5vw, 20px);
    border-color: var(--color-warning);
    font-size: 12px; color: var(--color-warning); font-weight: 500;
  }

  /* ── Upgrade card ── */
  .av-upgrade-card {
    display: flex; align-items: center; justify-content: space-between;
    padding: clamp(14px, 2.5vw, 20px); gap: 16px; cursor: pointer;
    text-align: left; width: 100%;
    transition: box-shadow 0.2s, transform 0.2s, border-color 0.2s;
  }
  .av-upgrade-card:hover {
    box-shadow: var(--shadow-card-hover); border-color: var(--color-secondary);
    transform: translateY(-1px);
  }
  .av-upgrade-title { font-size: 14px; font-weight: 600; }
  .av-upgrade-sub { font-size: 11px; color: var(--color-muted); margin-top: 3px; }
  .av-upgrade-price { font-size: 15px; font-weight: 600; color: var(--color-secondary); white-space: nowrap; }
  .av-upgrade-per { font-size: 11px; font-weight: 400; color: var(--color-muted); }

  /* ── Coupon ── */
  .av-coupon { margin-top: -8px; }
  .av-coupon-toggle {
    font-size: 11px; font-family: inherit; color: var(--color-muted);
    background: none; border: none; cursor: pointer; transition: color 0.15s;
  }
  .av-coupon-toggle:hover { color: var(--color-foreground); }
  .av-coupon-form {
    display: flex; gap: 8px;
    animation: av-enter 0.25s cubic-bezier(0.16, 1, 0.3, 1) both;
  }
  .av-coupon-input { flex: 1; min-width: 0; padding: 8px 12px !important; font-size: 12px !important; }

  /* ── Small button ── */
  .av-btn-sm { padding: 7px 16px !important; font-size: 12px !important; }

  /* ── Danger zone ── */
  .av-danger-zone { margin-top: 8px; }
  .av-danger-header {
    display: flex; align-items: center; justify-content: space-between;
    padding: clamp(12px, 2vw, 16px) clamp(14px, 2.5vw, 20px);
    cursor: pointer; border-radius: 12px;
    transition: background 0.15s;
  }
  .av-danger-header:hover { background: rgba(194,59,42,0.03); }
  .av-danger-label { font-size: 12px; color: var(--color-muted); }
  .av-danger-arrow {
    font-size: 10px; color: var(--color-muted);
    transition: transform 0.2s cubic-bezier(0.16, 1, 0.3, 1);
  }
  .av-danger-zone.open .av-danger-arrow { transform: rotate(90deg); }

  .av-danger-body {
    padding: 0 clamp(14px, 2.5vw, 20px) clamp(14px, 2.5vw, 20px);
    animation: av-enter 0.25s cubic-bezier(0.16, 1, 0.3, 1) both;
  }
  .av-danger-confirm {
    padding: 16px; border-color: var(--color-error); border-radius: 10px;
  }
  .av-danger-text {
    font-size: 12px; color: var(--color-muted); line-height: 1.6; margin-bottom: 14px;
  }
  .av-danger-actions { display: flex; gap: 8px; flex-wrap: wrap; }
  .av-btn-danger {
    display: inline-flex; align-items: center; justify-content: center; gap: 6px;
    padding: 8px 16px; font-size: 12px; font-weight: 600; font-family: inherit;
    color: white; background: var(--color-error); border: none; border-radius: 8px;
    cursor: pointer; transition: background 0.15s, transform 0.1s;
  }
  .av-btn-danger:hover:not(:disabled) { background: #a83222; transform: translateY(-1px); }
  .av-btn-danger:disabled { opacity: 0.5; cursor: not-allowed; }

  /* ── OTP input ── */
  .av-otp-input {
    font-family: "JetBrains Mono", monospace !important;
    font-size: 18px !important; letter-spacing: 0.3em; text-align: center;
    padding: 12px !important;
  }

  /* ── OTP card ── */
  .av-otp-card { padding: clamp(20px, 4vw, 28px); max-width: 400px; }
  .av-otp-desc { font-size: 12px; color: var(--color-muted); line-height: 1.6; margin-bottom: 20px; }
  .av-otp-form { display: flex; flex-direction: column; gap: 12px; }
  .av-otp-actions {
    display: flex; align-items: center; justify-content: space-between; margin-top: 8px;
  }
  .av-resend-btn {
    font-size: 11px; font-family: inherit; color: var(--color-muted);
    background: none; border: none; cursor: pointer;
    text-decoration: underline; transition: color 0.15s;
  }
  .av-resend-btn:hover { color: var(--color-foreground); }
  .av-resend-btn:disabled { opacity: 0.4; text-decoration: none; cursor: not-allowed; }
  .av-text-btn {
    font-size: 11px; font-family: inherit; color: var(--color-muted);
    background: none; border: none; cursor: pointer; transition: color 0.15s;
  }
  .av-text-btn:hover { color: var(--color-foreground); }

  /* ── Auth split ── */
  .av-auth-split {
    display: grid; gap: clamp(20px, 3vw, 28px); grid-template-columns: 1fr;
  }
  @media (min-width: 540px) {
    .av-auth-split { grid-template-columns: 1fr 1fr; align-items: start; }
  }

  .av-pitch {
    padding: clamp(20px, 4vw, 28px);
    list-style: none;
  }
  .av-pitch-list {
    display: flex; flex-direction: column; gap: 12px;
    list-style: none; padding: 0; margin: 0;
  }
  .av-pitch-item {
    display: flex; align-items: flex-start; gap: 10px;
    font-size: 13px; color: var(--color-muted); line-height: 1.5;
  }
  .av-pitch-check { width: 15px; height: 15px; flex-shrink: 0; margin-top: 2px; }

  /* ── Auth card ── */
  .av-auth-tabs { display: flex; border-bottom: 1px solid var(--color-border); }
  .av-auth-tab {
    flex: 1; padding: 13px; font-size: 13px; font-weight: 600;
    font-family: inherit; text-align: center; border: none; cursor: pointer;
    color: var(--color-muted); background: transparent; position: relative;
    transition: color 0.15s; border-radius: 0;
  }
  .av-auth-tab:hover { color: var(--color-foreground); }
  .av-auth-tab.on { color: var(--color-foreground); }
  .av-auth-tab.on::after {
    content: ''; position: absolute; bottom: -1px; left: 16px; right: 16px;
    height: 2px; background: var(--color-accent); border-radius: 2px 2px 0 0;
    animation: av-tab-grow 0.2s cubic-bezier(0.16, 1, 0.3, 1) both;
  }
  @keyframes av-tab-grow { from { transform: scaleX(0); } to { transform: scaleX(1); } }

  .av-auth-body {
    padding: clamp(16px, 3vw, 24px);
    display: flex; flex-direction: column; gap: 12px;
  }

  .av-google-btn {
    display: flex; align-items: center; justify-content: center; gap: 10px;
    width: 100%; padding: 11px; font-size: 13px; font-weight: 500; font-family: inherit;
    color: var(--color-foreground); background: var(--color-surface);
    border: 1px solid var(--color-border); border-radius: 8px; cursor: pointer;
    transition: border-color 0.15s, box-shadow 0.15s, transform 0.1s;
  }
  .av-google-btn:hover { border-color: var(--color-secondary); box-shadow: var(--shadow-card); transform: translateY(-1px); }
  .av-google-btn:active { transform: translateY(0); }
  .av-google-btn:disabled { opacity: 0.5; cursor: not-allowed; transform: none; }
  .av-google-icon { width: 18px; height: 18px; }

  .av-or { display: flex; align-items: center; gap: 14px; }
  .av-form-inset { padding: clamp(12px, 2vw, 16px); display: flex; flex-direction: column; gap: 10px; }

  /* ── Auth bottom ── */
  .av-link-btn {
    color: var(--color-primary); text-decoration: underline; background: none;
    border: none; font-family: inherit; font-size: inherit; cursor: pointer;
    transition: color 0.15s;
  }
  .av-link-btn:hover { color: var(--color-foreground); }

  /* ── Error ── */
  .av-error {
    padding: 10px 14px; border-radius: 8px;
    border: 1px solid rgba(194,59,42,0.3); background: rgba(194,59,42,0.04);
    font-size: 12px; color: var(--color-error); line-height: 1.5;
    animation: av-enter 0.25s cubic-bezier(0.16, 1, 0.3, 1) both;
  }
</style>
