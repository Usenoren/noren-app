<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { listen } from "@tauri-apps/api/event";
  import { checkPermissions, requestPermissions, getSettings, getProfileOverview } from "$lib/api/tauri";
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
    listen<string>("navigate", (event) => {
      const target = event.payload as View;
      if (["generate", "profiles", "extract", "settings"].includes(target)) {
        view = target;
      }
    }).then((fn) => {
      cleanup = fn;
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
    });

    return () => cleanup?.();
  });

  function handleOnboardingComplete() {
    needsOnboarding = false;
    view = "generate";
  }

  async function handleRequestPermissions() {
    const granted = await requestPermissions();
    hasPermissions = granted;
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
        <button
          onclick={() => { view = "extract"; }}
          class="text-muted hover:text-primary transition-colors text-[10px] cursor-pointer uppercase tracking-wide"
          aria-label="Extract"
          title="Extract voice profile"
        >
          EXT
        </button>
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
