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
    type ProfileOverview,
    type ProfileContent,
    type LivingProfileStatus,
    type ProfilePatch,
    type SyncStatus,
  } from "$lib/api/tauri";
  import { emit } from "@tauri-apps/api/event";
  import { friendlyError } from "$lib/utils/errors";
  import LoadingSpinner from "./LoadingSpinner.svelte";

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
    } catch (e) {
      error = friendlyError(e);
    }
  }

  async function handleRejectPatch(patchId: string) {
    error = "";
    try {
      await rejectProfilePatch(patchId);
      patches = patches.filter((p) => p.patch_id !== patchId);
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
</script>

<div class="flex flex-col gap-3 h-full p-4 overflow-hidden animate-fade-in-up">
  {#if !overview}
    <div class="flex items-center justify-center h-full">
      <LoadingSpinner />
    </div>
  {:else if !overview.exists}
    <!-- No profile — manual creation -->
    <div class="flex flex-col gap-3 h-full">
      <div>
        <p class="text-sm font-medium text-foreground">Create your voice profile</p>
        <p class="text-[10px] text-muted leading-relaxed mt-1">
          Describe how you write — tone, word choices, sentence length, and any quirks.
        </p>
      </div>

      <div class="flex-1 flex flex-col min-h-0">
        <textarea
          bind:value={editContent}
          class="flex-1 p-3 text-xs leading-relaxed border border-border bg-surface text-foreground resize-none placeholder-muted rounded-md focus:outline-none focus:border-secondary"
          placeholder={"Example:\n\nI write casually and directly. Short sentences. I use contractions, avoid jargon, and get to the point fast. I'm opinionated but not aggressive — more like a friend giving honest advice. I occasionally use humor and rhetorical questions."}
        ></textarea>
      </div>

      <!-- Pro nudge -->
      <div class="p-2 bg-tint border border-secondary/20 rounded-md flex items-start justify-between gap-2">
        <p class="text-[10px] text-muted leading-relaxed">
          <span class="text-secondary font-medium">Noren Pro</span> analyzes your real writing to build a much richer profile — sentence patterns, vocabulary, rhetorical style, and format-specific contexts. No API key needed.
        </p>
        <button
          onclick={() => emit("navigate", "settings")}
          class="text-[10px] text-secondary hover:text-foreground font-medium cursor-pointer whitespace-nowrap shrink-0 uppercase tracking-wide"
        >
          Upgrade
        </button>
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
        <div class="p-2 bg-tint border border-border rounded-md text-xs text-muted leading-relaxed">
          {error}
        </div>
      {/if}
    </div>
  {:else if overview.is_server}
    <!-- Server profile — metadata only -->
    <div class="flex flex-col gap-3 h-full">
      <div class="p-3 bg-surface border border-secondary/20 rounded-md">
        <p class="text-sm font-medium text-foreground">Voice profile on Noren servers</p>
        <p class="text-[10px] text-muted mt-1">
          Your extracted profile is securely stored on Noren servers and used automatically when generating text.
        </p>
      </div>

      {#if overview.formats.length > 0}
        <div class="p-3 bg-surface border border-border rounded-md">
          <span class="text-[10px] font-medium text-muted uppercase tracking-wide">Formats</span>
          <div class="flex gap-1.5 mt-1.5 flex-wrap">
            {#each overview.formats as fmt}
              <span class="px-2 py-0.5 text-xs bg-tint border border-border rounded text-secondary">{fmt}</span>
            {/each}
          </div>
        </div>
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
        </button>
      </div>

      {#if activeTab === "living"}
        <div class="flex-1 flex flex-col gap-3 overflow-y-auto">
          <div class="p-3 bg-surface border border-border rounded-md">
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
          </div>
        </div>
      {/if}

      <div class="flex items-center justify-between shrink-0 mt-auto">
        <span class="text-[10px] text-muted">Stored on Noren servers</span>
        <button
          onclick={handleExport}
          disabled={isExporting}
          class="px-3 py-1.5 text-xs border border-border hover:border-secondary transition-colors cursor-pointer text-muted hover:text-foreground disabled:opacity-50 rounded-md"
        >
          {isExporting ? "Exporting..." : "Export to disk"}
        </button>
      </div>

      {#if error}
        <div class="p-2 bg-tint border border-border rounded-md text-xs text-muted leading-relaxed shrink-0">
          {error}
        </div>
      {/if}
    </div>
  {:else}
    <!-- Tabs -->
    <div class="flex gap-1 overflow-x-auto shrink-0">
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
        class="px-2.5 py-1 text-xs whitespace-nowrap transition-colors cursor-pointer uppercase tracking-wide rounded-md
          {activeTab === 'living'
            ? 'bg-secondary text-white font-medium'
            : 'bg-surface text-muted border border-border hover:border-secondary hover:text-foreground'}"
      >
        Living
      </button>
    </div>

    <!-- Content -->
    <div class="flex-1 flex flex-col min-h-0">
      {#if activeTab === "living"}
        <!-- Living Profile panel -->
        <div class="flex-1 flex flex-col gap-3 overflow-y-auto">
          <!-- Opt-in toggle -->
          <div class="p-3 bg-surface border border-border rounded-md">
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
                    <div class="p-3 bg-surface border border-border rounded-md">
                      <p class="text-xs text-foreground">{patch.description}</p>
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
      {:else if isEditing}
        <textarea
          bind:value={editContent}
          class="flex-1 p-3 text-xs leading-relaxed border border-border bg-surface text-foreground resize-none rounded-md focus:outline-none focus:border-secondary font-mono"
        ></textarea>
      {:else}
        <div class="flex-1 p-3 bg-surface border border-border rounded-md overflow-y-auto">
          <pre class="text-xs text-foreground whitespace-pre-wrap leading-relaxed font-mono">{displayContent}</pre>
        </div>
      {/if}
    </div>

    <!-- Upgrade nudge for manual-only profiles (no format contexts) -->
    {#if overview.formats.length === 0 && activeTab === "core" && !isEditing}
      <div class="p-2 bg-tint border border-secondary/15 rounded-md shrink-0 flex items-start justify-between gap-2">
        <p class="text-[10px] text-muted leading-relaxed">
          Your profile covers the basics. <span class="text-secondary font-medium">Noren Pro</span> adds AI extraction, format-specific contexts, and vocabulary analysis for significantly more accurate voice matching.
        </p>
        <button
          onclick={() => emit("navigate", "settings")}
          class="text-[10px] text-secondary hover:text-foreground font-medium cursor-pointer whitespace-nowrap shrink-0 uppercase tracking-wide"
        >
          Upgrade
        </button>
      </div>
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
          {/if}
        </div>
      </div>
    {/if}

    <!-- Error -->
    {#if error}
      <div class="p-2 bg-tint border border-border rounded-md text-xs text-muted leading-relaxed shrink-0">
        {error}
      </div>
    {/if}
  {/if}
</div>
