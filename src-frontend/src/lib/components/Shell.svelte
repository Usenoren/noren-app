<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { listen } from "@tauri-apps/api/event";
  import { checkPermissions, requestPermissions, getSettings, getProfileOverview, migrateProfileToServer } from "$lib/api/tauri";
  import { refresh as refreshSubscription } from "$lib/stores/subscription.svelte";
  import GenerateView from "./GenerateView.svelte";
  import SettingsView from "./SettingsView.svelte";
  import ProfilesView from "./ProfilesView.svelte";
  import NorenMark from "./NorenMark.svelte";

  type View = "generate" | "profiles" | "settings";
  let view: View = $state("generate");
  let hasPermissions = $state(true);
  let hasProfile = $state(true);
  let noApiKey = $state(false);
  let loading = $state(true);
  let detectedApp = $state("");

  function closeWindow() {
    getCurrentWindow().hide();
  }

  function refreshApiKeyStatus() {
    getSettings().then((settings) => {
      noApiKey = settings.inference_mode === "byok" && !settings.has_key && settings.provider.requiresKey;
    });
  }

  $effect(() => {
    const cleanups: (() => void)[] = [];

    listen<string>("navigate", (event) => {
      const target = event.payload as View;
      if (["generate", "profiles", "settings"].includes(target)) {
        view = target;
        if (target === "generate") refreshApiKeyStatus();
      }
    }).then((fn) => cleanups.push(fn));

    listen<{ name: string; format: string | null }>("detected-app", (event) => {
      detectedApp = event.payload.name;
    }).then((fn) => cleanups.push(fn));

    // Re-check permissions and API key each time the window is shown
    listen("tauri://focus", () => {
      checkPermissions().then((ok) => {
        hasPermissions = ok;
      });
      refreshApiKeyStatus();
      refreshSubscription();
    }).then((fn) => cleanups.push(fn))

    checkPermissions().then((ok) => {
      hasPermissions = ok;
    });

    // Check profile status and API key availability
    Promise.all([getSettings(), getProfileOverview()]).then(([settings, profile]) => {
      hasProfile = profile.exists;
      if (
        settings.inference_mode === "byok" &&
        !settings.has_key &&
        settings.provider.requiresKey
      ) {
        noApiKey = true;
        view = "settings";
      }
      loading = false;

      // Load subscription status for feature gating
      if (settings.noren_pro_logged_in) {
        refreshSubscription();

        // Auto-migrate local profile to server for Pro users
        if (settings.inference_mode === "noren_pro" && profile.exists && !profile.is_server) {
          migrateProfileToServer().catch(() => {
            // Migration failed silently — user can retry manually
          });
        }
      }
    });

    return () => cleanups.forEach((fn) => fn());
  });

  async function handleRequestPermissions() {
    // Open System Settings → Accessibility
    await requestPermissions();

    // Poll until the user grants access (they need to toggle in System Settings)
    const maxAttempts = 60; // 30 seconds
    for (let i = 0; i < maxAttempts; i++) {
      await new Promise((r) => setTimeout(r, 500));
      const ok = await checkPermissions();
      if (ok) {
        hasPermissions = true;
        return;
      }
    }
  }
</script>

