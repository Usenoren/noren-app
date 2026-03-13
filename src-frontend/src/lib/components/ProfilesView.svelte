<script lang="ts">
  import {
    getProfileOverview,
    readProfileContent,
    saveProfileEdit,
    getLivingProfileStatus,
    setLivingProfileEnabled,
    uploadEditLog,
    refreshLivingProfile,
    getProfilePatches,
    approveProfilePatch,
    rejectProfilePatch,
    syncProfileUp,
    syncProfileDown,
    getSyncStatus,
    exportProfile,
    createCheckout,
    getSettings,
    type ProfileOverview,
    type ProfileContent,
    type LivingProfileStatus,
    type ProfilePatch,
    type SyncStatus,
  } from "$lib/api/tauri";
  import { emit } from "@tauri-apps/api/event";
  import { open } from "@tauri-apps/plugin-shell";
  import { canLivingProfile, canSync, canExport } from "$lib/stores/subscription.svelte";
  import { setPatchCount } from "$lib/stores/patches.svelte";
  import { refresh as refreshSubscription } from "$lib/stores/subscription.svelte";
  import { friendlyError } from "$lib/utils/errors";
  import { marked } from "marked";
  import DOMPurify from "dompurify";
  import LoadingSpinner from "./LoadingSpinner.svelte";

  marked.setOptions({ breaks: true });

  function renderMarkdown(content: string): string {
    return DOMPurify.sanitize(marked.parse(content) as string);
  }

  let overview = $state<ProfileOverview | null>(null);
  let profile = $state<ProfileContent | null>(null);
  let activeTab = $state("core");
  let isEditing = $state(false);
  let editContent = $state("");
  let isSaving = $state(false);
  let error = $state("");
  let saveSuccess = $state(false);

  // Living profile state
  let livingStatus = $state<LivingProfileStatus | null>(null);
  let patches = $state<ProfilePatch[]>([]);
  let isUploading = $state(false);
  let isRefreshing = $state(false);
  let refreshMessage = $state("");

  // Sync state
  let syncStatus = $state<SyncStatus | null>(null);
  let isSyncing = $state(false);
  let syncMessage = $state("");

  // Export state (server profiles)
  let isExporting = $state(false);
  let isDevMode = $state(false);

  // Empty state
  let showManualCreate = $state(false);

  let displayContent = $derived(
    activeTab === "core"
      ? profile?.core_identity ?? ""
      : profile?.contexts[activeTab] ?? "",
  );

  $effect(() => {
    loadProfile();
  });

  async function loadProfile() {
    try {
      const settings = await getSettings();
      isDevMode = settings.debug_mode ?? false;
      overview = await getProfileOverview();
      if (overview.exists && !overview.is_server) {
        profile = await readProfileContent();
      }
      // Load living profile status
      try {
        livingStatus = await getLivingProfileStatus();
        if (livingStatus.enabled) {
          const p = await getProfilePatches();
          patches = p;
          setPatchCount(p.length);
        }
      } catch { /* not logged in or not available */ }
      // Load sync status
      try {
        syncStatus = await getSyncStatus();
      } catch { /* not logged in or not available */ }
    } catch (e) {
      error = friendlyError(e);
    }
  }

  async function handleCreateProfile() {
    if (!editContent.trim()) return;
    isSaving = true;
    error = "";
    try {
      await saveProfileEdit({ coreIdentity: editContent.trim() });
      await loadProfile();
    } catch (e) {
      error = friendlyError(e);
    } finally {
      isSaving = false;
    }
  }

  function startEditing() {
    editContent = displayContent;
    isEditing = true;
    saveSuccess = false;
  }

  function cancelEditing() {
    isEditing = false;
    editContent = "";
    error = "";
  }

  async function handleSave() {
    isSaving = true;
    error = "";
    saveSuccess = false;
    try {
      if (activeTab === "core") {
        await saveProfileEdit({ coreIdentity: editContent });
      } else {
        await saveProfileEdit({
          coreIdentity: profile?.core_identity ?? "",
          contextFormat: activeTab,
          contextContent: editContent,
        });
      }
      isEditing = false;
      saveSuccess = true;
      await loadProfile();
    } catch (e) {
      error = friendlyError(e);
    } finally {
      isSaving = false;
    }
  }

  function switchTab(tab: string) {
    if (isEditing) cancelEditing();
    activeTab = tab;
    saveSuccess = false;
    refreshMessage = "";
  }

  async function handleToggleLiving() {
    if (!livingStatus) return;
    error = "";
    try {
      await setLivingProfileEnabled(!livingStatus.enabled);
      livingStatus = await getLivingProfileStatus();
    } catch (e) {
      error = friendlyError(e);
    }
  }

  async function handleUploadAndRefresh() {
    error = "";
    refreshMessage = "";
    isUploading = true;
    try {
      const count = await uploadEditLog();
      isUploading = false;
      if (count === 0) {
        refreshMessage = "No edits to upload yet. Keep writing!";
        return;
      }
      isRefreshing = true;
      const result = await refreshLivingProfile();
      patches = result.patches;
      setPatchCount(patches.length);
      refreshMessage = `Analyzed ${result.entries_analyzed} edits, found ${result.signals_found} signals, generated ${result.patches.length} patches.`;
    } catch (e) {
      error = friendlyError(e);
    } finally {
      isUploading = false;
      isRefreshing = false;
    }
  }

  async function handleApprovePatch(patchId: string) {
    error = "";
    try {
      await approveProfilePatch(patchId);
      patches = patches.filter((p) => p.patch_id !== patchId);
      setPatchCount(patches.length);
    } catch (e) {
      error = friendlyError(e);
    }
  }

  async function handleRejectPatch(patchId: string) {
    error = "";
    try {
      await rejectProfilePatch(patchId);
      patches = patches.filter((p) => p.patch_id !== patchId);
      setPatchCount(patches.length);
    } catch (e) {
      error = friendlyError(e);
    }
  }

  async function handleSyncUp() {
    isSyncing = true;
    syncMessage = "";
    error = "";
    try {
      const result = await syncProfileUp();
      syncMessage = result;
      syncStatus = await getSyncStatus();
    } catch (e) {
      error = friendlyError(e);
    } finally {
      isSyncing = false;
    }
  }

  async function handleSyncDown() {
    isSyncing = true;
    syncMessage = "";
    error = "";
    try {
      const result = await syncProfileDown();
      syncMessage = result;
      syncStatus = await getSyncStatus();
      await loadProfile(); // Reload profile after pull
    } catch (e) {
      error = friendlyError(e);
    } finally {
      isSyncing = false;
    }
  }

  async function handleExport() {
    isExporting = true;
    error = "";
    try {
      await exportProfile();
      await loadProfile();
    } catch (e) {
      error = friendlyError(e);
    } finally {
      isExporting = false;
    }
  }

  async function handleUpgrade(target: string) {
    error = "";
    try {
      const result = await createCheckout(target);
      if (result.checkout_url === "dev://granted") {
        await refreshSubscription();
      } else {
        await open(result.checkout_url);
      }
    } catch (e) {
      error = friendlyError(e);
    }
  }
