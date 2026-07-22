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
    exportProfile,
    createCheckout,
    createExportUnlockCheckout,
    guidedProfileEdit,
    getSettings,
    type ProfileOverview,
    type ProfileContent,
    type LivingProfileStatus,
    type RefreshResponse,
    type ProfileMetadataInfo,
    type ExternalSample,
    type RefreshHistoryEntry,
    type SectionDiff,
    type VoiceOverview,
    type GuidedEditResponse,
  } from "$lib/api/tauri";
  import { emit } from "@tauri-apps/api/event";
  import { open } from "@tauri-apps/plugin-shell";
  import { canLivingProfile, canExport, exportUnlockRemainingCents, exportUnlockProgress, isPro } from "$lib/stores/subscription.svelte";
  import { setRefreshAvailable } from "$lib/stores/patches.svelte";
  import { refresh as refreshSubscription } from "$lib/stores/subscription.svelte";
  import { friendlyError } from "$lib/utils/errors";
  import { toastSuccess } from "$lib/stores/toast.svelte";
  import { marked } from "marked";
  import DOMPurify from "dompurify";
  import LoadingSpinner from "./LoadingSpinner.svelte";
  import loomIdleUrl from "../../assets/loom-idle.png";

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
  let showAddContext = $state(false);
  let newContextFormat = $state("");
  let newContextContent = $state("");
  let isAddingContext = $state(false);

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

  // Sample submission (server profile)
  let showSampleInput = $state(false);
  let isSubmittingSample = $state(false);

  // Export state (server profiles)
  let isExporting = $state(false);
  let isDevMode = $state(false);

  // Guided editing state
  let guidedInstruction = $state("");
  let guidedFormat = $state<string | undefined>(undefined);
  let isGuidedEditing = $state(false);
  let guidedResult = $state<GuidedEditResponse | null>(null);
  let guidedError = $state("");

  // Voice card collapse (persisted)
  let voiceCardOpen = $state(localStorage.getItem("noren-voice-expanded") === "true");

  function toggleVoiceCard() {
    voiceCardOpen = !voiceCardOpen;
    localStorage.setItem("noren-voice-expanded", voiceCardOpen ? "true" : "false");
  }

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
  }

  function formatDate(iso: string): string {
    return new Date(iso).toLocaleDateString("en-US", { month: "short", day: "numeric", year: "numeric" });
  }

  function formatSectionName(section: string): string {
    if (section === "core_identity") return "Core";
    if (section.startsWith("contexts/")) return section.replace("contexts/", "");
    return section;
  }

  async function handleSubmitSample() {
    if (!sampleDraft.trim() || isSubmittingSample) return;
    isSubmittingSample = true;
    error = "";
    try {
      await uploadEditLog([{ text: sampleDraft.trim(), format: sampleFormat, added_at: new Date().toISOString() }]);
      sampleDraft = "";
      showSampleInput = false;
      try { profileMeta = await getProfileMetadataInfo(); } catch {}
    } catch (e) {
      error = friendlyError(e);
    } finally {
      isSubmittingSample = false;
    }
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
        // Always load refresh history for server profiles (edits tracked server-side).
        // For local profiles, only load if local tracking is enabled.
        if (overview.is_server || livingStatus.enabled) {
          await loadRefreshHistory();
        }
      } catch { /* not logged in or not available */ }
      try {
        profileMeta = await getProfileMetadataInfo();
        // Update nav dot: refresh is available when cooldown has passed
        const nextRefresh = profileMeta?.next_refresh_available;
        setRefreshAvailable(!nextRefresh || new Date(nextRefresh).getTime() <= Date.now());
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

  function normalizeContextFormat(input: string): string {
    return input.trim().toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "");
  }

  function startAddContext() {
    if (isEditing) cancelEditing();
    showAddContext = true;
    newContextFormat = "";
    newContextContent = "";
    error = "";
    saveSuccess = false;
  }

  function cancelAddContext() {
    showAddContext = false;
    newContextFormat = "";
    newContextContent = "";
    error = "";
  }

  async function handleAddContext() {
    const fmt = normalizeContextFormat(newContextFormat);
    const content = newContextContent.trim();
    if (!fmt) {
      error = "Enter a context name.";
      return;
    }
    if (fmt === "core" || fmt === "living") {
      error = "Choose a different context name.";
      return;
    }
    if (overview?.formats.includes(fmt)) {
      error = `Context "${fmt}" already exists.`;
      return;
    }
    if (!content) {
      error = "Add context details before saving.";
      return;
    }

    isAddingContext = true;
    error = "";
    saveSuccess = false;
    try {
      await saveProfileEdit({
        coreIdentity: profile?.core_identity ?? "",
        contextFormat: fmt,
        contextContent: content,
      });
      await loadProfile();
      activeTab = fmt;
      showAddContext = false;
      newContextFormat = "";
      newContextContent = "";
      saveSuccess = true;
    } catch (e) {
      error = friendlyError(e);
    } finally {
      isAddingContext = false;
    }
  }

  function switchTab(tab: string) {
    if (isEditing) cancelEditing();
    if (showAddContext) cancelAddContext();
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

  async function handleExport() {
    isExporting = true;
    error = "";
    try {
      const savedPath = await exportProfile();
      toastSuccess(`Profile exported to ${savedPath}`);
      await loadProfile();
    } catch (e) {
      const msg = friendlyError(e);
      if (!msg.includes("cancelled")) {
        error = msg;
      }
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

  async function handleGuidedEdit() {
    if (!guidedInstruction.trim()) return;
    guidedError = "";
    guidedResult = null;
    isGuidedEditing = true;
    try {
      const result = await guidedProfileEdit({
        instruction: guidedInstruction.trim(),
        format: guidedFormat,
      });
      guidedResult = result;
      if (result.edited) {
        guidedInstruction = "";
        await loadProfile();
      }
    } catch (e) {
      guidedError = friendlyError(e);
    } finally {
      isGuidedEditing = false;
    }
  }

  async function handleExportUnlock() {
    error = "";
    try {
      const result = await createExportUnlockCheckout();
      if (result.checkout_url === "dev://granted" || result.session_id === "already_unlocked") {
        await refreshSubscription();
        await loadProfile();
      } else {
        await open(result.checkout_url);
      }
    } catch (e) {
      error = friendlyError(e);
    }
  }
</script>

<div class="pv-page animate-fade-in-up">
  {#if !overview}
    <div class="flex items-center justify-center h-full">
      <LoadingSpinner />
    </div>
  {:else if !overview.exists}
    {#if !showManualCreate}
      <!-- Empty state: no profile -->
      <div class="flex-1 flex flex-col items-center justify-center -m-4 overflow-hidden">
        <div class="relative flex flex-col items-center gap-8 animate-fade-in-up" style="animation-duration: 0.6s">
          <img src={loomIdleUrl} alt="" class="w-[130px] loom-idle-img" />

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
              class="px-6 py-2.5 text-xs font-semibold bg-accent text-white hover:bg-accent-hover transition-all duration-200 cursor-pointer rounded-md hover:-translate-y-px"
              style="box-shadow: 0 2px 8px var(--color-accent-glow)"
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
            <p class="text-sm font-medium text-foreground font-heading italic">Describe your voice</p>
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
        <div class="p-2 bg-tint border border-secondary/20 rounded-xl flex flex-col gap-1.5">
          <p class="text-[10px] text-muted leading-relaxed">
            <span class="text-secondary font-medium">AI Extraction</span> captures more detail from your real writing.
          </p>
          <div class="flex gap-2 items-center">
            <button
              onclick={() => handleUpgrade("extraction")}
              class="text-[10px] text-secondary font-medium cursor-pointer hover:text-foreground uppercase tracking-wide"
            >
              One-time $49.99
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
              : 'bg-accent text-white hover:bg-accent-hover'}"
        >
          {isSaving ? "Saving..." : "Save Profile"}
        </button>

        {#if error}
          <div class="p-2 bg-tint border border-border rounded-xl text-xs text-muted leading-relaxed">
            {error}
          </div>
        {/if}
      </div>
    {/if}
  {:else if overview.is_server}
    <!-- Server profile -->
    {@const vo = overview.voice_overview}
    <div class="flex flex-col gap-3 h-full overflow-y-auto pv-stagger">

      <!-- Voice Card (collapsible) -->
      <div class="pv-voice-card" class:open={voiceCardOpen}>
        <button class="pv-vc-header" onclick={toggleVoiceCard}>
          <div class="pv-vc-header-top">
            <span class="font-heading text-sm italic text-foreground">Your voice</span>
            <span class="pv-vc-chevron">
              <svg viewBox="0 0 10 6" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M1 1l4 4 4-4"/></svg>
            </span>
          </div>
          {#if vo?.summary}
            <p class="pv-vc-summary">
              <em>{vo.summary.split('.')[0]}.</em>{vo.summary.split('.').length > 1 ? ' ' + vo.summary.split('.').slice(1).join('.').trim() : ''}
            </p>
          {:else}
            <p class="pv-vc-summary"><em>Voice profile on Noren servers</em></p>
          {/if}
          {#if vo?.counts || vo?.baseline_rhythm || overview.formats.length > 0}
            <div class="pv-vc-stats-row">
              {#if vo?.counts?.profile_lines}
                <span class="pv-vc-stat-chip">{vo.counts.profile_lines} <span>lines</span></span>
              {/if}
              {#if vo?.corpus?.unique_sample_count}
                <span class="pv-vc-stat-chip">{vo.corpus.unique_sample_count} <span>samples</span></span>
              {/if}
              {#if vo?.baseline_rhythm?.longToShortRatio}
                <span class="pv-vc-stat-chip">{vo.baseline_rhythm.longToShortRatio.toFixed(1)} <span>L:S</span></span>
              {/if}
              {#if overview.formats.length > 0}
                <span class="pv-vc-stat-chip">{overview.formats.length} <span>formats</span></span>
              {/if}
            </div>
          {/if}
        </button>

        <div class="pv-vc-detail-wrap">
          <div class="pv-vc-detail-clip">
            <div class="pv-vc-detail-inner">

              {#if vo?.summary}
                <div>
                  <span class="section-label">Voice snapshot</span>
                  <p class="text-xs text-foreground leading-relaxed mt-1.5">{vo.summary}</p>
                </div>
              {/if}

              {#if vo?.routing}
                {@const routing = vo.routing}
                <div>
                  <span class="section-label">Voice dimensions</span>
                  <div class="pv-dims">
                    {@render dimBar("Structure", routing.structure_predictability === "high" ? 85 : routing.structure_predictability === "medium" ? 50 : 15, routing.structure_predictability, "varied", "predictable")}
                    {@render dimBar("Register", routing.register_break_frequency * 10, `${routing.register_break_frequency} / 10`, "consistent", "shifting")}
                    {@render dimBar("Formality", routing.casual_marker_density === "high" ? 85 : routing.casual_marker_density === "medium" ? 50 : 15, routing.casual_marker_density, "formal", "casual")}
                    {@render dimBar("Phrasing", routing.signature_phrase_rigidity === "high" ? 85 : routing.signature_phrase_rigidity === "medium" ? 50 : 15, routing.signature_phrase_rigidity, "fluid", "fixed")}
                  </div>
                </div>
              {/if}

              {#if vo?.counts}
                {@const counts = vo.counts}
                <div>
                  <span class="section-label">Pattern depth</span>
                  <div class="pv-depth">
                    <div class="pv-depth-item"><span class="pv-depth-count">{counts.analogy_domains}</span><span class="pv-depth-name">analogy<br>families</span></div>
                    <div class="pv-depth-item"><span class="pv-depth-count">{counts.micro_constructions}</span><span class="pv-depth-name">sentence<br>patterns</span></div>
                    <div class="pv-depth-item"><span class="pv-depth-count">{counts.signature_phrases}</span><span class="pv-depth-name">signature<br>phrases</span></div>
                    <div class="pv-depth-item"><span class="pv-depth-count">{counts.anti_patterns}</span><span class="pv-depth-name">anti-<br>patterns</span></div>
                    {#if vo.corpus}
                      <div class="pv-depth-item pv-depth-full">
                        <span class="pv-depth-count" style="font-size: 14px;">{counts.profile_lines}</span>
                        <span class="pv-depth-name">lines of voice DNA across {vo.corpus.unique_sample_count} samples</span>
                      </div>
                    {/if}
                  </div>
                </div>
              {/if}

              {#if vo?.baseline_rhythm}
                {@const rhythm = vo.baseline_rhythm}
                <div>
                  <span class="section-label">Sentence rhythm</span>
                  <div style="margin-top: 10px;">
                    <div class="pv-rhythm-bar">
                      <div class="pv-rhythm-seg pv-rhythm-short" style="width: {rhythm.distributionPct.short}%"></div>
                      <div class="pv-rhythm-seg pv-rhythm-medium" style="width: {rhythm.distributionPct.medium}%"></div>
                      <div class="pv-rhythm-seg pv-rhythm-long" style="width: {rhythm.distributionPct.long}%"></div>
                      <div class="pv-rhythm-seg pv-rhythm-vlong" style="width: {rhythm.distributionPct.veryLong}%"></div>
                    </div>
                    <div class="pv-rhythm-legend">
                      <span class="pv-rhythm-legend-item"><span class="pv-rhythm-dot" style="background: var(--color-secondary)"></span>Short &lt;8w</span>
                      <span class="pv-rhythm-legend-item"><span class="pv-rhythm-dot" style="background: var(--color-accent)"></span>Medium 8-15w</span>
                      <span class="pv-rhythm-legend-item"><span class="pv-rhythm-dot" style="background: var(--color-warning)"></span>Long 16-25w</span>
                      <span class="pv-rhythm-legend-item"><span class="pv-rhythm-dot" style="background: #C23B2A"></span>25w+</span>
                    </div>
                    <div class="pv-rhythm-stats-grid">
                      <div class="pv-rhythm-stat"><span class="pv-rhythm-stat-val">{Math.round(rhythm.medianWordCount)}</span><span class="pv-rhythm-stat-lbl">median words</span></div>
                      <div class="pv-rhythm-stat"><span class="pv-rhythm-stat-val">{rhythm.sentenceCeiling}</span><span class="pv-rhythm-stat-lbl">ceiling</span></div>
                      <div class="pv-rhythm-stat"><span class="pv-rhythm-stat-val">{rhythm.longToShortRatio.toFixed(1)}</span><span class="pv-rhythm-stat-lbl">L:S ratio</span></div>
                      <div class="pv-rhythm-stat"><span class="pv-rhythm-stat-val">{rhythm.medianCommasPerSentence.toFixed(1)}</span><span class="pv-rhythm-stat-lbl">commas/sent</span></div>
                    </div>
                  </div>
                </div>
              {/if}

              {#if overview.formats.length > 0}
                <div>
                  <span class="section-label">Formats</span>
                  <div class="pv-format-list">
                    {#each overview.formats as fmt}
                      {@const fmtRhythm = vo?.format_rhythms?.[fmt]}
                      <div class="pv-format-row">
                        <div class="pv-format-accent" style="background: {FORMAT_ACCENTS[fmt] || 'var(--color-primary)'}"></div>
                        <span class="pv-format-name">{fmt}</span>
                        {#if fmtRhythm}
                          <div class="pv-format-stats">
                            <span class="pv-format-stat"><strong>{Math.round(fmtRhythm.medianWordCount)}</strong> median</span>
                            <span class="pv-format-stat"><strong>{fmtRhythm.longToShortRatio.toFixed(1)}</strong> L:S</span>
                          </div>
                        {/if}
                      </div>
                    {/each}
                  </div>
                </div>
              {/if}

            </div>
          </div>
        </div>
      </div>

      <!-- Guided Edit -->
      {#if isPro() && vo}
        <div class="ge-card">
          <span class="section-label">Refine your voice</span>
          <div class="ge-input-row">
            <input
              class="ge-input"
              bind:value={guidedInstruction}
              placeholder="e.g. Remove exclamation marks from my voice"
              disabled={isGuidedEditing}
              onkeydown={(e) => { if (e.key === "Enter") handleGuidedEdit(); }}
            />
            <button
              class="ge-submit"
              onclick={handleGuidedEdit}
              disabled={!guidedInstruction.trim() || isGuidedEditing}
            >
              {isGuidedEditing ? "..." : "Apply"}
            </button>
          </div>
          {#if overview.formats.length > 0}
            <div class="ge-format-row">
              <button
                class="ge-format-pill {guidedFormat === undefined ? 'active' : ''}"
                onclick={() => { guidedFormat = undefined; }}
              >Core</button>
              {#each overview.formats as fmt}
                <button
                  class="ge-format-pill {guidedFormat === fmt ? 'active' : ''}"
                  onclick={() => { guidedFormat = fmt; }}
                >{fmt}</button>
              {/each}
            </div>
          {/if}
          {#if isGuidedEditing}
            <div class="ge-loading">
              <LoadingSpinner />
              <span class="text-[11px] text-muted">Applying changes to {guidedFormat || "core"} profile...</span>
            </div>
          {/if}
          {#if guidedResult}
            <div class="ge-result" class:ge-result-noop={!guidedResult.edited}>
              <span class="ge-result-msg" class:text-signal={guidedResult.edited} class:text-muted={!guidedResult.edited}>
                {guidedResult.message}
              </span>
            </div>
          {/if}
          {#if guidedError}
            <p class="text-[10px] text-error mt-2">{guidedError}</p>
          {/if}
        </div>
      {/if}

      <!-- Living Profile section (server profiles) -->
      {#if canLivingProfile()}
        <div class="card-flat" style="padding: 12px 14px;">
          <div class="flex items-center gap-1.5 mb-3">
            <div class="w-[5px] h-[5px] rounded-full bg-secondary animate-voice-pulse"></div>
            <span class="section-label" style="margin:0">Living Profile</span>
          </div>

          <!-- Upload & Refresh -->
          <button
            onclick={handleUploadAndRefresh}
            disabled={isUploading || isRefreshing || daysUntilRefresh() !== null}
            class="w-full py-2 text-xs font-medium transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed rounded-md
              {daysUntilRefresh() !== null
                ? 'bg-surface text-muted border border-border'
                : 'bg-secondary text-white hover:bg-secondary/90'}"
          >
            {#if isUploading}
              <span class="inline-flex items-center gap-1"><LoadingSpinner /> Uploading edits...</span>
            {:else if isRefreshing}
              <span class="inline-flex items-center gap-1"><LoadingSpinner /> Analyzing patterns...</span>
            {:else if daysUntilRefresh() !== null}
              Available in {daysUntilRefresh()} day{daysUntilRefresh() !== 1 ? "s" : ""}
            {:else}
              Refresh profile
            {/if}
          </button>

          {#if refreshMessage}
            <p class="text-[10px] text-muted mt-2">{refreshMessage}</p>
          {/if}

          <!-- Signal counts -->
          {#if profileMeta && (profileMeta.edits_pending > 0 || profileMeta.samples_pending > 0 || profileMeta.generations_since_refresh > 0)}
            <div class="mt-2 flex flex-wrap gap-x-3 gap-y-1 text-[10px] text-muted">
              {#if profileMeta.edits_pending > 0}
                <span>{profileMeta.edits_pending} edit{profileMeta.edits_pending !== 1 ? "s" : ""}</span>
              {/if}
              {#if profileMeta.samples_pending > 0}
                <span>{profileMeta.samples_pending} sample{profileMeta.samples_pending !== 1 ? "s" : ""}</span>
              {/if}
              {#if profileMeta.generations_since_refresh > 0}
                <span>{profileMeta.generations_since_refresh} generation{profileMeta.generations_since_refresh !== 1 ? "s" : ""}</span>
              {/if}
              <span style="opacity: 0.6">queued for next refresh</span>
            </div>
          {/if}

          <!-- Add writing sample -->
          {#if showSampleInput}
            <div class="mt-2 flex flex-col gap-2">
              <textarea
                bind:value={sampleDraft}
                placeholder="Paste a writing sample..."
                class="w-full p-2.5 text-xs leading-relaxed border border-border bg-surface text-foreground resize-none rounded-md focus:outline-none focus:border-secondary"
                rows="5"
              ></textarea>
              <div class="flex flex-wrap gap-1">
                {#each overview?.formats || ["general"] as fmt}
                  <button
                    onclick={() => { sampleFormat = fmt; }}
                    class="px-2.5 py-1 text-[10px] rounded cursor-pointer transition-colors
                      {sampleFormat === fmt ? 'bg-secondary text-white' : 'bg-surface border border-border text-muted hover:text-foreground'}"
                  >{fmt}</button>
                {/each}
              </div>
              <div class="flex gap-2">
                <button
                  onclick={() => { showSampleInput = false; sampleDraft = ""; }}
                  class="px-2.5 py-1 text-[10px] text-muted hover:text-foreground cursor-pointer transition-colors"
                >Cancel</button>
                <button
                  onclick={handleSubmitSample}
                  disabled={!sampleDraft.trim() || isSubmittingSample}
                  class="px-2.5 py-1 text-[10px] bg-secondary text-white hover:bg-secondary/90 cursor-pointer rounded transition-colors font-medium disabled:opacity-50"
                >{isSubmittingSample ? "Submitting..." : "Submit sample"}</button>
              </div>
            </div>
          {:else}
            <button
              onclick={() => { showSampleInput = true; }}
              class="mt-2 text-[10px] text-muted hover:text-secondary cursor-pointer transition-colors"
            >+ Add writing sample</button>
          {/if}

          <!-- Inline observations after a fresh refresh -->
          {#if latestObservations.length > 0}
            <div class="mt-2 pl-2.5" style="border-left: 2px solid var(--color-secondary)">
              {#each latestObservations as obs}
                <p class="text-[10px] text-foreground leading-relaxed py-0.5">{obs}</p>
              {/each}
            </div>
          {/if}

          <!-- Rollback confirmation -->
          {#if showRollbackConfirm}
            <div class="mt-2 p-2.5 bg-surface border border-border rounded-xl">
              <p class="text-[10px] text-muted leading-relaxed">Any manual edits made after the last refresh will be lost.</p>
              <div class="flex gap-2 mt-2">
                <button
                  onclick={() => { showRollbackConfirm = false; }}
                  class="px-2 py-0.5 text-[10px] border border-border text-muted hover:text-foreground cursor-pointer rounded transition-colors"
                >Cancel</button>
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
            <div class="relative pl-4 mt-3">
              <div class="absolute left-[5px] top-[6px] bottom-0 w-px" style="background: var(--color-border)"></div>
              <div class="flex flex-col gap-0">
                {#each refreshHistory as entry, i}
                  {@const isExpanded = expandedEntryId === entry.id}
                  {@const isLatestActive = i === 0 && !entry.rolled_back}
                  <div
                    class="relative transition-opacity duration-200"
                    style={entry.rolled_back ? "opacity: 0.5" : ""}
                  >
                    <div
                      class="absolute -left-4 top-[5px] w-[10px] h-[10px] rounded-full border-2 transition-colors"
                      style="background: {isLatestActive ? 'var(--color-secondary)' : 'var(--color-surface)'}; border-color: {isLatestActive ? 'var(--color-secondary)' : 'var(--color-border)'}"
                    ></div>
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
                        {#if entry.sections_updated.length > 0}
                          <div>
                            <span class="text-[9px] uppercase tracking-wide text-muted font-medium">Changes</span>
                            <div class="mt-1 flex flex-wrap gap-1.5">
                              {#each entry.sections_updated as section}
                                <span class="px-2 py-1 rounded-md border border-border text-[10px] text-muted">
                                  {formatSectionName(section)}
                                </span>
                              {/each}
                            </div>
                          </div>
                        {/if}
                        {#if isLatestActive && profileMeta?.can_rollback}
                          <button
                            onclick={() => { showRollbackConfirm = true; }}
                            class="self-start text-[10px] text-muted hover:text-foreground cursor-pointer transition-colors mt-0.5"
                          >Undo this refresh</button>
                        {/if}
                      </div>
                    {/if}
                  </div>
                {/each}
              </div>
            </div>
          {:else if !isRefreshing && !isUploading}
            <p class="text-[10px] text-muted text-center py-3">
              No refreshes yet. Keep writing and come back when you are ready.
            </p>
          {/if}
        </div>
      {:else}
        <div class="p-2.5 bg-tint border border-secondary/10 rounded-xl">
          <p class="text-subhead text-secondary">Living Profile</p>
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
      {/if}

      {#if error}
        <div class="p-2 bg-tint border border-border rounded-xl text-xs text-muted leading-relaxed shrink-0">
          {error}
        </div>
      {/if}

      <!-- Footer: Export -->
      <div class="pv-footer-row">
        <span class="text-[10px] text-muted">Stored on Noren servers</span>
        {#if canExport()}
          <button
            onclick={handleExport}
            disabled={isExporting}
            class="pv-footer-export"
          >
            {isExporting ? "..." : "Export as Markdown"}
          </button>
        {:else if exportUnlockProgress() != null}
          <button onclick={handleExportUnlock} class="pv-footer-btn">
            Export <span class="text-[8px] text-secondary font-medium">${Math.round((exportUnlockRemainingCents() || 0) / 100)}</span>
          </button>
        {:else}
          <button
            onclick={handleExportUnlock}
            class="pv-footer-btn"
          >
            Export <span class="text-[8px] text-secondary font-medium">$</span>
          </button>
        {/if}
      </div>
    </div>
  {:else}
    <!-- Local profile -->
    <div class="flex flex-col gap-3 flex-1 min-h-0 pv-stagger">
    <!-- Tabs -->
    <div class="flex flex-wrap gap-1 shrink-0 border-b border-border">
      <button
        onclick={() => switchTab("core")}
        class="px-2.5 py-1.5 text-xs whitespace-nowrap transition-colors cursor-pointer uppercase tracking-wide
          {activeTab === 'core'
            ? 'border-b-2 border-accent text-accent font-medium'
            : 'border-b-2 border-transparent text-muted hover:text-foreground'}"
      >
        Core Identity
      </button>
      {#each overview.formats as fmt}
        <button
          onclick={() => switchTab(fmt)}
          class="px-2.5 py-1.5 text-xs whitespace-nowrap transition-colors cursor-pointer uppercase tracking-wide
            {activeTab === fmt
              ? 'border-b-2 border-accent text-accent font-medium'
              : 'border-b-2 border-transparent text-muted hover:text-foreground'}"
        >
          {fmt}
        </button>
      {/each}
      <button
        onclick={startAddContext}
        class="px-2.5 py-1.5 text-xs whitespace-nowrap transition-colors cursor-pointer uppercase tracking-wide border-b-2 border-transparent text-muted hover:text-foreground"
      >
        + Context
      </button>
      <button
        onclick={() => switchTab("living")}
        class="px-2.5 py-1.5 text-xs whitespace-nowrap transition-colors cursor-pointer uppercase tracking-wide relative
          {activeTab === 'living'
            ? 'border-b-2 border-accent text-accent font-medium'
            : 'border-b-2 border-transparent text-muted hover:text-foreground'}"
      >
        Living
        {#if !canLivingProfile()}
          <span class="ml-0.5 text-[8px] {activeTab === 'living' ? 'text-accent/70' : 'text-secondary'} font-medium">PRO</span>
        {/if}
      </button>
    </div>

    {#if showAddContext}
      <div class="p-3 bg-tint border border-border rounded-xl shrink-0 flex flex-col gap-2">
        <div class="flex flex-col sm:flex-row gap-2">
          <input
            bind:value={newContextFormat}
            placeholder="Context name, e.g. email"
            class="flex-1 px-3 py-2 text-xs border border-border bg-surface text-foreground rounded-md focus:outline-none focus:border-secondary"
          />
          <div class="flex gap-1">
            <button
              onclick={cancelAddContext}
              class="px-3 py-2 text-xs border border-border hover:border-secondary transition-colors cursor-pointer text-muted hover:text-foreground rounded-md"
            >
              Cancel
            </button>
            <button
              onclick={handleAddContext}
              disabled={isAddingContext}
              class="px-3 py-2 text-xs bg-accent text-white hover:bg-accent-hover transition-colors cursor-pointer disabled:opacity-50 rounded-md font-medium"
            >
              {isAddingContext ? "Adding..." : "Add"}
            </button>
          </div>
        </div>
        <textarea
          bind:value={newContextContent}
          placeholder="Describe how you write in this context."
          class="min-h-[120px] p-3 text-xs leading-relaxed border border-border bg-surface text-foreground resize-y rounded-md focus:outline-none focus:border-secondary"
        ></textarea>
      </div>
    {/if}

    {#if canLivingProfile() && activeTab !== "living"}
      <div class="flex items-center gap-1.5 shrink-0">
        <div class="w-[5px] h-[5px] rounded-full bg-secondary animate-voice-pulse"></div>
        <span class="text-subhead text-secondary">Living Profile</span>
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
          <div class="p-4 bg-tint border border-secondary/20 rounded-xl text-center max-w-[260px]">
            <p class="text-subhead text-secondary">Living Profile</p>
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
        <div class="flex-1 output-card p-3 overflow-y-auto">
          <div class="prose-profile text-xs text-foreground leading-relaxed selectable relative z-[1]">{@html renderMarkdown(displayContent)}</div>
        </div>
      {/if}
    </div>

    <!-- Upgrade nudge for manual-only profiles (no format contexts) -->
    {#if overview.formats.length === 0 && activeTab === "core" && !isEditing}
      <div class="p-2 bg-tint border border-secondary/15 rounded-xl shrink-0 flex flex-col gap-1.5">
        <p class="text-[10px] text-muted leading-relaxed">
          Your profile covers the basics. <span class="text-secondary font-medium">AI extraction</span> adds format-specific contexts and vocabulary analysis.
        </p>
        <div class="flex gap-2 items-center">
          <button
            onclick={() => handleUpgrade("extraction")}
            class="text-[10px] text-secondary font-medium cursor-pointer hover:text-foreground uppercase tracking-wide"
          >
            One-time $49.99
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
          {#if saveSuccess}
            <span class="text-signal">Saved</span>
          {:else}
            {activeTab === "core" ? "Core Identity" : activeTab} &middot; {displayContent.split("\n").length} lines
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
              class="px-3 py-1.5 text-xs bg-accent text-white hover:bg-accent-hover transition-colors cursor-pointer disabled:opacity-50 rounded-md font-medium"
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
          {/if}
        </div>
      </div>
    {/if}

    <!-- Error -->
    {#if error}
      <div class="p-2 bg-tint border border-border rounded-xl text-xs text-muted leading-relaxed shrink-0">
        {error}
      </div>
    {/if}
    </div>
  {/if}

  {#snippet dimBar(label: string, pct: number, value: string, lowLabel: string, highLabel: string)}
    <div class="pv-dim-row">
      <div class="pv-dim-header">
        <span class="pv-dim-label">{label}</span>
        <span class="pv-dim-value">{value}</span>
      </div>
      <div class="pv-dim-track">
        <div class="pv-dim-indicator" style="left: {pct}%"></div>
      </div>
      <div class="pv-dim-ends">
        <span class="pv-dim-end">{lowLabel}</span>
        <span class="pv-dim-end">{highLabel}</span>
      </div>
    </div>
  {/snippet}

  {#snippet livingTabContent()}
    <div class="flex-1 flex flex-col gap-3 overflow-y-auto">
      <!-- Edit tracking toggle -->
      <div class="card-flat" style="overflow: hidden;">
        <div class="pv-setting-row">
          <div>
            <div class="pv-setting-label">Edit tracking</div>
            <div class="pv-setting-desc">Track edits to improve your profile over time</div>
            {#if livingStatus?.enabled}
              <div class="text-[10px] text-secondary" style="margin-top: 6px;">
                {livingStatus.edit_count} edits tracked locally
              </div>
            {/if}
          </div>
          <button
            onclick={handleToggleLiving}
            class="toggle {livingStatus?.enabled ? 'active' : ''}"
            aria-label="Toggle edit tracking"
          ></button>
        </div>
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
          <div class="p-2.5 bg-surface border border-border rounded-xl">
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

                      {#if entry.sections_updated.length > 0}
                        <div>
                          <span class="text-[9px] uppercase tracking-wide text-muted font-medium">Changes</span>
                          <div class="mt-1 flex flex-wrap gap-1.5">
                            {#each entry.sections_updated as section}
                              <span class="px-2 py-1 rounded-md border border-border text-[10px] text-muted">
                                {formatSectionName(section)}
                              </span>
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
      <div class="rounded-xl overflow-hidden" style="border: 1px solid var(--color-border)">
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
            <span class="text-xs font-medium text-foreground font-heading italic">Recent writing</span>
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

<style>
  .pv-page {
    display: flex;
    flex-direction: column;
    gap: clamp(10px, 2vw, 14px);
    height: 100%;
    padding: clamp(12px, 3vw, 20px);
    overflow: hidden;
  }

  /* Staggered entry */
  .pv-stagger > :global(*) {
    animation: pv-enter 0.4s cubic-bezier(0.16, 1, 0.3, 1) both;
  }
  .pv-stagger > :global(*:nth-child(1)) { animation-delay: 0ms; }
  .pv-stagger > :global(*:nth-child(2)) { animation-delay: 60ms; }
  .pv-stagger > :global(*:nth-child(3)) { animation-delay: 120ms; }
  .pv-stagger > :global(*:nth-child(4)) { animation-delay: 180ms; }
  .pv-stagger > :global(*:nth-child(5)) { animation-delay: 240ms; }
  .pv-stagger > :global(*:nth-child(6)) { animation-delay: 300ms; }
  .pv-stagger > :global(*:nth-child(7)) { animation-delay: 360ms; }
  .pv-stagger > :global(*:nth-child(8)) { animation-delay: 420ms; }

  @keyframes pv-enter {
    from { opacity: 0; transform: translateY(8px); }
    to { opacity: 1; transform: translateY(0); }
  }

  /* Setting row (for toggle) */
  .pv-setting-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: clamp(12px, 2vw, 16px);
    gap: 12px;
  }
  .pv-setting-label { font-size: 13px; font-weight: 600; }
  .pv-setting-desc { font-size: 11px; color: var(--color-muted); margin-top: 2px; }

  /* Voice Card (collapsible) */
  .pv-voice-card {
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: 12px;
    border-top: 2px solid var(--color-accent);
    box-shadow: var(--shadow-card);
  }
  .pv-vc-header {
    display: block;
    width: 100%;
    padding: 14px 16px;
    cursor: pointer;
    transition: background 0.15s;
    user-select: none;
    text-align: left;
    background: none;
    border: none;
    font-family: inherit;
  }
  .pv-vc-header:hover { background: rgba(30,49,72,0.015); }
  .pv-vc-header-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .pv-vc-chevron {
    color: var(--color-muted);
    transition: transform 0.35s cubic-bezier(0.16, 1, 0.3, 1);
    width: 20px; height: 20px;
    display: flex; align-items: center; justify-content: center;
  }
  .pv-vc-chevron svg { width: 10px; height: 10px; }
  .pv-voice-card.open .pv-vc-chevron { transform: rotate(180deg); }
  .pv-vc-summary {
    font-size: 11px; color: var(--color-muted);
    line-height: 1.5; margin-top: 6px;
  }
  .pv-vc-summary em {
    font-style: normal;
    color: var(--color-foreground);
  }
  .pv-vc-stats-row {
    display: flex; gap: 6px; margin-top: 8px; flex-wrap: wrap;
  }
  .pv-vc-stat-chip {
    font-size: 10px; font-weight: 600; color: var(--color-foreground);
    background: var(--color-tint); padding: 2px 8px; border-radius: 6px;
    font-variant-numeric: tabular-nums; white-space: nowrap;
  }
  .pv-vc-stat-chip span {
    font-weight: 400; color: var(--color-muted); margin-left: 2px;
  }
  .pv-vc-detail-wrap {
    display: grid;
    grid-template-rows: 0fr;
    transition: grid-template-rows 0.4s cubic-bezier(0.16, 1, 0.3, 1);
  }
  .pv-voice-card.open .pv-vc-detail-wrap {
    grid-template-rows: 1fr;
  }
  .pv-vc-detail-clip {
    overflow: hidden;
    min-height: 0;
  }
  .pv-vc-detail-inner {
    padding: 0 16px 16px;
    display: flex;
    flex-direction: column;
    gap: 16px;
    border-top: 1px solid var(--color-border);
    padding-top: 14px;
  }

  /* Rhythm stats 2x2 grid */
  .pv-rhythm-stats-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px 16px;
    margin-top: 10px;
    padding-top: 8px;
    border-top: 1px solid var(--color-border);
  }

  /* Footer row */
  .pv-footer-row {
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: 12px;
    padding: 12px 16px;
    display: flex; align-items: center; justify-content: space-between;
    flex-shrink: 0;
  }
  .pv-footer-btn {
    padding: 5px 10px; font-size: 10px; font-weight: 500; font-family: inherit;
    border: 1px solid var(--color-border); border-radius: 6px;
    background: transparent; color: var(--color-muted); cursor: pointer;
    transition: all 0.15s;
  }
  .pv-footer-btn:hover { border-color: var(--color-secondary); color: var(--color-foreground); }
  .pv-footer-btn:disabled { opacity: 0.4; cursor: not-allowed; }
  .pv-footer-export {
    padding: 6px 14px; font-size: 11px; font-weight: 600; font-family: inherit;
    color: white; background: var(--color-primary); border: none; border-radius: 8px;
    cursor: pointer; transition: opacity 0.15s;
  }
  .pv-footer-export:hover { opacity: 0.9; }
  .pv-footer-export:disabled { opacity: 0.4; cursor: not-allowed; }

  /* Voice Dimensions */
  .pv-dims {
    display: flex;
    flex-direction: column;
    gap: 12px;
    margin-top: 10px;
  }
  .pv-dim-row {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .pv-dim-header {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
  }
  .pv-dim-label {
    font-size: 11px;
    font-weight: 600;
    color: var(--color-foreground);
  }
  .pv-dim-value {
    font-size: 10px;
    color: var(--color-muted);
    text-transform: lowercase;
  }
  .pv-dim-track {
    position: relative;
    height: 4px;
    background: var(--color-tint);
    border-radius: 100px;
  }
  .pv-dim-indicator {
    position: absolute;
    top: -3px;
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--color-secondary);
    border: 2px solid var(--color-surface);
    box-shadow: 0 1px 3px rgba(0,0,0,0.15);
    transform: translateX(-50%);
  }
  .pv-dim-ends {
    display: flex;
    justify-content: space-between;
    margin-top: 2px;
  }
  .pv-dim-end {
    font-size: 9px;
    color: var(--color-muted);
    opacity: 0.7;
  }

  /* Pattern Depth */
  .pv-depth {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
    margin-top: 10px;
  }
  .pv-depth-item {
    display: flex;
    align-items: baseline;
    gap: 6px;
  }
  .pv-depth-count {
    font-size: 18px;
    font-weight: 700;
    color: var(--color-foreground);
    line-height: 1;
    font-variant-numeric: tabular-nums;
  }
  .pv-depth-name {
    font-size: 10px;
    color: var(--color-muted);
    line-height: 1.3;
  }
  .pv-depth-full {
    grid-column: 1 / -1;
    padding-top: 4px;
    border-top: 1px solid var(--color-border);
    margin-top: 2px;
  }

  /* Sentence Rhythm */
  .pv-rhythm-bar {
    display: flex;
    height: 8px;
    border-radius: 100px;
    overflow: hidden;
    gap: 1px;
  }
  .pv-rhythm-seg {
    height: 100%;
    transition: width 0.6s cubic-bezier(0.16, 1, 0.3, 1);
  }
  .pv-rhythm-short { background: var(--color-secondary); border-radius: 100px 0 0 100px; }
  .pv-rhythm-medium { background: var(--color-accent); }
  .pv-rhythm-long { background: var(--color-warning); }
  .pv-rhythm-vlong { background: #C23B2A; border-radius: 0 100px 100px 0; }

  .pv-rhythm-legend {
    display: flex;
    gap: 10px;
    margin-top: 8px;
    flex-wrap: wrap;
  }
  .pv-rhythm-legend-item {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 9px;
    color: var(--color-muted);
  }
  .pv-rhythm-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .pv-rhythm-stat {
    display: flex;
    flex-direction: column;
  }
  .pv-rhythm-stat-val {
    font-size: 14px;
    font-weight: 700;
    color: var(--color-foreground);
    font-variant-numeric: tabular-nums;
  }
  .pv-rhythm-stat-lbl {
    font-size: 9px;
    color: var(--color-muted);
  }

  /* Format Cards */
  .pv-format-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-top: 10px;
  }
  .pv-format-row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    background: var(--color-tint);
    border-radius: 8px;
  }
  .pv-format-accent {
    width: 3px;
    height: 24px;
    border-radius: 2px;
    flex-shrink: 0;
  }
  .pv-format-name {
    font-size: 11px;
    font-weight: 600;
    color: var(--color-foreground);
    flex: 1;
  }
  .pv-format-stats {
    display: flex;
    gap: 10px;
  }
  .pv-format-stat {
    font-size: 10px;
    color: var(--color-muted);
    font-variant-numeric: tabular-nums;
  }
  .pv-format-stat :global(strong) {
    font-weight: 600;
    color: var(--color-foreground);
  }

  /* Guided Edit */
  .ge-card {
    position: relative;
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: 12px;
    padding: 14px 16px;
  }
  .ge-card::before {
    content: '';
    position: absolute;
    top: 0; left: 12px; right: 12px;
    height: 1px;
    background: linear-gradient(90deg, transparent, var(--color-secondary), transparent);
    opacity: 0.25;
    border-radius: 1px;
  }
  .ge-input-row {
    display: flex;
    gap: 8px;
    margin-top: 10px;
    align-items: stretch;
  }
  .ge-input {
    flex: 1;
    padding: 8px 12px;
    font-size: 12px;
    font-family: inherit;
    border: 1.5px solid var(--color-border);
    border-radius: 8px;
    background: var(--color-background);
    color: var(--color-foreground);
    outline: none;
    transition: border-color 0.15s;
  }
  .ge-input:focus { border-color: var(--color-secondary); }
  .ge-input::placeholder { color: var(--color-muted); opacity: 0.6; }
  .ge-submit {
    padding: 8px 14px;
    font-size: 11px;
    font-weight: 600;
    font-family: inherit;
    color: white;
    background: var(--color-secondary);
    border: none;
    border-radius: 8px;
    cursor: pointer;
    transition: all 0.15s;
    white-space: nowrap;
    flex-shrink: 0;
  }
  .ge-submit:hover { opacity: 0.9; }
  .ge-submit:disabled { opacity: 0.4; cursor: not-allowed; }
  .ge-format-row { display: flex; gap: 6px; margin-top: 8px; }
  .ge-format-pill {
    padding: 3px 8px;
    font-size: 10px;
    font-family: inherit;
    border-radius: 4px;
    cursor: pointer;
    transition: all 0.15s;
    border: 1px solid var(--color-border);
    background: transparent;
    color: var(--color-muted);
  }
  .ge-format-pill.active {
    background: var(--color-secondary);
    color: white;
    border-color: var(--color-secondary);
  }
  .ge-loading {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 10px;
  }
  .ge-result {
    margin-top: 10px;
    padding: 10px 12px;
    background: var(--color-tint);
    border-radius: 8px;
    animation: pv-enter 0.3s cubic-bezier(0.16, 1, 0.3, 1);
  }
  .ge-result-noop {
    background: var(--color-surface);
    border: 1px solid var(--color-border);
  }
  .ge-result-msg { font-size: 11px; font-weight: 500; }
</style>
