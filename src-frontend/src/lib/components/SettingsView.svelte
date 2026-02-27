<script lang="ts">
  import {
    getSettings,
    saveApiKey,
    removeApiKey,
    updateProvider,
    updateModel,
    testApiKey,
    type SettingsInfo,
  } from "$lib/api/tauri";
  import LoadingSpinner from "./LoadingSpinner.svelte";

  const providers = [
    { id: "anthropic", label: "Anthropic", defaultModel: "claude-sonnet-4-20250514" },
    { id: "openai", label: "OpenAI", defaultModel: "gpt-4o" },
    { id: "gemini", label: "Gemini", defaultModel: "gemini-2.5-flash" },
  ] as const;

  let settings = $state<SettingsInfo | null>(null);
  let selectedProvider = $state("anthropic");
  let modelInput = $state("");
  let apiKeyInput = $state("");
  let showKey = $state(false);
  let isTesting = $state(false);
  let isSaving = $state(false);
  let testResult = $state("");
  let error = $state("");

  let hasKeyForCurrent = $derived(
    settings
      ? selectedProvider === "anthropic"
        ? settings.has_anthropic_key
        : selectedProvider === "openai"
          ? settings.has_openai_key
          : settings.has_gemini_key
      : false,
  );

  $effect(() => {
    loadSettings();
  });

  async function loadSettings() {
    try {
      settings = await getSettings();
      selectedProvider = settings.provider;
      modelInput = settings.model;
    } catch (e) {
      error = String(e);
    }
  }

  async function handleProviderChange(provider: string) {
    selectedProvider = provider;
    error = "";
    testResult = "";
    apiKeyInput = "";
    showKey = false;

    const p = providers.find((x) => x.id === provider);
    if (p) modelInput = p.defaultModel;

    try {
      await updateProvider(provider);
      await updateModel(modelInput);
      await loadSettings();
    } catch (e) {
      error = String(e);
    }
  }

  async function handleModelSave() {
    error = "";
    try {
      await updateModel(modelInput);
    } catch (e) {
      error = String(e);
    }
  }

  async function handleSaveKey() {
    if (!apiKeyInput.trim()) return;
    isSaving = true;
    error = "";
    try {
      await saveApiKey(selectedProvider, apiKeyInput.trim());
      apiKeyInput = "";
      showKey = false;
      await loadSettings();
    } catch (e) {
      error = String(e);
    } finally {
      isSaving = false;
    }
  }

  async function handleTestKey() {
    const key = apiKeyInput.trim();
    if (!key) return;
    isTesting = true;
    testResult = "";
    error = "";
    try {
      const response = await testApiKey(selectedProvider, key, modelInput);
      testResult = `Key works! Response: "${response}"`;
    } catch (e) {
      error = `Key test failed: ${e}`;
    } finally {
      isTesting = false;
    }
  }

  async function handleRemoveKey() {
    error = "";
    try {
      await removeApiKey(selectedProvider);
      await loadSettings();
    } catch (e) {
      error = String(e);
    }
  }
</script>

<div class="flex flex-col gap-4 h-full p-4 overflow-y-auto animate-fade-in-up">
  {#if !settings}
    <div class="flex items-center justify-center h-full">
      <LoadingSpinner />
    </div>
  {:else}
    <!-- Provider -->
    <div>
      <span class="block text-xs font-medium text-muted mb-2 uppercase tracking-wide">Provider</span>
      <div class="flex gap-1">
        {#each providers as p}
          <button
            onclick={() => handleProviderChange(p.id)}
            class="px-3 py-1.5 text-xs transition-colors cursor-pointer uppercase tracking-wide rounded-md
              {selectedProvider === p.id
                ? 'bg-primary text-white font-medium'
                : 'bg-surface text-muted border border-border hover:border-secondary hover:text-foreground'}"
          >
            {p.label}
          </button>
        {/each}
      </div>
    </div>

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

    <!-- API Key -->
    <div>
      <div class="flex items-center justify-between mb-1.5">
        <span class="text-xs font-medium text-muted uppercase tracking-wide">
          API Key
          <span class="ml-1.5 text-[10px] font-normal normal-case tracking-normal {hasKeyForCurrent ? 'text-signal' : 'text-muted'}">
            {hasKeyForCurrent ? "Stored in Keychain" : "Not set"}
          </span>
        </span>
        {#if hasKeyForCurrent}
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
            placeholder={hasKeyForCurrent ? "Enter new key to replace" : "Enter API key"}
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
            onclick={handleTestKey}
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

    <!-- Test result -->
    {#if testResult}
      <div class="p-2 bg-tint border border-signal/30 rounded-md text-xs text-signal">
        {testResult}
      </div>
    {/if}

    <!-- Error -->
    {#if error}
      <div class="p-2 bg-surface border border-error/30 rounded-md text-xs text-error">
        {error}
      </div>
    {/if}

    <!-- Info -->
    <div class="mt-auto">
      <div class="divider"></div>
      <p class="text-[10px] text-muted leading-relaxed pt-3">
        API keys are stored securely in macOS Keychain, never in config files or the binary.
        Keys from environment variables (ANTHROPIC_API_KEY, etc.) are also supported.
      </p>
    </div>
  {/if}
</div>