</script>

<div class="flex flex-col gap-3 h-full p-4 overflow-hidden animate-fade-in-up">
  {#if !overview}
    <div class="flex items-center justify-center h-full">
      <LoadingSpinner />
    </div>
  {:else if !overview.exists}
    {#if !showManualCreate}
      <!-- Empty state: no profile -->
      <div class="flex-1 flex flex-col items-center justify-center gap-6 py-8 animate-fade-in-up">
        <!-- Empty loom frame illustration -->
        <svg class="w-[100px] h-[90px] opacity-55" viewBox="0 0 100 90" fill="none">
          <!-- Loom frame -->
          <rect x="15" y="10" width="70" height="70" rx="3" stroke="var(--color-border)" stroke-width="1.5" fill="none"/>
          <!-- Top beam -->
          <line x1="15" y1="18" x2="85" y2="18" stroke="var(--color-primary)" stroke-width="1.5" opacity="0.4"/>
          <!-- Bottom beam -->
          <line x1="15" y1="72" x2="85" y2="72" stroke="var(--color-primary)" stroke-width="1.5" opacity="0.4"/>
          <!-- Faint warp threads (waiting) -->
          <line x1="30" y1="18" x2="30" y2="72" stroke="var(--color-secondary)" stroke-width="0.75" opacity="0.2" stroke-dasharray="3 4"/>
          <line x1="42" y1="18" x2="42" y2="72" stroke="var(--color-secondary)" stroke-width="0.75" opacity="0.15" stroke-dasharray="3 4"/>
          <line x1="54" y1="18" x2="54" y2="72" stroke="var(--color-secondary)" stroke-width="0.75" opacity="0.2" stroke-dasharray="3 4"/>
          <line x1="66" y1="18" x2="66" y2="72" stroke="var(--color-secondary)" stroke-width="0.75" opacity="0.15" stroke-dasharray="3 4"/>
          <!-- Accent thread hint -->
          <path d="M28 40 C35 38, 45 42, 52 39 C59 36, 67 41, 72 39" stroke="var(--color-accent)" stroke-width="1" stroke-linecap="round" opacity="0.25"/>
        </svg>

        <div class="text-center max-w-[280px]">
          <p class="text-sm font-medium text-foreground">Your voice profile will live here</p>
          <p class="text-[10px] text-muted leading-relaxed mt-1.5">
            AI extraction analyzes your real writing to capture sentence patterns, vocabulary, tone, and format-specific style. The recommended way to start.
          </p>
        </div>

        <div class="flex flex-col items-center gap-2">
          <button
            onclick={() => emit("navigate", "extract")}
            class="px-5 py-2 text-xs font-semibold bg-primary text-white hover:bg-primary-hover transition-colors cursor-pointer rounded-md"
          >
            Extract your voice
          </button>
          <button
            onclick={() => { showManualCreate = true; }}
            class="text-[10px] text-secondary font-medium cursor-pointer hover:text-foreground"
          >
            Or describe it manually
          </button>
        </div>
      </div>
    {:else}
      <!-- Manual creation form -->
      <div class="flex flex-col gap-3 h-full animate-fade-in-up">
        <div class="flex items-center justify-between">
          <div>
            <p class="text-sm font-medium text-foreground">Describe your voice</p>
            <p class="text-[10px] text-muted leading-relaxed mt-1">
              Tone, word choices, sentence length, quirks.
            </p>
          </div>
          <button
            onclick={() => { showManualCreate = false; editContent = ""; }}
            class="text-[10px] text-muted cursor-pointer hover:text-foreground"
          >Back</button>
        </div>

        <div class="flex-1 flex flex-col min-h-0">
          <textarea
            bind:value={editContent}
            class="flex-1 p-3 text-xs leading-relaxed border border-border bg-surface text-foreground resize-none placeholder-muted rounded-md focus:outline-none focus:border-secondary"
            placeholder={"Example:\n\nI write casually and directly. Short sentences. I use contractions, avoid jargon, and get to the point fast. I'm opinionated but not aggressive — more like a friend giving honest advice. I occasionally use humor and rhetorical questions."}
          ></textarea>
        </div>

        <!-- AI extraction nudge -->
        <div class="p-2 bg-tint border border-secondary/20 rounded-lg flex flex-col gap-1.5">
          <p class="text-[10px] text-muted leading-relaxed">
            <span class="text-secondary font-medium">AI Extraction</span> captures more detail from your real writing.
          </p>
          <div class="flex gap-2 items-center">
            <button
              onclick={() => handleUpgrade("extraction")}
              class="text-[10px] text-secondary font-medium cursor-pointer hover:text-foreground uppercase tracking-wide"
            >
              One-time $29
            </button>
            <span class="text-[10px] text-muted">or</span>
            <button
              onclick={() => handleUpgrade("pro")}
              class="text-[10px] text-secondary font-medium cursor-pointer hover:text-foreground uppercase tracking-wide"
            >
              Included with Pro
            </button>
          </div>
        </div>

        <button
          onclick={handleCreateProfile}
          disabled={!editContent.trim() || isSaving}
          class="w-full py-2.5 px-4 text-sm font-semibold transition-colors cursor-pointer rounded-md
            {!editContent.trim() || isSaving
              ? 'bg-surface text-muted border border-border cursor-not-allowed opacity-50'
              : 'bg-primary text-white hover:bg-primary-hover'}"
        >
          {isSaving ? "Saving..." : "Save Profile"}
        </button>

        {#if error}
          <div class="p-2 bg-tint border border-border rounded-lg text-xs text-muted leading-relaxed">
            {error}
          </div>
        {/if}
      </div>
    {/if}
  {:else if overview.is_server}
    <!-- Server profile — metadata only -->
    <div class="flex flex-col gap-3 h-full">
      <div class="p-3 bg-surface border border-secondary/20 rounded-lg">
        <p class="text-sm font-medium text-foreground">Voice profile on Noren servers</p>
        <p class="text-[10px] text-muted mt-1">
          Your extracted profile is securely stored on Noren servers and used automatically when generating text.
        </p>
      </div>

      {#if canLivingProfile() && activeTab !== "living"}
        <div class="flex items-center gap-1.5">
          <div class="w-[5px] h-[5px] rounded-full bg-secondary animate-voice-pulse"></div>
          <span class="text-[10px] text-secondary font-medium">Living Profile</span>
        </div>
      {/if}

      {#if patches.length > 0 && activeTab !== "living"}
        <div class="flex items-center gap-2 p-2 bg-tint border border-secondary/20 rounded-lg shrink-0">
          <p class="flex-1 text-[10px] text-muted leading-relaxed">
            {patches.length} suggested refinement{patches.length !== 1 ? "s" : ""}.
            <button
              onclick={() => switchTab("living")}
              class="text-secondary font-medium cursor-pointer hover:text-foreground"
            >Review</button>
          </p>
        </div>
      {/if}

      {#if overview.formats.length > 0}
        <div class="p-3 bg-surface border border-border rounded-lg">
          <span class="text-[10px] font-medium text-muted uppercase tracking-wide">Formats</span>
          <div class="flex gap-1.5 mt-1.5 flex-wrap">
            {#each overview.formats as fmt}
              <span class="px-2 py-0.5 text-xs bg-tint border border-border rounded text-secondary">{fmt}</span>
            {/each}
          </div>
        </div>
      {/if}

      <!-- Debug inspector -->
      {#if isDevMode}
        {#if profile}
          <div class="flex gap-1 shrink-0">
            <button
              onclick={() => { activeTab = "core"; }}
              class="px-2.5 py-1 text-xs cursor-pointer uppercase tracking-wide rounded-md
                {activeTab === 'core' ? 'bg-primary text-white font-medium' : 'bg-surface text-muted border border-border hover:border-secondary'}"
            >Core</button>
            {#each Object.keys(profile.contexts || {}) as ctx}
              <button
                onclick={() => { activeTab = ctx; }}
                class="px-2.5 py-1 text-xs cursor-pointer uppercase tracking-wide rounded-md
                  {activeTab === ctx ? 'bg-primary text-white font-medium' : 'bg-surface text-muted border border-border hover:border-secondary'}"
              >{ctx}</button>
            {/each}
          </div>
          <div class="flex-1 min-h-0 overflow-y-auto">
            <pre class="p-3 text-xs leading-relaxed text-foreground bg-surface border border-border rounded-lg whitespace-pre-wrap">{displayContent}</pre>
          </div>
        {:else}
          <button
            onclick={async () => {
              try {
                await exportProfile();
                profile = await readProfileContent();
              } catch (e) {
                error = friendlyError(e);
              }
            }}
            class="px-4 py-2 text-xs font-medium bg-surface border border-border text-muted hover:border-secondary hover:text-foreground rounded-md cursor-pointer transition-colors"
          >
            Inspect profile
          </button>
        {/if}
      {/if}

      <!-- Living Profile tab -->
      <div class="flex gap-1 shrink-0">
        <button
          onclick={() => switchTab("living")}
          class="px-2.5 py-1 text-xs whitespace-nowrap transition-colors cursor-pointer uppercase tracking-wide rounded-md
            {activeTab === 'living'
              ? 'bg-secondary text-white font-medium'
              : 'bg-surface text-muted border border-border hover:border-secondary hover:text-foreground'}"
        >
          Living Profile
          {#if !canLivingProfile()}
            <span class="ml-0.5 text-[8px] {activeTab === 'living' ? 'text-white/70' : 'text-secondary'} font-medium">PRO</span>
          {/if}
        </button>
      </div>

      {#if activeTab === "living"}
        {#if canLivingProfile()}
        <div class="flex-1 flex flex-col gap-3 overflow-y-auto">
          <div class="p-3 bg-surface border border-border rounded-lg">
            <div class="flex items-center justify-between">
              <div>
                <span class="text-xs font-medium text-foreground">Edit tracking</span>
                <p class="text-[10px] text-muted mt-0.5">Track edits to improve your profile over time.</p>
              </div>
              <button
                onclick={handleToggleLiving}
                class="px-3 py-1 text-[10px] uppercase tracking-wide cursor-pointer rounded-md transition-colors
                  {livingStatus?.enabled
                    ? 'bg-secondary text-white font-medium'
                    : 'bg-surface text-muted border border-border hover:border-secondary'}"
              >
                {livingStatus?.enabled ? "On" : "Off"}
              </button>
            </div>
            {#if livingStatus?.enabled}
              <p class="text-[10px] text-secondary mt-2">
                {livingStatus.edit_count} edits tracked locally
              </p>
            {/if}
          </div>

          {#if livingStatus?.enabled}
            <button
              onclick={handleUploadAndRefresh}
              disabled={isUploading || isRefreshing}
              class="w-full py-2 text-xs font-medium bg-secondary text-white hover:bg-secondary/90 transition-colors cursor-pointer disabled:opacity-50 rounded-md"
            >
              {#if isUploading}
                <span class="inline-flex items-center gap-1"><LoadingSpinner /> Uploading edits...</span>
              {:else if isRefreshing}
                <span class="inline-flex items-center gap-1"><LoadingSpinner /> Analyzing patterns...</span>
              {:else}
                Refresh profile from edits
              {/if}
            </button>

            {#if refreshMessage}
              <p class="text-[10px] text-muted">{refreshMessage}</p>
            {/if}

            {#if patches.length > 0}
              <div>
                <span class="block text-xs font-medium text-muted mb-2 uppercase tracking-wide">
                  Suggested changes ({patches.length})
                </span>
                <div class="flex flex-col gap-2">
                  {#each patches as patch}
                    <div class="p-3 bg-surface border border-border rounded-lg">
                      <p class="text-xs text-foreground">{patch.description}</p>
                      {#if patch.original_text}
                        <p class="text-[10px] text-muted mt-1 font-mono line-through">{patch.original_text}</p>
                      {/if}
                      {#if patch.new_text}
                        <p class="text-[10px] text-secondary mt-1 font-mono">{patch.new_text}</p>
                      {/if}
                      <div class="flex items-center justify-between mt-2">
                        <span class="text-[10px] text-muted">
                          {patch.section} &middot; {Math.round(patch.confidence * 100)}% confidence
                        </span>
                        <div class="flex gap-1">
                          <button
                            onclick={() => handleRejectPatch(patch.patch_id)}
                            class="px-2 py-0.5 text-[10px] border border-border text-muted hover:text-error hover:border-error cursor-pointer rounded transition-colors"
                          >
                            Reject
                          </button>
                          <button
                            onclick={() => handleApprovePatch(patch.patch_id)}
                            class="px-2 py-0.5 text-[10px] bg-secondary text-white hover:bg-secondary/90 cursor-pointer rounded transition-colors font-medium"
                          >
                            Approve
                          </button>
                        </div>
                      </div>
                    </div>
                  {/each}
                </div>
              </div>
            {:else if !isRefreshing && !isUploading}
              <p class="text-[10px] text-muted text-center py-4">
                No pending suggestions. Keep writing and refresh periodically.
              </p>
            {/if}
          {:else}
            <p class="text-[10px] text-muted leading-relaxed">
              Enable edit tracking to let Noren learn from how you modify generated text.
              Your edits are stored locally and only uploaded when you choose to refresh.
            </p>
          {/if}
        </div>
        {:else}
        <div class="flex-1 flex flex-col items-center justify-center gap-3 py-8">
          <div class="p-4 bg-tint border border-secondary/20 rounded-lg text-center max-w-[260px]">
            <p class="text-xs font-medium text-secondary">Living Profile</p>
            <p class="text-[10px] text-muted mt-1 leading-relaxed">
              Your profile evolves as you write. Noren tracks your edits and suggests refinements automatically.
            </p>
            <button
              onclick={() => handleUpgrade("pro")}
              class="mt-3 px-4 py-1.5 text-[10px] font-medium bg-secondary text-white hover:bg-secondary/90 transition-colors cursor-pointer rounded uppercase tracking-wide"
            >
              Upgrade to Pro
            </button>
          </div>
        </div>
        {/if}
      {/if}

      {#if canLivingProfile() && activeTab !== "living"}
        <p class="text-[10px] text-muted shrink-0">Your profile refines automatically as you write.</p>
      {/if}

      <div class="flex items-center justify-between shrink-0 mt-auto">
        <span class="text-[10px] text-muted">Stored on Noren servers</span>
        {#if canExport()}
          <button
            onclick={handleExport}
            disabled={isExporting}
            class="px-3 py-1.5 text-xs border border-border hover:border-secondary transition-colors cursor-pointer text-muted hover:text-foreground disabled:opacity-50 rounded-md"
          >
            {isExporting ? "Exporting..." : "Export to disk"}
          </button>
        {:else}
          <button
            onclick={() => handleUpgrade("export")}
            class="px-3 py-1.5 text-xs border border-border hover:border-secondary transition-colors cursor-pointer text-muted hover:text-foreground rounded-md"
            title="One-time purchase to export your profile"
          >
            Export to disk <span class="text-[8px] text-secondary font-medium">$</span>
          </button>
        {/if}
      </div>

      {#if error}
        <div class="p-2 bg-tint border border-border rounded-lg text-xs text-muted leading-relaxed shrink-0">
          {error}
        </div>
      {/if}
    </div>
  {:else}
    <!-- Tabs -->
    <div class="flex flex-wrap gap-1 shrink-0">
      <button
        onclick={() => switchTab("core")}
        class="px-2.5 py-1 text-xs whitespace-nowrap transition-colors cursor-pointer uppercase tracking-wide rounded-md
          {activeTab === 'core'
            ? 'bg-primary text-white font-medium'
            : 'bg-surface text-muted border border-border hover:border-secondary hover:text-foreground'}"
      >
        Core Identity
      </button>
      {#each overview.formats as fmt}
        <button
          onclick={() => switchTab(fmt)}
          class="px-2.5 py-1 text-xs whitespace-nowrap transition-colors cursor-pointer uppercase tracking-wide rounded-md
            {activeTab === fmt
              ? 'bg-primary text-white font-medium'
              : 'bg-surface text-muted border border-border hover:border-secondary hover:text-foreground'}"
        >
          {fmt}
        </button>
      {/each}
      <button
        onclick={() => switchTab("living")}
        class="px-2.5 py-1 text-xs whitespace-nowrap transition-colors cursor-pointer uppercase tracking-wide rounded-md relative
          {activeTab === 'living'
            ? 'bg-secondary text-white font-medium'
            : 'bg-surface text-muted border border-border hover:border-secondary hover:text-foreground'}"
      >
        Living
        {#if !canLivingProfile()}
          <span class="ml-0.5 text-[8px] {activeTab === 'living' ? 'text-white/70' : 'text-secondary'} font-medium">PRO</span>
        {/if}
      </button>
    </div>

    {#if canLivingProfile() && activeTab !== "living"}
      <div class="flex items-center gap-1.5 shrink-0">
        <div class="w-[5px] h-[5px] rounded-full bg-secondary animate-voice-pulse"></div>
        <span class="text-[10px] text-secondary font-medium">Living Profile</span>
      </div>
    {/if}

    {#if patches.length > 0 && activeTab !== "living"}
      <div class="flex items-center gap-2 p-2 bg-tint border border-secondary/20 rounded-lg shrink-0">
        <p class="flex-1 text-[10px] text-muted leading-relaxed">
          {patches.length} suggested refinement{patches.length !== 1 ? "s" : ""}.
          <button
            onclick={() => switchTab("living")}
            class="text-secondary font-medium cursor-pointer hover:text-foreground"
          >Review</button>
        </p>
      </div>
    {/if}

    <!-- Content -->
    <div class="flex-1 flex flex-col min-h-0">
      {#if activeTab === "living"}
        {#if canLivingProfile()}
        <!-- Living Profile panel -->
        <div class="flex-1 flex flex-col gap-3 overflow-y-auto">
          <!-- Opt-in toggle -->
          <div class="p-3 bg-surface border border-border rounded-lg">
            <div class="flex items-center justify-between">
              <div>
                <span class="text-xs font-medium text-foreground">Edit tracking</span>
                <p class="text-[10px] text-muted mt-0.5">
                  Track edits you make to generated text so your profile can improve over time.
                </p>
              </div>
              <button
                onclick={handleToggleLiving}
                class="px-3 py-1 text-[10px] uppercase tracking-wide cursor-pointer rounded-md transition-colors
                  {livingStatus?.enabled
                    ? 'bg-secondary text-white font-medium'
                    : 'bg-surface text-muted border border-border hover:border-secondary'}"
              >
                {livingStatus?.enabled ? "On" : "Off"}
              </button>
            </div>
            {#if livingStatus?.enabled}
              <p class="text-[10px] text-secondary mt-2">
                {livingStatus.edit_count} edits tracked locally
              </p>
            {/if}
          </div>

          {#if livingStatus?.enabled}
            <!-- Upload & Refresh -->
            <button
              onclick={handleUploadAndRefresh}
              disabled={isUploading || isRefreshing}
              class="w-full py-2 text-xs font-medium bg-secondary text-white hover:bg-secondary/90 transition-colors cursor-pointer disabled:opacity-50 rounded-md"
            >
              {#if isUploading}
                <span class="inline-flex items-center gap-1"><LoadingSpinner /> Uploading edits...</span>
              {:else if isRefreshing}
                <span class="inline-flex items-center gap-1"><LoadingSpinner /> Analyzing patterns...</span>
              {:else}
                Refresh profile from edits
              {/if}
            </button>

            {#if refreshMessage}
              <p class="text-[10px] text-muted">{refreshMessage}</p>
            {/if}

            <!-- Pending patches -->
            {#if patches.length > 0}
              <div>
                <span class="block text-xs font-medium text-muted mb-2 uppercase tracking-wide">
                  Suggested changes ({patches.length})
                </span>
                <div class="flex flex-col gap-2">
                  {#each patches as patch}
                    <div class="p-3 bg-surface border border-border rounded-lg">
                      <p class="text-xs text-foreground">{patch.description}</p>
                      {#if patch.original_text}
                        <p class="text-[10px] text-muted mt-1 font-mono line-through">{patch.original_text}</p>
                      {/if}
                      {#if patch.new_text}
                        <p class="text-[10px] text-secondary mt-1 font-mono">{patch.new_text}</p>
                      {/if}
                      <div class="flex items-center justify-between mt-2">
                        <span class="text-[10px] text-muted">
                          {patch.section} &middot; {Math.round(patch.confidence * 100)}% confidence
                        </span>
                        <div class="flex gap-1">
                          <button
                            onclick={() => handleRejectPatch(patch.patch_id)}
                            class="px-2 py-0.5 text-[10px] border border-border text-muted hover:text-error hover:border-error cursor-pointer rounded transition-colors"
                          >
                            Reject
                          </button>
                          <button
                            onclick={() => handleApprovePatch(patch.patch_id)}
                            class="px-2 py-0.5 text-[10px] bg-secondary text-white hover:bg-secondary/90 cursor-pointer rounded transition-colors font-medium"
                          >
                            Approve
                          </button>
                        </div>
                      </div>
                    </div>
                  {/each}
                </div>
              </div>
            {:else if !isRefreshing && !isUploading}
              <p class="text-[10px] text-muted text-center py-4">
                No pending suggestions. Keep writing and refresh periodically.
              </p>
            {/if}
          {:else}
            <p class="text-[10px] text-muted leading-relaxed">
              Enable edit tracking to let Noren learn from how you modify generated text.
              Your edits are stored locally and only uploaded when you choose to refresh.
            </p>
          {/if}
        </div>
        {:else}
        <!-- Living Profile locked -->
        <div class="flex-1 flex flex-col items-center justify-center gap-3 py-8">
          <div class="p-4 bg-tint border border-secondary/20 rounded-lg text-center max-w-[260px]">
            <p class="text-xs font-medium text-secondary">Living Profile</p>
            <p class="text-[10px] text-muted mt-1 leading-relaxed">
              Your profile evolves as you write. Noren tracks your edits and suggests refinements automatically.
            </p>
            <button
              onclick={() => handleUpgrade("pro")}
              class="mt-3 px-4 py-1.5 text-[10px] font-medium bg-secondary text-white hover:bg-secondary/90 transition-colors cursor-pointer rounded uppercase tracking-wide"
            >
              Upgrade to Pro
            </button>
          </div>
        </div>
        {/if}
      {:else if isEditing}
        <textarea
          bind:value={editContent}
          class="flex-1 p-3 text-xs leading-relaxed border border-border bg-surface text-foreground resize-none rounded-md focus:outline-none focus:border-secondary font-mono"
        ></textarea>
      {:else}
        <div class="flex-1 p-3 bg-surface border border-border rounded-lg overflow-y-auto">
          <div class="prose-profile text-xs text-foreground leading-relaxed selectable">{@html renderMarkdown(displayContent)}</div>
        </div>
      {/if}
    </div>

    <!-- Upgrade nudge for manual-only profiles (no format contexts) -->
    {#if overview.formats.length === 0 && activeTab === "core" && !isEditing}
      <div class="p-2 bg-tint border border-secondary/15 rounded-lg shrink-0 flex flex-col gap-1.5">
        <p class="text-[10px] text-muted leading-relaxed">
          Your profile covers the basics. <span class="text-secondary font-medium">AI extraction</span> adds format-specific contexts and vocabulary analysis.
        </p>
        <div class="flex gap-2 items-center">
          <button
            onclick={() => handleUpgrade("extraction")}
            class="text-[10px] text-secondary font-medium cursor-pointer hover:text-foreground uppercase tracking-wide"
          >
            One-time $29
          </button>
          <span class="text-[10px] text-muted">or</span>
          <button
            onclick={() => handleUpgrade("pro")}
            class="text-[10px] text-secondary font-medium cursor-pointer hover:text-foreground uppercase tracking-wide"
          >
            Included with Pro
          </button>
        </div>
      </div>
    {/if}

    {#if canLivingProfile() && activeTab !== "living"}
      <p class="text-[10px] text-muted shrink-0">Your profile refines automatically as you write.</p>
    {/if}

    <!-- Actions -->
    {#if activeTab !== "living"}
      <div class="flex items-center justify-between shrink-0">
        <span class="text-[10px] text-muted">
          {#if syncMessage}
            <span class="text-signal">{syncMessage}</span>
          {:else if saveSuccess}
            <span class="text-signal">Saved</span>
          {:else}
            {activeTab === "core" ? "Core Identity" : activeTab} &middot; {displayContent.split("\n").length} lines
            {#if syncStatus?.has_remote}
              &middot; <span class="text-secondary">synced v{syncStatus.remote_version}</span>
            {/if}
          {/if}
        </span>
        <div class="flex gap-1">
          {#if isEditing}
            <button
              onclick={cancelEditing}
              class="px-3 py-1.5 text-xs border border-border hover:border-secondary transition-colors cursor-pointer text-muted hover:text-foreground rounded-md"
            >
              Cancel
            </button>
            <button
              onclick={handleSave}
              disabled={isSaving}
              class="px-3 py-1.5 text-xs bg-primary text-white hover:bg-primary-hover transition-colors cursor-pointer disabled:opacity-50 rounded-md font-medium"
            >
              {isSaving ? "Saving..." : "Save"}
            </button>
          {:else}
            <button
              onclick={startEditing}
              class="px-3 py-1.5 text-xs border border-border hover:border-secondary transition-colors cursor-pointer text-muted hover:text-foreground rounded-md"
            >
              Edit
            </button>
            {#if canSync()}
              <button
                onclick={handleSyncUp}
                disabled={isSyncing}
                class="px-2 py-1.5 text-[10px] border border-border hover:border-secondary transition-colors cursor-pointer text-muted hover:text-foreground disabled:opacity-50 rounded-md uppercase tracking-wide"
                title="Push profile to cloud"
              >
                {isSyncing ? "..." : "Push"}
              </button>
              {#if syncStatus?.has_remote}
                <button
                  onclick={handleSyncDown}
                  disabled={isSyncing}
                  class="px-2 py-1.5 text-[10px] border border-border hover:border-secondary transition-colors cursor-pointer text-muted hover:text-foreground disabled:opacity-50 rounded-md uppercase tracking-wide"
                  title="Pull profile from cloud"
                >
                  Pull
                </button>
              {/if}
            {:else}
              <button
                onclick={() => handleUpgrade("pro")}
                class="px-2 py-1.5 text-[10px] border border-border hover:border-secondary transition-colors cursor-pointer text-muted hover:text-foreground rounded-md uppercase tracking-wide"
                title="Sync requires Pro"
              >
                Sync <span class="text-[8px] text-secondary font-medium">PRO</span>
              </button>
            {/if}
          {/if}
        </div>
      </div>
    {/if}

    <!-- Error -->
    {#if error}
      <div class="p-2 bg-tint border border-border rounded-lg text-xs text-muted leading-relaxed shrink-0">
        {error}
      </div>
    {/if}
  {/if}
</div>