{#if loading}
  <div class="flex items-center justify-center h-screen popup-shell">
    <span class="text-xs text-muted">Loading...</span>
  </div>
{:else}
<div class="flex flex-col h-screen overflow-hidden popup-shell">
  <!-- Chrome bar -->
  <div
    data-tauri-drag-region
    class="h-8 flex items-center px-3 gap-1.5 shrink-0 border-b border-border"
    style="background:var(--color-bg-rail)"
  >
    <!-- Window dots -->
    <button
      onclick={closeWindow}
      class="w-[9px] h-[9px] rounded-full cursor-pointer shrink-0 z-[1]"
      style="background:#FF5F57;opacity:0.8"
      aria-label="Close"
    ></button>
    <div class="w-[9px] h-[9px] rounded-full shrink-0 z-[1]" style="background:#FEBD2E;opacity:0.8"></div>
    <div class="w-[9px] h-[9px] rounded-full shrink-0 z-[1]" style="background:#27CA40;opacity:0.8"></div>

    <!-- Noren mark -->
    <div class="ml-1.5 z-[1]" style="color:var(--color-muted)">
      <NorenMark width={12} height={14} />
    </div>

    <!-- Spacer -->
    <div data-tauri-drag-region class="flex-1"></div>

    <!-- Icon nav buttons -->
    <div class="flex gap-0.5 z-[1]">
      <button
        onclick={() => { view = "generate"; refreshApiKeyStatus(); }}
        class="w-6 h-[22px] rounded flex items-center justify-center cursor-pointer transition-colors hover:bg-foreground/[0.04]"
        aria-label="Weave"
        title="Weave"
      >
        <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="color:{view === 'generate' ? 'var(--color-accent)' : 'var(--color-muted)'}">
          <path d="M12 19l7-7 3 3-7 7-3-3z"/><path d="M18 13l-1.5-7.5L2 2l3.5 14.5L13 18l5-5z"/>
        </svg>
      </button>
      <button
        onclick={() => { view = "profiles"; }}
        class="w-6 h-[22px] rounded flex items-center justify-center cursor-pointer transition-colors hover:bg-foreground/[0.04]"
        aria-label="Profiles"
        title="Voice profiles"
      >
        <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="color:{view === 'profiles' ? 'var(--color-accent)' : 'var(--color-muted)'}">
          <path d="M20 21v-2a4 4 0 00-4-4H8a4 4 0 00-4 4v2"/><circle cx="12" cy="7" r="4"/>
        </svg>
      </button>
      <button
        onclick={() => { view = "settings"; }}
        class="w-6 h-[22px] rounded flex items-center justify-center cursor-pointer transition-colors hover:bg-foreground/[0.04]"
        aria-label="Settings"
        title="Settings"
      >
        <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="color:{view === 'settings' ? 'var(--color-accent)' : 'var(--color-muted)'}">
          <circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 00.33 1.82l.06.06a2 2 0 010 2.83 2 2 0 01-2.83 0l-.06-.06a1.65 1.65 0 00-1.82-.33 1.65 1.65 0 00-1 1.51V21a2 2 0 01-4 0v-.09A1.65 1.65 0 009 19.4a1.65 1.65 0 00-1.82.33l-.06.06a2 2 0 01-2.83-2.83l.06-.06A1.65 1.65 0 004.68 15a1.65 1.65 0 00-1.51-1H3a2 2 0 010-4h.09A1.65 1.65 0 004.6 9a1.65 1.65 0 00-.33-1.82l-.06-.06a2 2 0 012.83-2.83l.06.06A1.65 1.65 0 009 4.68a1.65 1.65 0 001-1.51V3a2 2 0 014 0v.09a1.65 1.65 0 001 1.51 1.65 1.65 0 001.82-.33l.06-.06a2 2 0 012.83 2.83l-.06.06A1.65 1.65 0 0019.4 9a1.65 1.65 0 001.51 1H21a2 2 0 010 4h-.09a1.65 1.65 0 00-1.51 1z"/>
        </svg>
      </button>
    </div>
  </div>

  <!-- Accent thread -->
  <div class="divider-thread"></div>

  <!-- Detected app context -->
  {#if detectedApp && view === "generate"}
    <div class="flex items-center gap-1.5 px-3 py-1.5 bg-tint border-b border-border shrink-0">
      <span class="text-[9px] font-semibold text-foreground">{detectedApp}</span>
      <div class="w-1 h-1 rounded-full bg-signal shrink-0"></div>
      <span class="text-[9px] text-muted">connected</span>
    </div>
  {/if}

  <!-- Accessibility permission banner -->
  {#if !hasPermissions && view === "generate"}
    <div class="px-4 py-2 bg-tint border-b border-border shrink-0">
      <div class="flex items-center justify-between">
        <p class="text-xs text-warning">
          Accessibility access needed for text capture
        </p>
        <button
          onclick={handleRequestPermissions}
          class="text-xs text-primary hover:text-foreground font-medium cursor-pointer uppercase tracking-wide"
        >
          Grant
        </button>
      </div>
    </div>
  {/if}

  <!-- Main content -->
  <div class="flex-1 min-h-0">
    {#if view === "generate"}
      <GenerateView isPopup={true} {hasProfile} {noApiKey} />
    {:else if view === "profiles"}
      <ProfilesView />
    {:else}
      <SettingsView />
    {/if}
  </div>
</div>
{/if}
