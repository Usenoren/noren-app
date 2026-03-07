<script lang="ts">
  import {
    getSettings,
    setProvider,
    saveApiKey,
    removeApiKey,
    updateModel,
    updateBaseUrl,
    testConnection,
    listOllamaModels,
    listClaudeModels,
    getThinkingSettings,
    setThinkingSettings,
    updateHotkey,
    factoryReset,
    type SettingsInfo,
  } from "$lib/api/tauri";
  import { emit } from "@tauri-apps/api/event";
  import { friendlyError } from "$lib/utils/errors";
  import LoadingSpinner from "./LoadingSpinner.svelte";

  function focusOnMount(node: HTMLElement) {
    node.focus();
  }

  const presets = [
    { id: "claude-token", label: "Claude Token" },
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

  // Hotkey state
  let isRecording = $state(false);
  let recordedHotkey = $state("");
  let hotkeyError = $state("");

  // Ollama model discovery
  let ollamaModels = $state<string[]>([]);
  let ollamaLoading = $state(false);

  let requiresKey = $derived(settings?.provider.requiresKey ?? true);
  let isCustom = $derived(selectedPreset === "custom");
  let isOllama = $derived(selectedPreset === "ollama");
  let isClaudeToken = $derived(selectedPreset === "claude-token");
  let isAnthropicType = $derived(selectedPreset === "claude-token" || selectedPreset === "anthropic");
  let isNorenPro = $derived(settings?.inference_mode === "noren_pro");

  // Dynamic Claude model list
  let claudeModels = $state<{ id: string; label: string }[]>([]);
  let claudeModelsLoading = $state(false);

  // Extended thinking
  let extendedThinking = $state(false);
  let thinkingBudget = $state(10000);

  let showResetConfirm = $state(false);
  let resetting = $state(false);

  async function handleFactoryReset() {
    resetting = true;
    try {
      await factoryReset();
      window.location.reload();
    } catch (e) {
      error = friendlyError(e);
      resetting = false;
    }
  }

  $effect(() => {
    loadSettings();
  });

  async function loadSettings() {
    try {
      settings = await getSettings();
      selectedPreset = settings.provider.name;
      modelInput = settings.provider.model;
      baseUrlInput = settings.provider.baseUrl;

      // Load thinking settings
      try {
        const ts = await getThinkingSettings();
        extendedThinking = ts.enabled;
        thinkingBudget = ts.budget;
      } catch { /* ignore */ }

      if (settings.provider.name === "ollama") {
        fetchOllamaModels();
      }
      if ((settings.provider.name === "claude-token" || settings.provider.name === "anthropic") && settings.has_key) {
        fetchClaudeModels();
      }
    } catch (e) {
      error = friendlyError(e);
    }
  }

  function formatHotkeyHuman(s: string): string {
    return s
      .replace("Meta", "Cmd")
      .replace("Control", "Ctrl")
      .replace("Alt", "Option")
      .replace(/Key([A-Z])/g, "$1")
      .replace(/Digit(\d)/g, "$1")
      .split("+")
      .join(" + ");
  }

  function handleHotkeyKeydown(e: KeyboardEvent) {
    e.preventDefault();
    e.stopPropagation();

    // Ignore lone modifier presses
    if (["Meta", "Shift", "Alt", "Control"].includes(e.key)) return;

    const parts: string[] = [];
    if (e.metaKey) parts.push("Meta");
    if (e.ctrlKey) parts.push("Control");
    if (e.altKey) parts.push("Alt");
    if (e.shiftKey) parts.push("Shift");

    if (parts.length === 0) {
      hotkeyError = "At least one modifier key (Cmd, Ctrl, Alt, Shift) is required";
      return;
    }

    parts.push(e.code);
    recordedHotkey = parts.join("+");
    hotkeyError = "";
  }

  async function handleHotkeySave() {
    if (!recordedHotkey) return;
    hotkeyError = "";
    try {
      await updateHotkey(recordedHotkey);
      isRecording = false;
      recordedHotkey = "";
      await loadSettings();
    } catch (e) {
      hotkeyError = friendlyError(e);
    }
  }

  function handleHotkeyCancel() {
    isRecording = false;
    recordedHotkey = "";
    hotkeyError = "";
  }

  async function fetchClaudeModels() {
    claudeModelsLoading = true;
    try {
      const models = await listClaudeModels();
      claudeModels = models.map(m => ({ id: m.id, label: m.name }));
      if (claudeModels.length > 0 && !claudeModels.find(m => m.id === modelInput)) {
        modelInput = claudeModels[0].id;
        await updateModel(modelInput);
      }
    } catch {
      claudeModels = [];
    } finally {
      claudeModelsLoading = false;
    }
  }

  async function handleThinkingToggle() {
    extendedThinking = !extendedThinking;
    await setThinkingSettings(extendedThinking, thinkingBudget);
  }

  async function handleThinkingBudgetSave() {
    await setThinkingSettings(extendedThinking, thinkingBudget);
  }

  async function fetchOllamaModels() {
    ollamaLoading = true;
    try {
      ollamaModels = await listOllamaModels();
    } catch {
      ollamaModels = [];
    }
    ollamaLoading = false;

    // Auto-select first model if current model isn't available
    if (ollamaModels.length > 0 && !ollamaModels.includes(modelInput)) {
      modelInput = ollamaModels[0];
      await updateModel(modelInput);
    }
  }

  async function handlePresetChange(presetId: string) {
    selectedPreset = presetId;
    error = "";
    testResult = "";
    apiKeyInput = "";
    showKey = false;
    ollamaModels = [];

    if (presetId === "custom") {
      baseUrlInput = "";
      modelInput = "";
      return;
    }

    try {
      await setProvider({ name: presetId });
      await loadSettings();

      if (presetId === "ollama") {
        await fetchOllamaModels();
      }
      if (presetId === "claude-token" || presetId === "anthropic") {
        await fetchClaudeModels();
      }
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
    <!-- Keyboard Shortcut -->
    <div>
      <span class="block text-xs font-medium text-muted mb-2 uppercase tracking-wide">Quick Access Shortcut</span>
      {#if isRecording}
        <div class="flex flex-col gap-2">
          <div
            tabindex="-1"
            role="textbox"
            class="px-3 py-2 text-xs border-2 border-secondary bg-surface text-foreground rounded-md focus:outline-none text-center font-medium"
            onkeydown={handleHotkeyKeydown}
            use:focusOnMount
          >
            {recordedHotkey ? formatHotkeyHuman(recordedHotkey) : "Press a key combination..."}
          </div>
          <div class="flex gap-2">
            <button
              onclick={handleHotkeySave}
              disabled={!recordedHotkey}
              class="flex-1 px-3 py-1.5 text-xs bg-primary text-white hover:bg-primary-hover transition-colors cursor-pointer disabled:opacity-50 rounded-md font-medium"
            >
              Save
            </button>
            <button
              onclick={handleHotkeyCancel}
              class="px-3 py-1.5 text-xs border border-border hover:border-secondary transition-colors cursor-pointer text-muted hover:text-foreground rounded-md"
            >
              Cancel
            </button>
          </div>
          {#if hotkeyError}
            <p class="text-[10px] text-error">{hotkeyError}</p>
          {/if}
        </div>
      {:else}
        <div class="flex items-center justify-between">
          <span class="text-xs text-foreground font-medium">
            {formatHotkeyHuman(settings.hotkey)}
          </span>
          <button
            onclick={() => { isRecording = true; recordedHotkey = ""; hotkeyError = ""; }}
            class="px-3 py-1.5 text-xs border border-border hover:border-secondary transition-colors cursor-pointer text-muted hover:text-foreground rounded-md"
          >
            Change
          </button>
        </div>
      {/if}
    </div>

    {#if isNorenPro}
      <!-- Noren Pro inference badge -->
      <div class="p-3 bg-tint border border-secondary/30 rounded-lg">
        <div class="flex items-center gap-2">
          <span class="text-xs font-medium text-secondary">Noren Pro</span>
          <span class="px-1.5 py-0.5 text-[9px] font-semibold uppercase tracking-wider rounded-full bg-secondary/20 text-secondary">Active</span>
        </div>
        <p class="text-[10px] text-muted mt-1">No API key needed. Inference runs on Noren servers.</p>
      </div>
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
        {#if isAnthropicType && claudeModelsLoading}
          <div class="flex items-center gap-2 text-xs text-muted">
            <LoadingSpinner /> Fetching models...
          </div>
        {:else if isAnthropicType && claudeModels.length > 0}
          <select
            bind:value={modelInput}
            onchange={handleModelSave}
            class="w-full px-3 py-1.5 text-xs border border-border bg-surface text-foreground rounded-md focus:outline-none focus:border-secondary"
          >
            {#each claudeModels as m}
              <option value={m.id}>{m.label}</option>
            {/each}
          </select>
        {:else if isOllama && ollamaLoading}
          <div class="flex items-center gap-2 text-xs text-muted">
            <LoadingSpinner /> Detecting models...
          </div>
        {:else if isOllama && ollamaModels.length > 0}
          <select
            bind:value={modelInput}
            onchange={handleModelSave}
            class="w-full px-3 py-1.5 text-xs border border-border bg-surface text-foreground rounded-md focus:outline-none focus:border-secondary"
          >
            {#each ollamaModels as m}
              <option value={m}>{m}</option>
            {/each}
          </select>
        {:else}
          <div class="flex gap-2">
            <input
              type="text"
              bind:value={modelInput}
              class="flex-1 px-3 py-1.5 text-xs border border-border bg-surface text-foreground rounded-md focus:outline-none focus:border-secondary"
              placeholder={isAnthropicType ? "claude-sonnet-4-6" : "Model ID"}
            />
            <button
              onclick={handleModelSave}
              class="px-3 py-1.5 text-xs border border-border hover:border-secondary transition-colors cursor-pointer text-muted hover:text-foreground rounded-md"
            >
              Save
            </button>
          </div>
          {#if isOllama}
            <p class="text-[10px] text-warning mt-1">Could not detect models. Is Ollama running?</p>
          {/if}
        {/if}
      </div>

      <!-- Extended Thinking (Anthropic only) -->
      {#if isAnthropicType}
        <div>
          <div class="flex items-center justify-between">
            <span class="text-xs font-medium text-muted uppercase tracking-wide">Extended Thinking</span>
            <button
              onclick={handleThinkingToggle}
              class="relative w-9 h-5 rounded-full transition-colors cursor-pointer {extendedThinking ? 'bg-secondary' : 'bg-border'}"
            >
              <span class="absolute top-0.5 left-0.5 w-4 h-4 rounded-full bg-white transition-transform {extendedThinking ? 'translate-x-4' : ''}"></span>
            </button>
          </div>
          {#if extendedThinking}
            <div class="flex items-center gap-2 mt-2">
              <span class="text-[10px] text-muted whitespace-nowrap">Budget:</span>
              <select
                bind:value={thinkingBudget}
                onchange={handleThinkingBudgetSave}
                class="flex-1 px-2 py-1 text-[10px] border border-border bg-surface text-foreground rounded-md focus:outline-none focus:border-secondary"
              >
                <option value={5000}>5k tokens (fast)</option>
                <option value={10000}>10k tokens</option>
                <option value={25000}>25k tokens</option>
                <option value={50000}>50k tokens (deep)</option>
              </select>
            </div>
          {/if}
          <p class="text-[10px] text-muted mt-1.5">
            {extendedThinking ? "Model will reason step-by-step before responding. Slower but higher quality." : "Direct responses without chain-of-thought reasoning."}
          </p>
        </div>
      {/if}

      <!-- API Key (only for providers that require one) -->
      {#if requiresKey}
        <div>
          {#if isClaudeToken}
            <p class="text-[10px] text-muted mb-2 leading-relaxed">
              Run <code class="bg-surface px-1 py-0.5 rounded text-foreground">claude setup-token</code> in your terminal, then paste the token below.
            </p>
          {/if}
          <div class="flex items-center justify-between mb-1.5">
            <span class="text-xs font-medium text-muted uppercase tracking-wide">
              {isClaudeToken ? "Setup Token" : "API Key"}
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
                placeholder={isClaudeToken
                  ? (settings.has_key ? "Paste new token to replace" : "sk-ant-oat01-...")
                  : (settings.has_key ? "Enter new key to replace" : "Enter API key")}
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
                {isSaving ? "Saving..." : isClaudeToken ? "Save Token" : "Save to Keychain"}
              </button>
            </div>
          {/if}
        </div>
      {:else}
        <!-- No key needed message -->
        <div class="p-2 bg-tint border border-border rounded-lg">
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
      <div class="p-2 bg-tint border border-signal/30 rounded-lg text-xs text-signal">
        {testResult}
      </div>
    {/if}

    <!-- Error -->
    {#if error}
      <div class="p-2 bg-tint border border-border rounded-lg text-xs text-muted leading-relaxed">
        {error}
      </div>
    {/if}

    <!-- Info -->
    <div class="mt-auto">
      <div class="divider"></div>
      <p class="text-[10px] text-muted leading-relaxed pt-3">
        {#if isNorenPro}
          <button
            onclick={() => emit("navigate", "account")}
            class="text-primary hover:text-foreground cursor-pointer underline"
          >
            Manage subscription in Account
          </button>
        {:else}
          API keys are stored securely in macOS Keychain, never in config files.
          Any OpenAI-compatible provider works. Groq, Together, Mistral, OpenRouter, LM Studio, and more.
        {/if}
      </p>

      <!-- Factory Reset -->
      <div class="pt-4">
        {#if showResetConfirm}
          <div class="p-3 border border-error/30 bg-tint rounded-lg">
            <p class="text-xs text-foreground font-medium mb-1">Reset everything?</p>
            <p class="text-[10px] text-muted mb-3">This will delete all config, profiles, chat history, and keychain entries. The app will restart as if freshly installed.</p>
            <div class="flex gap-2">
              <button
                onclick={handleFactoryReset}
                disabled={resetting}
                class="px-3 py-1.5 text-xs bg-error text-white hover:bg-error/80 transition-colors cursor-pointer disabled:opacity-50 rounded-md font-medium"
              >
                {resetting ? "Resetting..." : "Yes, reset everything"}
              </button>
              <button
                onclick={() => { showResetConfirm = false; }}
                class="px-3 py-1.5 text-xs border border-border hover:border-secondary transition-colors cursor-pointer text-muted hover:text-foreground rounded-md"
              >
                Cancel
              </button>
            </div>
          </div>
        {:else}
          <button
            onclick={() => { showResetConfirm = true; }}
            class="text-[10px] text-muted hover:text-error transition-colors cursor-pointer"
          >
            Factory reset
          </button>
        {/if}
      </div>
    </div>
  {/if}
</div>
