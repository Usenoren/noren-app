<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { listen } from "@tauri-apps/api/event";
  import { checkPermissions, requestPermissions, getSettings, getProfileOverview, getSubscriptionStatus, migrateProfileToServer } from "$lib/api/tauri";
  import GenerateView from "./GenerateView.svelte";
  import SettingsView from "./SettingsView.svelte";
  import ProfilesView from "./ProfilesView.svelte";
  import ExtractView from "./ExtractView.svelte";
  import OnboardingView from "./OnboardingView.svelte";

  type View = "generate" | "profiles" | "extract" | "settings" | "onboarding";
  let view: View = $state("generate");
  let hasPermissions = $state(true);
  let needsOnboarding = $state(false);
  let loading = $state(true);
  let canExtract = $state(false);

  const viewLabels: Record<View, string> = {
    generate: "noren",
    profiles: "Profiles",
    extract: "Extract",
    settings: "Settings",
    onboarding: "Welcome",
  };

  function closeWindow() {
    getCurrentWindow().hide();
  }

  $effect(() => {
    let cleanup: (() => void) | undefined;
    let cleanup2: (() => void) | undefined;

    listen<string>("navigate", (event) => {
      const target = event.payload as View;
      if (["generate", "profiles", "extract", "settings"].includes(target)) {
        view = target;
      }
    }).then((fn) => {
      cleanup = fn;
    });

    // Re-check permissions each time the window is shown
    // (user may have granted access in System Settings while the app was hidden)
    listen("tauri://focus", () => {
      checkPermissions().then((ok) => {
        hasPermissions = ok;
      });
    }).then((fn) => {
      cleanup2 = fn;
    });

    checkPermissions().then((ok) => {
      hasPermissions = ok;
    });

    // Check if we need onboarding (no profile) or settings (no key in BYOK mode)
    Promise.all([getSettings(), getProfileOverview()]).then(([settings, profile]) => {
      if (!profile.exists) {
        needsOnboarding = true;
        view = "onboarding";
      } else if (
        settings.inference_mode === "byok" &&
        !settings.has_key &&
        settings.provider.requiresKey
      ) {
        view = "settings";
      }
      loading = false;

      // Check extraction access
      if (settings.noren_pro_logged_in) {
        getSubscriptionStatus().then((sub) => {
          canExtract = sub.can_extract;
        }).catch(() => { canExtract = false; });

        // Auto-migrate local profile to server for Pro users
        if (settings.inference_mode === "noren_pro" && profile.exists && !profile.is_server) {
          migrateProfileToServer().catch(() => {
            // Migration failed silently — user can retry manually
          });
        }
      }
    });

    return () => { cleanup?.(); cleanup2?.(); };
  });

  function handleOnboardingComplete() {
    needsOnboarding = false;
    view = "generate";
  }

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
  <div class="flex items-center justify-center h-screen bg-background">
    <span class="text-xs text-muted">Loading...</span>
  </div>
{:else}
<div class="flex flex-col h-screen overflow-hidden bg-background">
  <!-- Title bar -->
  <div
    data-tauri-drag-region
    class="flex items-center justify-between px-4 py-2 border-b border-border bg-surface shrink-0"
  >
    <div data-tauri-drag-region class="flex items-center gap-2">
      {#if view !== "generate" && view !== "onboarding"}
        <button
          onclick={() => { view = "generate"; }}
          class="text-muted hover:text-primary transition-colors text-sm cursor-pointer"
          aria-label="Back"
        >
          &larr;
        </button>
      {/if}
      <span data-tauri-drag-region class="text-xs font-medium text-foreground pointer-events-none tracking-wide {view === 'generate' || view === 'onboarding' ? 'font-heading' : ''}">
        {viewLabels[view]}
      </span>
    </div>
    <div class="flex items-center gap-3">
      {#if view === "generate"}
        {#if canExtract}
          <button
            onclick={() => { view = "extract"; }}
            class="text-muted hover:text-primary transition-colors text-[10px] cursor-pointer uppercase tracking-wide"
            aria-label="Extract"
            title="Extract voice profile"
          >
            EXT
          </button>
        {/if}
        <button
          onclick={() => { view = "profiles"; }}
          class="text-muted hover:text-primary transition-colors text-[10px] cursor-pointer uppercase tracking-wide"
          aria-label="Profiles"
          title="Voice profiles"
        >
          PRF
        </button>
        <button
          onclick={() => { view = "settings"; }}
          class="text-muted hover:text-primary transition-colors text-[10px] cursor-pointer uppercase tracking-wide"
          aria-label="Settings"
          title="Settings"
        >
          SET
        </button>
      {/if}
      <button
        onclick={closeWindow}
        class="text-muted hover:text-error transition-colors text-lg leading-none cursor-pointer"
        aria-label="Close"
      >
        &times;
      </button>
    </div>
  </div>

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
    {#if view === "onboarding"}
      <OnboardingView onComplete={handleOnboardingComplete} />
    {:else if view === "generate"}
      <GenerateView />
    {:else if view === "profiles"}
      <ProfilesView />
    {:else if view === "extract"}
      <ExtractView />
    {:else}
      <SettingsView />
    {/if}
  </div>
</div>
{/if}
