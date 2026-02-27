<script lang="ts">
  import {
    getProfileOverview,
    readProfileContent,
    saveProfileEdit,
    type ProfileOverview,
    type ProfileContent,
  } from "$lib/api/tauri";
  import LoadingSpinner from "./LoadingSpinner.svelte";

  let overview = $state<ProfileOverview | null>(null);
  let profile = $state<ProfileContent | null>(null);
  let activeTab = $state("core");
  let isEditing = $state(false);
  let editContent = $state("");
  let isSaving = $state(false);
  let error = $state("");
  let saveSuccess = $state(false);

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
      if (overview.exists) {
        profile = await readProfileContent();
      }
    } catch (e) {
      error = String(e);
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
      error = String(e);
    } finally {
      isSaving = false;
    }
  }

  function switchTab(tab: string) {
    if (isEditing) cancelEditing();
    activeTab = tab;
    saveSuccess = false;
  }
</script>

<div class="flex flex-col gap-3 h-full p-4 overflow-hidden animate-fade-in-up">
  {#if !overview}
    <div class="flex items-center justify-center h-full">
      <LoadingSpinner />
    </div>
  {:else if !overview.exists}
    <!-- No profile -->
    <div class="flex flex-col items-center justify-center h-full gap-3 text-center">
      <p class="text-sm text-muted">No voice profile found.</p>
      <p class="text-xs text-muted max-w-[280px] leading-relaxed">
        Create a profile using the CLI:
      </p>
      <code class="px-3 py-1.5 bg-surface border border-border rounded text-xs text-secondary font-mono">
        noren extract --samples your-writing.txt
      </code>
      <p class="text-[10px] text-muted mt-2">
        Profile directory: {overview.path}
      </p>
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
    </div>

    <!-- Content -->
    <div class="flex-1 flex flex-col min-h-0">
      {#if isEditing}
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

    <!-- Actions -->
    <div class="flex items-center justify-between shrink-0">
      <span class="text-[10px] text-muted">
        {#if saveSuccess}
          <span class="text-signal">Saved</span>
        {:else}
          {activeTab === "core" ? "Core Identity" : activeTab} &middot; {displayContent.split("\n").length} lines
        {/if}
      </span>
      <div class="flex gap-2">
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
        {/if}
      </div>
    </div>

    <!-- Error -->
    {#if error}
      <div class="p-2 bg-surface border border-error/30 rounded-md text-xs text-error shrink-0">
        {error}
      </div>
    {/if}
  {/if}
</div>
