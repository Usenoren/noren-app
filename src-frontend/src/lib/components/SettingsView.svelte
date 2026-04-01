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
    listGeminiModels,
    listOpenAIModels,
    listCustomModels,
    getThinkingSettings,
    setThinkingSettings,
    updateHotkey,
    factoryReset,
    type SettingsInfo,
  } from "$lib/api/tauri";
  import { emit } from "@tauri-apps/api/event";
  import { friendlyError } from "$lib/utils/errors";
  import { PALETTES, getTheme, setAndPersistTheme, type PaletteId } from "$lib/stores/theme.svelte";
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

  // OpenAI model discovery
  let openaiModels = $state<{ id: string; label: string }[]>([]);
  let openaiModelsLoading = $state(false);

  // Gemini model discovery
  let geminiModels = $state<{ id: string; label: string }[]>([]);
  let geminiModelsLoading = $state(false);

  // Custom model discovery
  let customModels = $state<{ id: string; label: string }[]>([]);
  let customModelsLoading = $state(false);

  let requiresKey = $derived(settings?.provider.requiresKey ?? true);
  let isCustom = $derived(selectedPreset === "custom");
  let isOllama = $derived(selectedPreset === "ollama");
  let isClaudeToken = $derived(selectedPreset === "claude-token");
  let isAnthropicType = $derived(selectedPreset === "claude-token" || selectedPreset === "anthropic");
  let isGemini = $derived(selectedPreset === "gemini");
  let isOpenAI = $derived(selectedPreset === "openai");
  let isNorenPro = $derived(settings?.noren_pro_logged_in === true);

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
      if (settings.provider.name === "gemini" && settings.has_key) {
        fetchGeminiModels();
      }
      if (settings.provider.name === "openai" && settings.has_key) {
        fetchOpenAIModels();
      }
      if (settings.provider.name === "custom" && settings.provider.baseUrl) {
        fetchCustomModels();
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

  async function fetchGeminiModels() {
    geminiModelsLoading = true;
    try {
      const models = await listGeminiModels();
      geminiModels = models.map(m => ({ id: m.id, label: m.name }));
      if (geminiModels.length > 0 && !geminiModels.find(m => m.id === modelInput)) {
        modelInput = geminiModels[0].id;
        await updateModel(modelInput);
      }
    } catch {
      geminiModels = [];
    } finally {
      geminiModelsLoading = false;
    }
  }

  async function fetchOpenAIModels() {
    openaiModelsLoading = true;
    try {
      const models = await listOpenAIModels();
      openaiModels = models.map(m => ({ id: m.id, label: m.name }));
      if (openaiModels.length > 0 && !openaiModels.find(m => m.id === modelInput)) {
        modelInput = openaiModels[0].id;
        await updateModel(modelInput);
      }
    } catch {
      openaiModels = [];
    } finally {
      openaiModelsLoading = false;
    }
  }

  async function fetchCustomModels() {
    customModelsLoading = true;
    try {
      const models = await listCustomModels();
      customModels = models.map(m => ({ id: m.id, label: m.name }));
      if (customModels.length > 0 && !customModels.find(m => m.id === modelInput)) {
        modelInput = customModels[0].id;
        await updateModel(modelInput);
      }
    } catch {
      customModels = [];
    } finally {
      customModelsLoading = false;
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
      customModels = [];
      await setProvider({ name: "custom", requiresKey: true });
      await loadSettings();
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
      if (presetId === "gemini") {
        await fetchGeminiModels();
      }
      if (presetId === "openai") {
        await fetchOpenAIModels();
      }
    } catch (e) {
      error = friendlyError(e);
    }
  }

  async function handleSaveCustom() {
    if (!baseUrlInput.trim()) return;
    error = "";
    try {
      await setProvider({
        name: "custom",
        baseUrl: baseUrlInput.trim(),
        model: modelInput.trim() || "",
        requiresKey: true,
      });
      await loadSettings();
      await fetchCustomModels();
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

<div class="sv-page animate-fade-in-up">
  <h1 class="text-heading text-foreground" style="margin-bottom: 6px;">Settings</h1>

  {#if !settings}
    <div class="flex items-center justify-center" style="min-height: 200px;">
      <LoadingSpinner />
    </div>
  {:else}
    <div class="sv-sections sv-stagger">

      <!-- ── Appearance ── -->
      <div class="card-flat sv-card-pad">
        <span class="section-label" style="display:block; margin-bottom: 12px;">Appearance</span>
        <div class="sv-palette-grid">
          {#each PALETTES as palette}
            {@const isActive = getTheme() === palette.id}
            <button
              class="sv-palette-btn"
              class:active={isActive}
              onclick={() => setAndPersistTheme(palette.id)}
            >
              <div class="sv-palette-preview" style="background: {palette.bg};">
                <div class="sv-palette-bar" style="background: {palette.surface}; border-bottom: 1px solid {palette.border};"></div>
                <div class="sv-palette-card" style="background: {palette.surface}; border: 1px solid {palette.border}; border-left: 2px solid {palette.accent};"></div>
              </div>
              <span class="sv-palette-label">
                {palette.name}{palette.id === 'kon' ? ' *' : ''}
              </span>
            </button>
          {/each}
        </div>
      </div>

      <!-- ── Keyboard Shortcut ── -->
      <div class="card-flat sv-card-pad">
        <span class="section-label" style="display:block; margin-bottom: 12px;">Quick Access Shortcut</span>
        {#if isRecording}
          <div class="sv-hotkey-record">
            <div
              tabindex="-1"
              role="textbox"
              class="card-inset sv-hotkey-capture"
              onkeydown={handleHotkeyKeydown}
              use:focusOnMount
            >
              {recordedHotkey ? formatHotkeyHuman(recordedHotkey) : "Press a key combination..."}
            </div>
            <div class="flex gap-2">
              <button onclick={handleHotkeySave} disabled={!recordedHotkey} class="btn-primary flex-1">Save</button>
              <button onclick={handleHotkeyCancel} class="btn-outline">Cancel</button>
            </div>
            {#if hotkeyError}
              <p class="text-[10px] text-error">{hotkeyError}</p>
            {/if}
          </div>
        {:else}
          <div class="card-inset sv-hotkey-display">
            <span class="text-xs text-foreground font-medium">{formatHotkeyHuman(settings.hotkey)}</span>
            <button
              onclick={() => { isRecording = true; recordedHotkey = ""; hotkeyError = ""; }}
              class="btn-outline"
            >Change</button>
          </div>
        {/if}
      </div>

      <div class="divider-thread"></div>

      <!-- ── Inference: Pro or BYOK ── -->
      {#if isNorenPro}
        <!-- Noren Pro inference -->
        <div class="card-hero sv-card-pad">
          <div class="flex items-center gap-2.5 mb-2">
            <span class="text-subhead text-foreground">Noren Pro</span>
            <span class="sv-status-badge sv-badge-active">Active</span>
          </div>
          <p class="text-xs text-muted leading-relaxed">No API key needed. Inference runs on Noren servers with your voice profile.</p>
          <div class="divider-thread" style="margin: 16px 0 12px;"></div>
          <button onclick={() => emit("navigate", "account")} class="btn-outline text-xs">Manage subscription</button>
        </div>

        <!-- Model (read-only for Pro) -->
        <div class="card-flat">
          <div class="sv-setting-row">
            <div>
              <div class="sv-setting-label">Model</div>
              <div class="sv-setting-desc">
                <span class="font-mono text-foreground">{settings.provider.model}</span>
                <span class="sv-status-badge sv-badge-info" style="margin-left: 6px;">Voice router</span>
              </div>
              <div class="text-[11px] text-muted" style="margin-top: 2px;">Selected automatically based on your voice profile</div>
            </div>
          </div>
        </div>

        <!-- Extended Thinking (Pro) -->
        <div class="card-flat" style="overflow: hidden;">
          <div class="sv-setting-row">
            <div>
              <div class="sv-setting-label">Extended thinking</div>
              <div class="sv-setting-desc">Chain-of-thought for complex tasks</div>
            </div>
            <button onclick={handleThinkingToggle} class="toggle {extendedThinking ? 'active' : ''}" aria-label="Toggle extended thinking"></button>
          </div>
          {#if extendedThinking}
            <div style="height: 1px; background: var(--color-border);"></div>
            <div class="flex items-center gap-2" style="padding: 10px clamp(14px, 2.5vw, 20px);">
              <span class="text-[11px] text-muted whitespace-nowrap">Budget:</span>
              <select bind:value={thinkingBudget} onchange={handleThinkingBudgetSave} class="input-field flex-1 text-[11px]">
                <option value={5000}>5k tokens (fast)</option>
                <option value={10000}>10k tokens</option>
                <option value={25000}>25k tokens</option>
                <option value={50000}>50k tokens (deep)</option>
              </select>
            </div>
          {/if}
        </div>
      {:else}
        <!-- ── BYOK Section ── -->

        <!-- Provider picker -->
        <div>
          <span class="section-label" style="display:block; margin-bottom: 8px;">Provider</span>
          <div class="sv-provider-grid">
            {#each presets as p}
              <button
                onclick={() => handlePresetChange(p.id)}
                class="sv-provider-btn"
                class:active={selectedPreset === p.id}
              >{p.label}</button>
            {/each}
          </div>
        </div>

        <!-- Base URL (Ollama / Custom) -->
        {#if selectedPreset === "ollama" || isCustom}
          <div>
            <span class="section-label" style="display:block; margin-bottom: 6px;">Base URL</span>
            <div class="flex gap-2">
              <input
                type="text"
                bind:value={baseUrlInput}
                class="input-field flex-1"
                placeholder={selectedPreset === "ollama" ? "http://localhost:11434/v1" : "https://api.example.com/v1"}
              />
              <button onclick={isCustom ? handleSaveCustom : handleBaseUrlSave} class="btn-outline">Save</button>
            </div>
          </div>
        {/if}

        <!-- Model -->
        <div>
          <span class="section-label" style="display:block; margin-bottom: 6px;">Model</span>
          {#if isAnthropicType && claudeModelsLoading}
            <div class="flex items-center gap-2 text-xs text-muted"><LoadingSpinner /> Fetching models...</div>
          {:else if isAnthropicType && claudeModels.length > 0}
            <select bind:value={modelInput} onchange={handleModelSave} class="input-field">{#each claudeModels as m}<option value={m.id}>{m.label}</option>{/each}</select>
          {:else if isOpenAI && openaiModelsLoading}
            <div class="flex items-center gap-2 text-xs text-muted"><LoadingSpinner /> Fetching models...</div>
          {:else if isOpenAI && openaiModels.length > 0}
            <select bind:value={modelInput} onchange={handleModelSave} class="input-field">{#each openaiModels as m}<option value={m.id}>{m.label}</option>{/each}</select>
          {:else if isOpenAI}
            <div class="flex gap-2">
              <input type="text" bind:value={modelInput} class="input-field flex-1" placeholder="gpt-4o" />
              <button onclick={handleModelSave} class="btn-outline">Save</button>
            </div>
          {:else if isGemini && geminiModelsLoading}
            <div class="flex items-center gap-2 text-xs text-muted"><LoadingSpinner /> Fetching models...</div>
          {:else if isGemini && geminiModels.length > 0}
            <select bind:value={modelInput} onchange={handleModelSave} class="input-field">{#each geminiModels as m}<option value={m.id}>{m.label}</option>{/each}</select>
          {:else if isGemini}
            <div class="flex gap-2">
              <input type="text" bind:value={modelInput} class="input-field flex-1" placeholder="gemini-2.0-flash" />
              <button onclick={handleModelSave} class="btn-outline">Save</button>
            </div>
          {:else if isCustom && customModelsLoading}
            <div class="flex items-center gap-2 text-xs text-muted"><LoadingSpinner /> Fetching models...</div>
          {:else if isCustom && customModels.length > 0}
            <select bind:value={modelInput} onchange={handleModelSave} class="input-field">{#each customModels as m}<option value={m.id}>{m.label}</option>{/each}</select>
          {:else if isOllama && ollamaLoading}
            <div class="flex items-center gap-2 text-xs text-muted"><LoadingSpinner /> Detecting models...</div>
          {:else if isOllama && ollamaModels.length > 0}
            <select bind:value={modelInput} onchange={handleModelSave} class="input-field">{#each ollamaModels as m}<option value={m}>{m}</option>{/each}</select>
          {:else}
            <div class="flex gap-2">
              <input type="text" bind:value={modelInput} class="input-field flex-1" placeholder={isAnthropicType ? "claude-sonnet-4-6" : "Model ID"} />
              <button onclick={handleModelSave} class="btn-outline">Save</button>
            </div>
            {#if isOllama}
              <p class="text-[10px] text-warning mt-1">Could not detect models. Is Ollama running?</p>
            {/if}
          {/if}
        </div>

        <!-- Extended Thinking (Anthropic BYOK) -->
        {#if isAnthropicType}
          <div class="card-flat" style="overflow: hidden;">
            <div class="sv-setting-row">
              <div>
                <div class="sv-setting-label">Extended thinking</div>
                <div class="sv-setting-desc">
                  {extendedThinking ? "Model will reason step-by-step before responding. Slower but higher quality." : "Direct responses without chain-of-thought reasoning."}
                </div>
              </div>
              <button onclick={handleThinkingToggle} class="toggle {extendedThinking ? 'active' : ''}" aria-label="Toggle extended thinking"></button>
            </div>
            {#if extendedThinking}
              <div style="height: 1px; background: var(--color-border);"></div>
              <div class="flex items-center gap-2" style="padding: 10px clamp(14px, 2.5vw, 20px);">
                <span class="text-[11px] text-muted whitespace-nowrap">Budget:</span>
                <select bind:value={thinkingBudget} onchange={handleThinkingBudgetSave} class="input-field flex-1 text-[11px]">
                  <option value={5000}>5k tokens (fast)</option>
                  <option value={10000}>10k tokens</option>
                  <option value={25000}>25k tokens</option>
                  <option value={50000}>50k tokens (deep)</option>
                </select>
              </div>
            {/if}
          </div>
        {/if}

        <!-- API Key -->
        {#if requiresKey}
          <div>
            {#if isClaudeToken}
              <p class="text-[10px] text-muted mb-2 leading-relaxed">
                Run <code class="bg-surface px-1 py-0.5 rounded text-foreground">claude setup-token</code> in your terminal, then paste the token below.
              </p>
            {/if}
            <div class="flex items-center justify-between mb-1.5">
              <span class="section-label">
                {isClaudeToken ? "Setup Token" : "API Key"}
                <span class="ml-1.5 text-[10px] font-normal normal-case tracking-normal {settings.has_key ? 'text-signal' : 'text-muted'}">
                  {settings.has_key ? "Stored in Keychain" : "Not set"}
                </span>
              </span>
              {#if settings.has_key}
                <button onclick={handleRemoveKey} class="text-[10px] text-error hover:text-foreground cursor-pointer uppercase tracking-wide">Remove</button>
              {/if}
            </div>

            <div class="flex gap-2">
              <div class="relative flex-1">
                <input
                  type={showKey ? "text" : "password"}
                  bind:value={apiKeyInput}
                  class="input-field pr-12"
                  placeholder={isClaudeToken
                    ? (settings.has_key ? "Paste new token to replace" : "sk-ant-oat01-...")
                    : (settings.has_key ? "Enter new key to replace" : "Enter API key")}
                />
                <button
                  onclick={() => { showKey = !showKey; }}
                  class="absolute right-2 top-1/2 -translate-y-1/2 text-[10px] text-muted hover:text-secondary cursor-pointer uppercase"
                >{showKey ? "Hide" : "Show"}</button>
              </div>
            </div>

            {#if apiKeyInput.trim()}
              <div class="flex gap-2 mt-2">
                <button onclick={handleTestConnection} disabled={isTesting} class="btn-outline">
                  {#if isTesting}<span class="inline-flex items-center gap-1"><LoadingSpinner /> Testing</span>{:else}Test{/if}
                </button>
                <button onclick={handleSaveKey} disabled={isSaving} class="btn-primary">
                  {isSaving ? "Saving..." : isClaudeToken ? "Save Token" : "Save to Keychain"}
                </button>
              </div>
            {/if}
          </div>
        {:else}
          <div class="p-2 bg-tint border border-border rounded-xl">
            <p class="text-xs text-muted">No API key needed. {settings.provider.name} runs locally.</p>
          </div>
        {/if}

        <!-- Test Connection (stored key or no key needed) -->
        {#if !requiresKey || (settings.has_key && !apiKeyInput.trim())}
          <button onclick={handleTestConnection} disabled={isTesting} class="btn-outline self-start">
            {#if isTesting}<span class="inline-flex items-center gap-1"><LoadingSpinner /> Testing</span>{:else}Test Connection{/if}
          </button>
        {/if}
      {/if}

      <!-- Test result -->
      {#if testResult}
        <div class="sv-result-bar sv-result-ok">{testResult}</div>
      {/if}

      <!-- Error -->
      {#if error}
        <div class="sv-result-bar sv-result-err">{error}</div>
      {/if}

      <!-- Info (BYOK only) -->
      {#if !isNorenPro}
        <div>
          <div class="divider-thread"></div>
          <p class="text-[11px] text-muted leading-relaxed pt-3">
            API keys are stored securely in macOS Keychain, never in config files.
            Any OpenAI-compatible provider works. Groq, Together, Mistral, OpenRouter, LM Studio, and more.
          </p>
        </div>
      {/if}

      <!-- Factory Reset -->
      <div class="sv-footer">
        <div class="divider-thread"></div>
        <div style="padding-top: 12px;">
          {#if showResetConfirm}
            <div class="card-flat sv-reset-confirm">
              <p class="text-xs text-foreground font-medium" style="margin-bottom: 4px;">Reset everything?</p>
              <p class="text-[10px] text-muted" style="margin-bottom: 12px;">This will delete all config, profiles, chat history, and keychain entries. The app will restart as if freshly installed.</p>
              <div class="flex gap-2">
                <button onclick={handleFactoryReset} disabled={resetting} class="sv-btn-danger">
                  {resetting ? "Resetting..." : "Yes, reset everything"}
                </button>
                <button onclick={() => { showResetConfirm = false; }} class="btn-outline">Cancel</button>
              </div>
            </div>
          {:else}
            <button onclick={() => { showResetConfirm = true; }} class="sv-reset-link">Factory reset</button>
          {/if}
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  /* ── Page container ── */
  .sv-page {
    padding: clamp(20px, 4vw, 40px);
    padding-top: clamp(16px, 3vw, 28px);
    max-width: 680px;
    height: 100%;
    overflow-y: auto;
  }

  /* ── Sections ── */
  .sv-sections {
    display: flex;
    flex-direction: column;
    gap: clamp(16px, 2.5vw, 24px);
  }

  /* ── Staggered entry ── */
  .sv-stagger > :global(*) {
    animation: sv-enter 0.4s cubic-bezier(0.16, 1, 0.3, 1) both;
  }
  .sv-stagger > :global(*:nth-child(1)) { animation-delay: 0ms; }
  .sv-stagger > :global(*:nth-child(2)) { animation-delay: 60ms; }
  .sv-stagger > :global(*:nth-child(3)) { animation-delay: 120ms; }
  .sv-stagger > :global(*:nth-child(4)) { animation-delay: 180ms; }
  .sv-stagger > :global(*:nth-child(5)) { animation-delay: 240ms; }
  .sv-stagger > :global(*:nth-child(6)) { animation-delay: 300ms; }
  .sv-stagger > :global(*:nth-child(7)) { animation-delay: 360ms; }
  .sv-stagger > :global(*:nth-child(8)) { animation-delay: 420ms; }
  .sv-stagger > :global(*:nth-child(9)) { animation-delay: 480ms; }
  .sv-stagger > :global(*:nth-child(10)) { animation-delay: 540ms; }

  @keyframes sv-enter {
    from { opacity: 0; transform: translateY(10px); }
    to { opacity: 1; transform: translateY(0); }
  }

  /* ── Card padding ── */
  .sv-card-pad { padding: clamp(14px, 2.5vw, 20px); }

  /* ── Palette grid ── */
  .sv-palette-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(100px, 1fr));
    gap: 8px;
  }
  .sv-palette-btn {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: 6px;
    border-radius: 8px;
    border: 1px solid var(--color-border);
    background: transparent;
    cursor: pointer;
    transition: border-color 0.2s, background 0.2s, box-shadow 0.2s;
  }
  .sv-palette-btn:hover { border-color: var(--color-secondary); }
  .sv-palette-btn.active {
    border-color: var(--color-accent);
    background: var(--color-accent-wash);
    box-shadow: 0 0 10px rgba(122,51,64,0.15);
  }
  .sv-palette-preview {
    width: 100%;
    height: 42px;
    border-radius: 6px;
    overflow: hidden;
    position: relative;
  }
  .sv-palette-bar {
    height: 8px;
  }
  .sv-palette-card {
    margin: 5px 8px;
    height: 16px;
    border-radius: 3px;
  }
  .sv-palette-label {
    font-family: "JetBrains Mono", monospace;
    font-size: 10px;
    color: var(--color-muted);
    font-weight: 500;
  }

  /* ── Hotkey ── */
  .sv-hotkey-record {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .sv-hotkey-capture {
    padding: 12px;
    text-align: center;
    font-size: 13px;
    font-weight: 500;
    color: var(--color-foreground);
    border: 2px solid var(--color-secondary) !important;
    outline: none;
  }
  .sv-hotkey-display {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 12px;
  }

  /* ── Setting rows ── */
  .sv-setting-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: clamp(12px, 2vw, 16px) clamp(14px, 2.5vw, 20px);
    gap: 12px;
    flex-wrap: wrap;
  }
  .sv-setting-label { font-size: 13px; font-weight: 600; }
  .sv-setting-desc { font-size: 11px; color: var(--color-muted); margin-top: 2px; }

  /* ── Status badges ── */
  .sv-status-badge {
    display: inline-flex;
    align-items: center;
    padding: 2px 8px;
    font-size: 9px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    border-radius: 100px;
  }
  .sv-badge-active { background: rgba(122,51,64,0.12); color: var(--color-accent); }
  .sv-badge-info { background: var(--color-tint); color: var(--color-secondary); }

  /* ── Provider grid ── */
  .sv-provider-grid {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .sv-provider-btn {
    padding: 7px 14px;
    font-size: 12px;
    font-family: inherit;
    border-radius: 8px;
    cursor: pointer;
    transition: background 0.15s, color 0.15s, border-color 0.15s;
    background: var(--color-surface);
    color: var(--color-muted);
    border: 1px solid var(--color-border);
  }
  .sv-provider-btn:hover {
    border-color: var(--color-secondary);
    color: var(--color-foreground);
  }
  .sv-provider-btn.active {
    background: var(--color-primary);
    color: white;
    border-color: var(--color-primary);
    font-weight: 500;
  }

  /* ── Result bars ── */
  .sv-result-bar {
    padding: 10px 14px;
    border-radius: 8px;
    font-size: 12px;
    line-height: 1.5;
    animation: sv-enter 0.25s cubic-bezier(0.16, 1, 0.3, 1) both;
  }
  .sv-result-ok {
    background: rgba(45,122,79,0.04);
    border: 1px solid rgba(45,122,79,0.3);
    color: var(--color-signal);
  }
  .sv-result-err {
    background: rgba(194,59,42,0.04);
    border: 1px solid rgba(194,59,42,0.3);
    color: var(--color-error);
  }

  /* ── Footer ── */
  .sv-footer { margin-top: auto; }

  /* ── Factory reset ── */
  .sv-reset-confirm {
    padding: clamp(14px, 2.5vw, 20px);
    border-color: var(--color-error);
  }
  .sv-btn-danger {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 8px 16px;
    font-size: 12px;
    font-weight: 600;
    font-family: inherit;
    color: white;
    background: var(--color-error);
    border: none;
    border-radius: 8px;
    cursor: pointer;
    transition: background 0.15s, transform 0.1s;
  }
  .sv-btn-danger:hover:not(:disabled) { background: #a83222; transform: translateY(-1px); }
  .sv-btn-danger:disabled { opacity: 0.5; cursor: not-allowed; }

  .sv-reset-link {
    font-size: 11px;
    font-family: inherit;
    color: var(--color-muted);
    background: none;
    border: none;
    cursor: pointer;
    transition: color 0.15s;
  }
  .sv-reset-link:hover { color: var(--color-error); }
</style>
