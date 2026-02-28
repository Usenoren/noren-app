<script lang="ts">
  import {
    getSettings,
    setProvider,
    saveApiKey,
    removeApiKey,
    updateModel,
    updateBaseUrl,
    testConnection,
    setInferenceMode,
    norenProLogin,
    norenProSignup,
    norenProLogout,
    getNorenProUsage,
    getSubscriptionStatus,
    createCheckout,
    openBillingPortal,
    googleOAuthInit,
    googleOAuthPoll,
    type SettingsInfo,
    type NorenProStatus,
    type SubscriptionStatus,
  } from "$lib/api/tauri";
  import { open } from "@tauri-apps/plugin-shell";
  import { friendlyError } from "$lib/utils/errors";
  import LoadingSpinner from "./LoadingSpinner.svelte";

  const presets = [
    { id: "anthropic", label: "Anthropic" },
    { id: "openai", label: "OpenAI" },
    { id: "gemini", label: "Gemini" },
    { id: "ollama", label: "Ollama" },
    { id: "custom", label: "Custom" },
  ] as const;

  let settings = $state<SettingsInfo | null>(null);
  let selectedPreset = $state("anthropic");
  let modelInput = $state("");
  let baseUrlInput = $state("");
  let apiKeyInput = $state("");
  let showKey = $state(false);
  let isTesting = $state(false);
  let isSaving = $state(false);
  let testResult = $state("");
  let error = $state("");

  // Noren Pro state
  let proEmail = $state("");
  let proPassword = $state("");
  let proLoading = $state(false);
  let proStatus = $state<NorenProStatus | null>(null);
  let authMode = $state<"login" | "signup">("login");
  let googleLoading = $state(false);

  // Subscription state
  let subscription = $state<SubscriptionStatus | null>(null);

  let requiresKey = $derived(settings?.provider.requiresKey ?? true);
  let isCustom = $derived(selectedPreset === "custom");
  let showProSection = $state(false);
  let isNorenPro = $derived(settings?.inference_mode === "noren_pro");

  const tiers = [
    { id: "extraction", label: "Extraction", price: "$29", period: "one-time", desc: "Voice profile extraction" },
    { id: "living", label: "Living Profile", price: "$8", period: "/mo", desc: "Auto-improving voice profile" },
    { id: "pro", label: "Noren Pro", price: "$19", period: "/mo", desc: "Bundled inference + everything" },
  ] as const;

  $effect(() => {
    loadSettings();
  });

  async function loadSettings() {
    try {
      settings = await getSettings();
      selectedPreset = settings.provider.name;
      modelInput = settings.provider.model;
      baseUrlInput = settings.provider.baseUrl;
      showProSection = settings.inference_mode === "noren_pro";

      // Load Noren Pro status + usage if logged in
      if (settings.noren_pro_logged_in) {
        try {
          proStatus = await getNorenProUsage();
          // Also load subscription status
          try {
            subscription = await getSubscriptionStatus();
          } catch {
            subscription = null;
          }
        } catch {
          // Token is likely stale (server restarted) — auto-logout
          try {
            await norenProLogout();
            settings = await getSettings();
            showProSection = false;
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

  async function handleModeSwitch(mode: "byok" | "noren_pro") {
    error = "";
    try {
      await setInferenceMode(mode);
      await loadSettings();
    } catch (e) {
      error = friendlyError(e);
    }
  }

  async function handleProAuth() {
    if (!proEmail.trim() || !proPassword.trim()) return;
    proLoading = true;
    error = "";
    try {
      if (authMode === "signup") {
        await norenProSignup(proEmail.trim(), proPassword.trim());
      } else {
        await norenProLogin(proEmail.trim(), proPassword.trim());
      }
      proEmail = "";
      proPassword = "";
      await handleModeSwitch("noren_pro");
    } catch (e) {
      error = friendlyError(e);
    } finally {
      proLoading = false;
    }
  }

  async function handleGoogleSignIn() {
    googleLoading = true;
    error = "";
    try {
      const { auth_url, session_id } = await googleOAuthInit();
      await open(auth_url);

      // Poll every 2s until complete (max 5 min)
      for (let i = 0; i < 150; i++) {
        await new Promise((r) => setTimeout(r, 2000));
        if (!googleLoading) return; // cancelled
        try {
          const result = await googleOAuthPoll(session_id);
          if (result.complete) {
            await handleModeSwitch("noren_pro");
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
      await handleModeSwitch("byok");
    } catch (e) {
      error = friendlyError(e);
    }
  }

  async function handleUpgrade(tier: string) {
    error = "";
    try {
      const result = await createCheckout(tier);
      if (result.checkout_url === "dev://granted") {
        // Dev mode — tier granted directly, just reload
        await loadSettings();
      } else {
        await open(result.checkout_url);
      }
    } catch (e) {
      error = friendlyError(e);
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

  async function handlePresetChange(presetId: string) {
    selectedPreset = presetId;
    error = "";
    testResult = "";
    apiKeyInput = "";
    showKey = false;

    if (presetId === "custom") {
      baseUrlInput = "";
      modelInput = "";
      return;
    }

    try {
      await setProvider({ name: presetId });
      await loadSettings();
    } catch (e) {
      error = friendlyError(e);
    }
  }

  async function handleSaveCustom() {
    if (!baseUrlInput.trim() || !modelInput.trim()) return;
    error = "";
    try {
      await setProvider({
        name: "custom",
        baseUrl: baseUrlInput.trim(),
        model: modelInput.trim(),
        requiresKey: true,
      });
      await loadSettings();
    } catch (e) {
      error = friendlyError(e);
    }
  }

  async function handleModelSave() {
    error = "";
    try {
      await updateModel(modelInput);
    } catch (e) {
      error = friendlyError(e);
    }
  }

  async function handleBaseUrlSave() {
    error = "";
    try {
      await updateBaseUrl(baseUrlInput);
    } catch (e) {
      error = friendlyError(e);
    }
  }

  async function handleSaveKey() {
    if (!apiKeyInput.trim()) return;
    isSaving = true;
    error = "";
    try {
      await saveApiKey(apiKeyInput.trim());
      apiKeyInput = "";
      showKey = false;
      await loadSettings();
    } catch (e) {
      error = friendlyError(e);
    } finally {
      isSaving = false;
    }
  }

  async function handleTestConnection() {
    isTesting = true;
    testResult = "";
    error = "";
    try {
      const key = apiKeyInput.trim() || undefined;
      const response = await testConnection(key);
      testResult = `Connected! Response: "${response}"`;
    } catch (e) {
      error = friendlyError(e);
    } finally {
      isTesting = false;
    }
  }

  async function handleRemoveKey() {
    error = "";
    try {
      await removeApiKey();
      await loadSettings();
    } catch (e) {
      error = friendlyError(e);
    }
  }
</script>

<div class="flex flex-col gap-4 h-full p-4 overflow-y-auto animate-fade-in-up">
  {#if !settings}
    <div class="flex items-center justify-center h-full">
      <LoadingSpinner />
    </div>
  {:else}
    <!-- Inference Mode Toggle -->
    <div>
      <span class="block text-xs font-medium text-muted mb-2 uppercase tracking-wide">Inference</span>
      <div class="flex gap-1">
        <button
          onclick={() => { showProSection = false; handleModeSwitch("byok"); }}
          class="flex-1 px-3 py-2 text-xs transition-colors cursor-pointer rounded-md text-center
            {!showProSection
              ? 'bg-primary text-white font-medium'
              : 'bg-surface text-muted border border-border hover:border-secondary hover:text-foreground'}"
        >
          BYOK
          <span class="block text-[10px] font-normal opacity-70 mt-0.5">Your API key</span>
        </button>
        <button
          onclick={() => { showProSection = true; if (settings?.noren_pro_logged_in) handleModeSwitch("noren_pro"); }}
          class="flex-1 px-3 py-2 text-xs transition-colors cursor-pointer rounded-md text-center
            {showProSection
              ? 'bg-secondary text-white font-medium'
              : 'bg-surface text-muted border border-border hover:border-secondary hover:text-foreground'}"
        >
          Noren Pro
          <span class="block text-[10px] font-normal opacity-70 mt-0.5">No key needed</span>
        </button>
      </div>
    </div>

    {#if showProSection}
      <!-- Noren Pro section -->
      {#if settings.noren_pro_logged_in && proStatus}
        <div class="flex flex-col gap-3">
          <!-- Account info -->
          <div class="p-3 bg-tint border border-secondary/30 rounded-md">
            <div class="flex items-center justify-between mb-2">
              <span class="text-xs font-medium text-secondary">
                {subscription?.tier === "pro" ? "Noren Pro" : subscription?.tier === "living" ? "Living Profile" : subscription?.tier === "extraction" ? "Extraction" : "Free"}
              </span>
              <button
                onclick={handleProLogout}
                class="text-[10px] text-muted hover:text-error cursor-pointer uppercase tracking-wide"
              >
                Sign out
              </button>
            </div>
            <p class="text-[10px] text-muted">{proStatus.email}</p>

            {#if proStatus.tokens_used != null && proStatus.tokens_limit != null}
              <div class="mt-2">
                <div class="flex items-center justify-between text-[10px] text-muted mb-1">
                  <span>{proStatus.tokens_used.toLocaleString()} tokens used</span>
                  <span>{proStatus.tokens_limit.toLocaleString()} limit</span>
                </div>
                <div class="h-1.5 bg-border rounded-full overflow-hidden">
                  <div
                    class="h-full bg-secondary rounded-full transition-all"
                    style="width: {Math.min(100, (proStatus.tokens_used / proStatus.tokens_limit) * 100)}%"
                  ></div>
                </div>
                <p class="text-[10px] text-muted mt-1">
                  {proStatus.requests_this_month} requests this month
                </p>
              </div>
            {/if}

            {#if subscription?.active && subscription.cancel_at_period_end}
              <p class="text-[10px] text-warning mt-2">
                Cancels at end of period
              </p>
            {/if}
          </div>

          <!-- Subscription tiers -->
          {#if !subscription?.active || subscription.tier === "free"}
            <div>
              <span class="block text-xs font-medium text-muted mb-2 uppercase tracking-wide">Upgrade</span>
              <div class="flex flex-col gap-2">
                {#each tiers as t}
                  <button
                    onclick={() => handleUpgrade(t.id)}
                    class="flex items-center justify-between p-3 bg-surface border border-border rounded-md hover:border-secondary transition-colors cursor-pointer text-left"
                  >
                    <div>
                      <span class="text-xs font-medium text-foreground">{t.label}</span>
                      <span class="block text-[10px] text-muted mt-0.5">{t.desc}</span>
                    </div>
                    <span class="text-xs font-medium text-secondary">{t.price}<span class="text-[10px] text-muted font-normal">{t.period}</span></span>
                  </button>
                {/each}
              </div>
            </div>
          {:else}
            <button
              onclick={handleManageBilling}
              class="px-3 py-1.5 text-xs border border-border hover:border-secondary transition-colors cursor-pointer text-muted hover:text-foreground rounded-md self-start"
            >
              Manage subscription
            </button>
          {/if}
        </div>
      {:else}
        <!-- Login/Signup form -->
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
            class="w-full py-2 text-xs font-medium bg-secondary text-white hover:bg-secondary/90 transition-colors cursor-pointer disabled:opacity-50 rounded-md"
          >
            {#if proLoading}
              <span class="inline-flex items-center gap-1"><LoadingSpinner /> {authMode === "signup" ? "Creating..." : "Signing in..."}</span>
            {:else}
              {authMode === "signup" ? "Create account" : "Sign in"}
            {/if}
          </button>
        </div>
      {/if}
    {:else}
      <!-- BYOK section -->
      <!-- Provider -->
      <div>
        <span class="block text-xs font-medium text-muted mb-2 uppercase tracking-wide">Provider</span>
        <div class="flex flex-wrap gap-1">
          {#each presets as p}
            <button
              onclick={() => handlePresetChange(p.id)}
              class="px-3 py-1.5 text-xs transition-colors cursor-pointer rounded-md
                {selectedPreset === p.id
                  ? 'bg-primary text-white font-medium'
                  : 'bg-surface text-muted border border-border hover:border-secondary hover:text-foreground'}"
            >
              {p.label}
            </button>
          {/each}
        </div>
      </div>

      <!-- Base URL (for Ollama and Custom) -->
      {#if selectedPreset === "ollama" || isCustom}
        <div>
          <span class="block text-xs font-medium text-muted mb-1.5 uppercase tracking-wide">Base URL</span>
          <div class="flex gap-2">
            <input
              type="text"
              bind:value={baseUrlInput}
              class="flex-1 px-3 py-1.5 text-xs border border-border bg-surface text-foreground rounded-md focus:outline-none focus:border-secondary"
              placeholder={selectedPreset === "ollama" ? "http://localhost:11434/v1" : "https://api.example.com/v1"}
            />
            <button
              onclick={isCustom ? handleSaveCustom : handleBaseUrlSave}
              class="px-3 py-1.5 text-xs border border-border hover:border-secondary transition-colors cursor-pointer text-muted hover:text-foreground rounded-md"
            >
              Save
            </button>
          </div>
        </div>
      {/if}

      <!-- Model -->
      <div>
        <span class="block text-xs font-medium text-muted mb-1.5 uppercase tracking-wide">Model</span>
        <div class="flex gap-2">
          <input
            type="text"
            bind:value={modelInput}
            class="flex-1 px-3 py-1.5 text-xs border border-border bg-surface text-foreground rounded-md focus:outline-none focus:border-secondary"
            placeholder="Model ID"
          />
          <button
            onclick={handleModelSave}
            class="px-3 py-1.5 text-xs border border-border hover:border-secondary transition-colors cursor-pointer text-muted hover:text-foreground rounded-md"
          >
            Save
          </button>
        </div>
      </div>

      <!-- API Key (only for providers that require one) -->
      {#if requiresKey}
        <div>
          <div class="flex items-center justify-between mb-1.5">
            <span class="text-xs font-medium text-muted uppercase tracking-wide">
              API Key
              <span class="ml-1.5 text-[10px] font-normal normal-case tracking-normal {settings.has_key ? 'text-signal' : 'text-muted'}">
                {settings.has_key ? "Stored in Keychain" : "Not set"}
              </span>
            </span>
            {#if settings.has_key}
              <button
                onclick={handleRemoveKey}
                class="text-[10px] text-error hover:text-foreground cursor-pointer uppercase tracking-wide"
              >
                Remove
              </button>
            {/if}
          </div>

          <div class="flex gap-2">
            <div class="relative flex-1">
              <input
                type={showKey ? "text" : "password"}
                bind:value={apiKeyInput}
                class="w-full px-3 py-1.5 pr-12 text-xs border border-border bg-surface text-foreground rounded-md focus:outline-none focus:border-secondary"
                placeholder={settings.has_key ? "Enter new key to replace" : "Enter API key"}
              />
              <button
                onclick={() => { showKey = !showKey; }}
                class="absolute right-2 top-1/2 -translate-y-1/2 text-[10px] text-muted hover:text-secondary cursor-pointer uppercase"
              >
                {showKey ? "Hide" : "Show"}
              </button>
            </div>
          </div>

          {#if apiKeyInput.trim()}
            <div class="flex gap-2 mt-2">
              <button
                onclick={handleTestConnection}
                disabled={isTesting}
                class="px-3 py-1.5 text-xs border border-border hover:border-secondary transition-colors cursor-pointer text-muted hover:text-foreground disabled:opacity-50 rounded-md"
              >
                {#if isTesting}
                  <span class="inline-flex items-center gap-1"><LoadingSpinner /> Testing</span>
                {:else}
                  Test
                {/if}
              </button>
              <button
                onclick={handleSaveKey}
                disabled={isSaving}
                class="px-3 py-1.5 text-xs bg-primary text-white hover:bg-primary-hover transition-colors cursor-pointer disabled:opacity-50 rounded-md font-medium"
              >
                {isSaving ? "Saving..." : "Save to Keychain"}
              </button>
            </div>
          {/if}
        </div>
      {:else}
        <!-- No key needed message -->
        <div class="p-2 bg-tint border border-border rounded-md">
          <p class="text-xs text-muted">
            No API key needed — {settings.provider.name} runs locally.
          </p>
        </div>
      {/if}

      <!-- Test Connection (for providers without key, or with stored key) -->
      {#if !requiresKey || (settings.has_key && !apiKeyInput.trim())}
        <button
          onclick={handleTestConnection}
          disabled={isTesting}
          class="px-3 py-1.5 text-xs border border-border hover:border-secondary transition-colors cursor-pointer text-muted hover:text-foreground disabled:opacity-50 rounded-md self-start"
        >
          {#if isTesting}
            <span class="inline-flex items-center gap-1"><LoadingSpinner /> Testing</span>
          {:else}
            Test Connection
          {/if}
        </button>
      {/if}
    {/if}

    <!-- Test result -->
    {#if testResult}
      <div class="p-2 bg-tint border border-signal/30 rounded-md text-xs text-signal">
        {testResult}
      </div>
    {/if}

    <!-- Error -->
    {#if error}
      <div class="p-2 bg-tint border border-border rounded-md text-xs text-muted leading-relaxed">
        {error}
      </div>
    {/if}

    <!-- Info -->
    <div class="mt-auto">
      <div class="divider"></div>
      <p class="text-[10px] text-muted leading-relaxed pt-3">
        {#if showProSection}
          Noren Pro handles inference on our servers. No API key needed. Usage resets monthly.
        {:else}
          API keys are stored securely in macOS Keychain, never in config files.
          Any OpenAI-compatible provider works — Groq, Together, Mistral, OpenRouter, LM Studio, and more.
        {/if}
      </p>
    </div>
  {/if}
</div>
