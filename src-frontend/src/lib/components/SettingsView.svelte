<script lang="ts">
  import {
    getSettings,
    setProvider,
    saveApiKey,
    removeApiKey,
    updateModel,
    updateBaseUrl,
    testConnection,
    type SettingsInfo,
  } from "$lib/api/tauri";
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

  let requiresKey = $derived(settings?.provider.requiresKey ?? true);
  let isCustom = $derived(selectedPreset === "custom");

  $effect(() => {
    loadSettings();
  });

  async function loadSettings() {
    try {
      settings = await getSettings();
      selectedPreset = settings.provider.name;
      modelInput = settings.provider.model;
      baseUrlInput = settings.provider.baseUrl;
    } catch (e) {
      error = String(e);
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
      error = String(e);
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

  async function handleBaseUrlSave() {
    error = "";
    try {
      await updateBaseUrl(baseUrlInput);
    } catch (e) {
      error = String(e);
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
      error = String(e);
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
      error = `Connection failed: ${e}`;
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
        API keys are stored securely in macOS Keychain, never in config files.
        Any OpenAI-compatible provider works — Groq, Together, Mistral, OpenRouter, LM Studio, and more.
      </p>
    </div>
  {/if}
</div>
