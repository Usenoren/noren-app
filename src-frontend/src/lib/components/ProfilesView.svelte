<script lang="ts">
  import {
    getProfileOverview,
    readProfileContent,
    saveProfileEdit,
    getLivingProfileStatus,
    setLivingProfileEnabled,
    uploadEditLog,
    refreshLivingProfile,
    getProfileMetadataInfo,
    rollbackProfile,
    getRefreshHistory,
    syncProfileUp,
    syncProfileDown,
    getSyncStatus,
    exportProfile,
    createCheckout,
    getSettings,
    type ProfileOverview,
    type ProfileContent,
    type LivingProfileStatus,
    type RefreshResponse,
    type ProfileMetadataInfo,
    type ExternalSample,
    type RefreshHistoryEntry,
    type SectionDiff,
    type SyncStatus,
  } from "$lib/api/tauri";
  import { emit } from "@tauri-apps/api/event";
  import { open } from "@tauri-apps/plugin-shell";
  import { canLivingProfile, canSync, canExport } from "$lib/stores/subscription.svelte";
  import { setRefreshAvailable } from "$lib/stores/patches.svelte";
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
  let isUploading = $state(false);
  let isRefreshing = $state(false);
  let refreshMessage = $state("");
  let latestObservations = $state<string[]>([]);
  let profileMeta = $state<ProfileMetadataInfo | null>(null);
  let isRollingBack = $state(false);
  let showRollbackConfirm = $state(false);

  // Evolution timeline
  let refreshHistory = $state<RefreshHistoryEntry[]>([]);
  let expandedEntryId = $state<string | null>(null);
  let expandedDiffSection = $state<string | null>(null);

  // Sync state
  let syncStatus = $state<SyncStatus | null>(null);
  let isSyncing = $state(false);
  let syncMessage = $state("");

  // Export state (server profiles)
  let isExporting = $state(false);
  let isDevMode = $state(false);

  // Empty state
  let showManualCreate = $state(false);

  // External writing samples (voice specimens for profile refresh)
  type WritingSample = { text: string; format: string; added_at: string };
  let writingDrawerOpen = $state(false);
  let sampleDraft = $state("");
  let sampleFormat = $state("general");
  let writingSamples = $state<WritingSample[]>([]);

  const FORMAT_ACCENTS: Record<string, string> = {
    general: "var(--color-primary)",
    blog: "var(--color-secondary)",
    twitter: "var(--color-accent)",
    email: "var(--color-signal)",
  };

  function hydrateWritingSamples() {
    try {
      const raw = localStorage.getItem("noren:writing_samples");
      if (raw) writingSamples = JSON.parse(raw);
    } catch {}
  }

  function persistWritingSamples() {
    localStorage.setItem("noren:writing_samples", JSON.stringify(writingSamples));
  }

  function commitSample() {
    const body = sampleDraft.trim();
    if (!body) return;
    writingSamples = [
      ...writingSamples,
      { text: body, format: sampleFormat, added_at: new Date().toISOString() },
    ];
    persistWritingSamples();
    sampleDraft = "";
  }

  function discardSample(idx: number) {
    writingSamples = writingSamples.filter((_, i) => i !== idx);
    persistWritingSamples();
  }

  function clearAllSamples() {
    writingSamples = [];
    persistWritingSamples();
  }

  async function loadRefreshHistory() {
    try {
      refreshHistory = await getRefreshHistory(20, 0);
      if (refreshHistory.length > 0) {
        expandedEntryId = refreshHistory[0].id;
      }
    } catch {}
  }

  function toggleEntry(id: string) {
    expandedEntryId = expandedEntryId === id ? null : id;
    expandedDiffSection = null;
  }

  function toggleDiffSection(section: string) {
    expandedDiffSection = expandedDiffSection === section ? null : section;
  }

  function formatDate(iso: string): string {
    return new Date(iso).toLocaleDateString("en-US", { month: "short", day: "numeric", year: "numeric" });
  }

  function formatSectionName(section: string): string {
    if (section === "core_identity") return "Core";
    if (section.startsWith("contexts/")) return section.replace("contexts/", "");
    return section;
  }

  let displayContent = $derived(
    activeTab === "core"
      ? profile?.core_identity ?? ""
      : profile?.contexts[activeTab] ?? "",
  );

  $effect(() => {
    loadProfile();
    hydrateWritingSamples();
  });

  async function loadProfile() {
    try {
      const settings = await getSettings();
      isDevMode = settings.debug_mode ?? false;
      overview = await getProfileOverview();
      if (overview.exists && !overview.is_server) {
        profile = await readProfileContent();
      }
      // Load living profile status + metadata + history
      try {
        livingStatus = await getLivingProfileStatus();
        if (livingStatus.enabled) {
          await loadRefreshHistory();
        }
      } catch { /* not logged in or not available */ }
      try {
        profileMeta = await getProfileMetadataInfo();
        // Update nav dot: refresh is available when cooldown has passed
        const nextRefresh = profileMeta?.next_refresh_available;
        setRefreshAvailable(!nextRefresh || new Date(nextRefresh).getTime() <= Date.now());
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

  function daysUntilRefresh(): number | null {
    if (!profileMeta?.next_refresh_available) return null;
    const diff = new Date(profileMeta.next_refresh_available).getTime() - Date.now();
    if (diff <= 0) return null;
    return Math.ceil(diff / (1000 * 60 * 60 * 24));
  }

  async function handleUploadAndRefresh() {
    error = "";
    refreshMessage = "";
    showRollbackConfirm = false;
    isUploading = true;
    try {
      const samples: ExternalSample[] | undefined = writingSamples.length > 0 ? writingSamples : undefined;
      const count = await uploadEditLog(samples);
      isUploading = false;
      if (count === 0 && !samples?.length) {
        refreshMessage = "No edits to upload yet. Keep writing!";
        return;
      }
      // Clear local samples after successful upload
      if (samples?.length) {
        clearAllSamples();
      }
      isRefreshing = true;
      const result: RefreshResponse = await refreshLivingProfile();
      refreshMessage = result.message;
      if (result.observations.length > 0) {
        latestObservations = result.observations;
      }
      await loadRefreshHistory();
      // Reload metadata (rate limit, rollback state)
      try {
        profileMeta = await getProfileMetadataInfo();
        const nextRefresh = profileMeta?.next_refresh_available;
        setRefreshAvailable(!nextRefresh || new Date(nextRefresh).getTime() <= Date.now());
      } catch {}
    } catch (e) {
      error = friendlyError(e);
    } finally {
      isUploading = false;
      isRefreshing = false;
    }
  }

  async function handleRollback() {
    error = "";
    isRollingBack = true;
    try {
      await rollbackProfile();
      refreshMessage = "Profile restored to pre-refresh version.";
      showRollbackConfirm = false;
      await loadRefreshHistory();
      await loadProfile();
    } catch (e) {
      error = friendlyError(e);
    } finally {
      isRollingBack = false;
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
      <div class="flex-1 flex flex-col items-center justify-center -m-4 overflow-hidden">
        <div class="relative flex flex-col items-center gap-8 animate-fade-in-up" style="animation-duration: 0.6s">
          <!-- Noren curtain — rod with hanging panels -->
          <svg class="w-[130px] h-[88px]" viewBox="0 0 130 88" fill="none">
            <!-- Rod -->
            <line x1="10" y1="8" x2="120" y2="8" stroke="var(--color-primary)" stroke-width="2" stroke-linecap="round" opacity="0.25"/>
            <!-- End caps -->
            <circle cx="10" cy="8" r="2.5" fill="var(--color-primary)" opacity="0.18"/>
            <circle cx="120" cy="8" r="2.5" fill="var(--color-primary)" opacity="0.18"/>

            <!-- Panel 1 -->
            <g style="animation: panel-sway 5s ease-in-out infinite; transform-origin: 30px 8px">
              <rect x="18" y="8" width="24" height="56" rx="1.5" stroke="var(--color-border)" stroke-width="0.75" fill="var(--color-tint)" opacity="0.2"/>
            </g>

            <!-- Panel 2 -->
            <g style="animation: panel-sway 5s 0.8s ease-in-out infinite; transform-origin: 65px 8px">
              <rect x="53" y="8" width="24" height="62" rx="1.5" stroke="var(--color-border)" stroke-width="0.75" fill="var(--color-tint)" opacity="0.2"/>
            </g>

            <!-- Panel 3 -->
            <g style="animation: panel-sway 5s 1.6s ease-in-out infinite; transform-origin: 100px 8px">
              <rect x="88" y="8" width="24" height="52" rx="1.5" stroke="var(--color-border)" stroke-width="0.75" fill="var(--color-tint)" opacity="0.2"/>
            </g>
          </svg>

          <div class="text-center max-w-[260px]">
            <h2 class="font-heading text-[21px] italic font-normal text-foreground leading-snug tracking-[-0.3px]">
              The loom is ready
            </h2>
            <p class="text-[11px] text-muted leading-[1.7] mt-3">
              AI extraction reads your real writing and captures sentence patterns, vocabulary, tone, and format-specific style. The best way to start.
            </p>
          </div>

          <div class="flex flex-col items-center gap-3">
            <button
              onclick={() => emit("navigate", "extract")}
              class="px-6 py-2.5 text-xs font-semibold bg-primary text-white hover:bg-primary-hover transition-all duration-200 cursor-pointer rounded-md hover:-translate-y-px"
              style="box-shadow: 0 2px 8px var(--color-primary-muted)"
            >
              Extract your voice
            </button>
            <button
              onclick={() => { showManualCreate = true; }}
              class="text-[11px] text-secondary font-medium cursor-pointer hover:text-foreground transition-colors"
            >
              Or describe it manually
            </button>
          </div>
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
        {@render livingTabContent()}
        {:else}
        <div class="flex-1 flex flex-col items-center justify-center gap-3 py-8">
          <div class="p-4 bg-tint border border-secondary/20 rounded-lg text-center max-w-[260px]">
            <p class="text-xs font-medium text-secondary">Living Profile</p>
            <p class="text-[10px] text-muted mt-1 leading-relaxed">
              Your profile evolves as you write. Noren tracks your edits and refines automatically.
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

    <!-- Content -->
    <div class="flex-1 flex flex-col min-h-0">
      {#if activeTab === "living"}
        {#if canLivingProfile()}
        {@render livingTabContent()}
        {:else}
        <!-- Living Profile locked -->
        <div class="flex-1 flex flex-col items-center justify-center gap-3 py-8">
          <div class="p-4 bg-tint border border-secondary/20 rounded-lg text-center max-w-[260px]">
            <p class="text-xs font-medium text-secondary">Living Profile</p>
            <p class="text-[10px] text-muted mt-1 leading-relaxed">
              Your profile evolves as you write. Noren tracks your edits and refines automatically.
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

  {#snippet livingTabContent()}
    <div class="flex-1 flex flex-col gap-3 overflow-y-auto">
      <!-- Edit tracking toggle -->
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
        <!-- Upload & Refresh -->
        {@const rateDays = daysUntilRefresh()}
        <button
          onclick={handleUploadAndRefresh}
          disabled={isUploading || isRefreshing || rateDays !== null}
          class="w-full py-2 text-xs font-medium transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed rounded-md
            {rateDays !== null
              ? 'bg-surface text-muted border border-border'
              : 'bg-secondary text-white hover:bg-secondary/90'}"
        >
          {#if isUploading}
            <span class="inline-flex items-center gap-1"><LoadingSpinner /> Uploading edits...</span>
          {:else if isRefreshing}
            <span class="inline-flex items-center gap-1"><LoadingSpinner /> Analyzing patterns...</span>
          {:else if rateDays !== null}
            Available in {rateDays} day{rateDays !== 1 ? "s" : ""}
          {:else}
            Refresh profile{#if livingStatus.edit_count > 0 || writingSamples.length > 0}
              <span class="ml-1 opacity-70">({livingStatus.edit_count} edit{livingStatus.edit_count !== 1 ? "s" : ""}{#if writingSamples.length > 0}, {writingSamples.length} sample{writingSamples.length !== 1 ? "s" : ""}{/if})</span>
            {/if}
          {/if}
        </button>

        {#if refreshMessage}
          <p class="text-[10px] text-muted">{refreshMessage}</p>
        {/if}

        <!-- Inline observations after a fresh refresh -->
        {#if latestObservations.length > 0}
          <div class="pl-2.5" style="border-left: 2px solid var(--color-secondary)">
            {#each latestObservations as obs}
              <p class="text-[10px] text-foreground leading-relaxed">{obs}</p>
            {/each}
          </div>
        {/if}

        <!-- Rollback confirmation -->
        {#if showRollbackConfirm}
          <div class="p-2.5 bg-surface border border-border rounded-lg">
            <p class="text-[10px] text-muted leading-relaxed">Any manual edits made after the last refresh will be lost.</p>
            <div class="flex gap-2 mt-2">
              <button
                onclick={() => { showRollbackConfirm = false; }}
                class="px-2 py-0.5 text-[10px] border border-border text-muted hover:text-foreground cursor-pointer rounded transition-colors"
              >
                Cancel
              </button>
              <button
                onclick={handleRollback}
                disabled={isRollingBack}
                class="px-2 py-0.5 text-[10px] bg-secondary text-white hover:bg-secondary/90 cursor-pointer rounded transition-colors font-medium disabled:opacity-50"
              >
                {#if isRollingBack}
                  <span class="inline-flex items-center gap-1"><LoadingSpinner /> Restoring...</span>
                {:else}
                  Confirm rollback
                {/if}
              </button>
            </div>
          </div>
        {/if}

        <!-- Evolution Timeline -->
        {#if refreshHistory.length > 0}
          <div class="relative pl-4 mt-1">
            <!-- Timeline spine -->
            <div class="absolute left-[5px] top-[6px] bottom-0 w-px" style="background: var(--color-border)"></div>

            <div class="flex flex-col gap-0">
              {#each refreshHistory as entry, i}
                {@const isExpanded = expandedEntryId === entry.id}
                {@const isLatestActive = i === 0 && !entry.rolled_back}
                <div
                  class="relative transition-opacity duration-200"
                  style={entry.rolled_back ? "opacity: 0.5" : ""}
                >
                  <!-- Node circle -->
                  <div
                    class="absolute -left-4 top-[5px] w-[10px] h-[10px] rounded-full border-2 transition-colors"
                    style="background: {isLatestActive ? 'var(--color-secondary)' : 'var(--color-surface)'}; border-color: {isLatestActive ? 'var(--color-secondary)' : 'var(--color-border)'}"
                  ></div>

                  <!-- Entry card -->
                  <button
                    onclick={() => toggleEntry(entry.id)}
                    class="w-full text-left py-2 cursor-pointer"
                  >
                    <div class="flex items-center gap-2">
                      <span class="text-xs text-foreground">{formatDate(entry.created_at)}</span>
                      {#if entry.rolled_back}
                        <span class="px-1.5 py-px text-[8px] uppercase tracking-wide font-medium rounded" style="color: var(--color-error); opacity: 0.7; border: 1px solid var(--color-error); border-opacity: 0.2">Rolled back</span>
                      {/if}
                      {#if !isExpanded}
                        <svg class="w-[8px] h-[8px] ml-auto shrink-0" viewBox="0 0 8 8" fill="none" stroke="var(--color-muted)" stroke-width="1.5" stroke-linecap="round"><path d="M2 3l2 2 2-2"/></svg>
                      {/if}
                    </div>
                    <p class="text-[10px] text-muted mt-0.5">
                      {entry.edits_analyzed} edit{entry.edits_analyzed !== 1 ? "s" : ""}, {entry.samples_analyzed} sample{entry.samples_analyzed !== 1 ? "s" : ""}, {entry.generations_analyzed} generation{entry.generations_analyzed !== 1 ? "s" : ""}
                    </p>
                  </button>

                  {#if isExpanded}
                    <div class="pb-3 flex flex-col gap-2.5">
                      <!-- Observations -->
                      {#if entry.observations.length > 0}
                        <div>
                          <span class="text-[9px] uppercase tracking-wide text-muted font-medium">What we noticed</span>
                          <div class="mt-1 pl-2.5" style="border-left: 2px solid var(--color-secondary)">
                            {#each entry.observations as obs}
                              <p class="text-[10px] text-foreground leading-relaxed py-0.5">{obs}</p>
                            {/each}
                          </div>
                        </div>
                      {/if}

                      <!-- Diff sections -->
                      {#if entry.diffs.length > 0}
                        <div>
                          <span class="text-[9px] uppercase tracking-wide text-muted font-medium">Changes</span>
                          <div class="mt-1 flex flex-col gap-1">
                            {#each entry.diffs as diff}
                              {@const isDiffOpen = expandedDiffSection === diff.section}
                              <div>
                                <button
                                  onclick={() => toggleDiffSection(diff.section)}
                                  class="flex items-center gap-1.5 text-[10px] text-foreground cursor-pointer hover:text-secondary transition-colors py-0.5 w-full text-left"
                                >
                                  <svg
                                    class="w-[7px] h-[7px] shrink-0 transition-transform duration-150"
                                    class:rotate-90={isDiffOpen}
                                    viewBox="0 0 7 7" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"
                                  ><path d="M2.5 1l2.5 2.5-2.5 2.5"/></svg>
                                  {formatSectionName(diff.section)}
                                </button>
                                {#if isDiffOpen}
                                  <div class="ml-3 mt-1 flex flex-col gap-1.5">
                                    <div class="relative overflow-hidden rounded" style="max-height: 80px">
                                      <pre class="text-[10px] font-mono text-muted p-2 leading-relaxed whitespace-pre-wrap" style="opacity: 0.6">{diff.before.slice(0, 500)}{diff.before.length > 500 ? "..." : ""}</pre>
                                      <div class="absolute bottom-0 left-0 right-0 h-4" style="background: linear-gradient(transparent, var(--color-background))"></div>
                                    </div>
                                    <div class="w-full h-px" style="background: var(--color-border)"></div>
                                    <div class="relative overflow-hidden rounded" style="max-height: 80px">
                                      <pre class="text-[10px] font-mono text-secondary p-2 leading-relaxed whitespace-pre-wrap">{diff.after.slice(0, 500)}{diff.after.length > 500 ? "..." : ""}</pre>
                                      <div class="absolute bottom-0 left-0 right-0 h-4" style="background: linear-gradient(transparent, var(--color-background))"></div>
                                    </div>
                                  </div>
                                {/if}
                              </div>
                            {/each}
                          </div>
                        </div>
                      {/if}

                      <!-- Undo button (only on latest non-rolled-back entry) -->
                      {#if isLatestActive && profileMeta?.can_rollback}
                        <button
                          onclick={() => { showRollbackConfirm = true; }}
                          class="self-start text-[10px] text-muted hover:text-foreground cursor-pointer transition-colors mt-0.5"
                        >
                          Undo this refresh
                        </button>
                      {/if}
                    </div>
                  {/if}
                </div>
              {/each}
            </div>
          </div>
        {:else if !isRefreshing && !isUploading}
          <p class="text-[10px] text-muted text-center py-4">
            No refreshes yet. Keep writing and come back when you are ready.
          </p>
        {/if}
      {:else}
        <p class="text-[10px] text-muted leading-relaxed">
          Enable edit tracking to let Noren learn from how you modify generated text.
          Your edits are stored locally and only uploaded when you choose to refresh.
        </p>
      {/if}

      <!-- Voice specimen collector -->
      <div class="rounded-lg overflow-hidden" style="border: 1px solid var(--color-border)">
        <button
          onclick={() => { writingDrawerOpen = !writingDrawerOpen; }}
          class="w-full flex items-center gap-2.5 px-3 py-2.5 cursor-pointer transition-colors hover:bg-tint/40"
          style="background: var(--color-surface)"
        >
          <svg class="w-[14px] h-[14px] shrink-0" viewBox="0 0 16 16" fill="none" style="color: var(--color-secondary)">
            <path d="M11.5 1.5l3 3-9 9H2.5v-3l9-9z" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"/>
            <path d="M9.5 3.5l3 3" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>
          </svg>
          <span class="flex-1 text-left">
            <span class="text-xs font-medium text-foreground">Recent writing</span>
            {#if writingSamples.length > 0}
              <span class="ml-1.5 text-[9px] font-medium" style="color: var(--color-secondary)">{writingSamples.length}</span>
            {/if}
          </span>
          <svg
            class="w-[10px] h-[10px] transition-transform duration-200"
            class:rotate-180={writingDrawerOpen}
            viewBox="0 0 10 10" fill="none" stroke="var(--color-muted)" stroke-width="1.5" stroke-linecap="round"
          >
            <path d="M2.5 3.75l2.5 2.5 2.5-2.5"/>
          </svg>
        </button>

        {#if writingDrawerOpen}
          <div class="flex flex-col gap-3 px-3 pb-3" style="border-top: 1px dashed var(--color-border)">
            <p class="text-[10px] leading-relaxed pt-3" style="color: var(--color-muted)">
              Paste writing you have published elsewhere. Blog posts, tweets, emails. These feed into your next profile refresh.
            </p>

            <div class="flex gap-1">
              {#each Object.keys(FORMAT_ACCENTS) as fmt}
                <button
                  onclick={() => { sampleFormat = fmt; }}
                  class="px-2 py-[3px] text-[10px] cursor-pointer rounded-sm transition-all duration-150"
                  style={sampleFormat === fmt
                    ? `background: ${FORMAT_ACCENTS[fmt]}; color: white; font-weight: 500`
                    : `background: transparent; color: var(--color-muted); border: 1px solid var(--color-border)`}
                >
                  {fmt}
                </button>
              {/each}
            </div>

            <div class="relative">
              <textarea
                bind:value={sampleDraft}
                class="w-full p-2.5 text-xs leading-[1.7] resize-none placeholder-muted focus:outline-none rounded"
                style="
                  border: 1.5px dashed {sampleDraft.trim() ? 'var(--color-secondary)' : 'var(--color-border)'};
                  background: var(--color-warm-surface);
                  color: var(--color-foreground);
                  min-height: 88px;
                  transition: border-color 0.2s;
                "
                placeholder="Paste a piece of your writing..."
              ></textarea>
              {#if sampleDraft.trim()}
                <span class="absolute bottom-2 right-2.5 text-[9px] tabular-nums" style="color: var(--color-muted)">
                  {sampleDraft.trim().split(/\s+/).length} words
                </span>
              {/if}
            </div>

            <div class="flex justify-end">
              <button
                onclick={commitSample}
                disabled={!sampleDraft.trim()}
                class="px-3 py-1.5 text-[10px] font-medium uppercase tracking-[1.5px] cursor-pointer rounded transition-all duration-150"
                style={!sampleDraft.trim()
                  ? 'background: var(--color-surface); color: var(--color-muted); border: 1px solid var(--color-border); opacity: 0.45; cursor: not-allowed'
                  : 'background: var(--color-secondary); color: white'}
              >
                Collect
              </button>
            </div>

            {#if writingSamples.length > 0}
              <div class="flex flex-col gap-1" style="border-top: 1px solid var(--color-border); padding-top: 0.5rem">
                {#each writingSamples as sample, i}
                  <div class="flex items-center gap-2 py-1 group rounded-sm px-1 transition-colors hover:bg-tint/30">
                    <div
                      class="w-[3px] h-[18px] rounded-full shrink-0"
                      style="background: {FORMAT_ACCENTS[sample.format] || FORMAT_ACCENTS.general}"
                    ></div>
                    <span class="text-[9px] font-medium uppercase tracking-wide shrink-0 w-[40px]" style="color: var(--color-muted)">{sample.format}</span>
                    <span class="flex-1 text-[10px] truncate" style="color: var(--color-foreground)">{sample.text.slice(0, 70)}{sample.text.length > 70 ? '...' : ''}</span>
                    <button
                      onclick={() => discardSample(i)}
                      class="opacity-0 group-hover:opacity-100 transition-opacity cursor-pointer p-0.5 rounded hover:bg-error/10"
                      title="Remove"
                    >
                      <svg class="w-[10px] h-[10px]" viewBox="0 0 10 10" fill="none" stroke="var(--color-error)" stroke-width="1.3" stroke-linecap="round">
                        <path d="M2.5 2.5l5 5M7.5 2.5l-5 5"/>
                      </svg>
                    </button>
                  </div>
                {/each}
                <button
                  onclick={clearAllSamples}
                  class="self-start text-[9px] mt-1 cursor-pointer text-muted hover:text-error transition-colors"
                >
                  Clear all samples
                </button>
              </div>
            {/if}
          </div>
        {/if}
      </div>
    </div>
  {/snippet}
</div>
