<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import { checkPermissions, requestPermissions, getSettings, getProfileOverview, migrateProfileToServer } from "$lib/api/tauri";
  import { syncThemeFromConfig } from "$lib/stores/theme.svelte";
  import { refresh as refreshSubscription, canExtract } from "$lib/stores/subscription.svelte";
  import { isRefreshAvailable } from "$lib/stores/patches.svelte";
  import {
    init as initExtraction,
    getIsExtracting,
    getCurrentFormat,
    getCurrentIndex,
    getTotalFormats,
    getProgress,
    getError as getExtractionError,
    isDone as isExtractionDone,
    dismiss as dismissExtraction,
    retry as retryExtraction,
    canRetry as canRetryExtraction,
    isFirstCompletion,
  } from "$lib/stores/extraction.svelte";
  import GenerateView from "./GenerateView.svelte";
  import RepurposeView from "./RepurposeView.svelte";
  import ChatView from "./ChatView.svelte";
  import SettingsView from "./SettingsView.svelte";
  import ProfilesView from "./ProfilesView.svelte";
  import ExtractView from "./ExtractView.svelte";
  import AccountView from "./AccountView.svelte";
  import HelpView from "./HelpView.svelte";
  import OnboardingView from "./OnboardingView.svelte";
  import NorenMark from "./NorenMark.svelte";
  import AnnouncementBell from "./AnnouncementBell.svelte";
  import ToastContainer from "./ToastContainer.svelte";
  import SpotlightTour from "./SpotlightTour.svelte";
  import { toastWarning } from "$lib/stores/toast.svelte";
  import { startTour, type TourStep } from "$lib/stores/tour.svelte";

  const appTourSteps: TourStep[] = [
    { target: "format", description: "Every format has its own rhythm. Pick one and Noren adjusts length, structure, and tone to match.", placement: "bottom" },
    { target: "adapt", description: "Have a draft already? Switch to Adapt. Paste your text and Noren rewrites it in your voice.", placement: "bottom" },
    { target: "chat", description: "Your thinking space. Brainstorm, research, or talk through ideas before you write.", placement: "right" },
    { target: null, description: "Press \u2318K in any text field on your Mac and Noren writes in your voice. Telegram, Mail, Slack, anywhere.", placement: "center" },
    { target: "input", description: "You\u2019re ready. Describe what you want to write and Noren takes it from here.", placement: "top" },
  ];

  type View = "generate" | "repurpose" | "chat" | "profiles" | "extract" | "account" | "settings" | "help" | "onboarding";
  let view: View = $state("generate");
  let hasPermissions = $state(true);
  let needsOnboarding = $state(false);
  let loading = $state(true);

  const navItems: { id: View; label: string; icon: string }[] = [
    { id: "generate", label: "Weave", icon: "pen" },
    // { id: "repurpose", label: "Repurpose", icon: "refresh" },
    { id: "chat", label: "Chat", icon: "chat" },
    { id: "profiles", label: "Profiles", icon: "user" },
    { id: "extract", label: "Extract", icon: "wand" },
    { id: "account", label: "Account", icon: "badge" },
    { id: "settings", label: "Settings", icon: "gear" },
    { id: "help", label: "Help", icon: "help" },
  ];

  onMount(() => {
    document.documentElement.style.fontSize = "14px";
  });

  $effect(() => {
    initExtraction();
    const cleanups: (() => void)[] = [];

    listen<string>("navigate", (event) => {
      const target = event.payload as View;
      if (["generate", "repurpose", "chat", "profiles", "extract", "account", "settings", "help"].includes(target)) {
        view = target;
      }
    }).then((fn) => cleanups.push(fn));

    listen("tauri://focus", () => {
      checkPermissions().then((ok) => {
        hasPermissions = ok;
      });
      refreshSubscription();
    }).then((fn) => cleanups.push(fn));

    checkPermissions().then((ok) => {
      hasPermissions = ok;
    });

    syncThemeFromConfig();
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

      if (settings.noren_pro_logged_in) {
        refreshSubscription();

        if (settings.noren_pro_logged_in && profile.exists && !profile.is_server) {
          migrateProfileToServer().catch(() => toastWarning("Profile sync to server failed"));
        }
      }
    }).catch(() => {
      loading = false;
    });

    return () => cleanups.forEach((fn) => fn());
  });

  function handleOnboardingComplete(targetView?: string) {
    needsOnboarding = false;
    const validViews: View[] = ["generate", "chat", "profiles", "extract", "account", "settings", "help"];
    if (targetView && validViews.includes(targetView as View)) {
      view = targetView as View;
    } else {
      view = "generate";
    }
    // Trigger tour after onboarding lands on generate
    if (!targetView || targetView === "generate") {
      setTimeout(() => startTour(appTourSteps), 700);
    }
  }

  // Also trigger tour for returning users who completed onboarding in a previous session
  // but haven't seen the tour yet (e.g. app was closed mid-onboarding)
  $effect(() => {
    if (!loading && !needsOnboarding && view === "generate") {
      setTimeout(() => startTour(appTourSteps), 700);
    }
  });

  async function handleRequestPermissions() {
    await requestPermissions();
    const maxAttempts = 60;
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
    <span class="text-sm text-muted">Loading...</span>
  </div>
{:else}
<div class="flex flex-col h-screen overflow-hidden bg-background">
  {#if view === "onboarding"}
    <!-- Onboarding takes full width -->
    <div class="flex-1 min-h-0">
      <OnboardingView onComplete={handleOnboardingComplete} />
    </div>
  {:else}
    <!-- Main layout: sidebar + content -->
    <div class="flex flex-1 min-h-0">
      <!-- Nav rail -->
      <nav class="w-14 shrink-0 flex flex-col items-center py-2.5 gap-0.5 border-r border-border nav-rail-threads" style="background:var(--color-bg-rail)">
        <div class="flex flex-col items-center gap-0.5">
          {#each navItems as item}
            <button
              data-tour={item.id === "chat" ? "chat" : undefined}
              onclick={() => { view = item.id; }}
              class="w-[44px] h-[48px] flex flex-col items-center justify-center gap-[3px] rounded-md transition-colors cursor-pointer relative
                {view === item.id
                  ? 'nav-active-indicator'
                  : 'hover:bg-foreground/[0.04]'}"
            >
              {#if item.icon === "pen"}
                <svg class="shrink-0" style="width:17px;height:17px;color:{view === item.id ? 'var(--color-accent)' : 'var(--color-muted)'}" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                  <path d="M12 19l7-7 3 3-7 7-3-3z"/><path d="M18 13l-1.5-7.5L2 2l3.5 14.5L13 18l5-5z"/>
                </svg>
              {:else if item.icon === "refresh"}
                <svg class="shrink-0" style="width:17px;height:17px;color:{view === item.id ? 'var(--color-accent)' : 'var(--color-muted)'}" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M19.5 12c0-1.232-.046-2.453-.138-3.662a4.006 4.006 0 00-3.7-3.7 48.678 48.678 0 00-7.324 0 4.006 4.006 0 00-3.7 3.7c-.017.22-.032.441-.046.662M19.5 12l3-3m-3 3l-3-3m-12 3c0 1.232.046 2.453.138 3.662a4.006 4.006 0 003.7 3.7 48.656 48.656 0 007.324 0 4.006 4.006 0 003.7-3.7c.017-.22.032-.441.046-.662M4.5 12l3 3m-3-3l-3 3"/>
                </svg>
              {:else if item.icon === "chat"}
                <svg class="shrink-0" style="width:17px;height:17px;color:{view === item.id ? 'var(--color-accent)' : 'var(--color-muted)'}" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                  <path d="M21 15a2 2 0 01-2 2H7l-4 4V5a2 2 0 012-2h14a2 2 0 012 2z"/>
                </svg>
              {:else if item.icon === "user"}
                <svg class="shrink-0" style="width:17px;height:17px;color:{view === item.id ? 'var(--color-accent)' : 'var(--color-muted)'}" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                  <path d="M20 21v-2a4 4 0 00-4-4H8a4 4 0 00-4 4v2"/><circle cx="12" cy="7" r="4"/>
                </svg>
              {:else if item.icon === "wand"}
                <svg class="shrink-0" style="width:17px;height:17px;color:{view === item.id ? 'var(--color-accent)' : 'var(--color-muted)'}" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                  <path d="M12 10a2 2 0 0 0-2 2c0 1.02-.1 2.51-.26 4"/><path d="M14 13.12c0 2.38 0 6.38-1 8.88"/><path d="M17.29 21.02c.12-.6.43-2.3.5-3.02"/><path d="M2 12a10 10 0 0 1 18-6"/><path d="M2 16h.01"/><path d="M21.8 16c.2-2 .131-5.354 0-6"/><path d="M5 19.5C5.5 18 6 15 6 12a6 6 0 0 1 .34-2"/><path d="M8.65 22c.21-.66.45-1.32.57-2"/><path d="M9 6.8a6 6 0 0 1 9 5.2v2"/>
                </svg>
              {:else if item.icon === "badge"}
                <svg class="shrink-0" style="width:17px;height:17px;color:{view === item.id ? 'var(--color-accent)' : 'var(--color-muted)'}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <rect x="3" y="4" width="18" height="16" rx="2"/>
                  <circle cx="9" cy="11" r="2.5"/>
                  <path d="M15 10h2M15 14h2M5 20c0-2 2-3.5 4-3.5s4 1.5 4 3.5"/>
                </svg>
              {:else if item.icon === "gear"}
                <svg class="shrink-0" style="width:17px;height:17px;color:{view === item.id ? 'var(--color-accent)' : 'var(--color-muted)'}" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                  <circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 00.33 1.82l.06.06a2 2 0 010 2.83 2 2 0 01-2.83 0l-.06-.06a1.65 1.65 0 00-1.82-.33 1.65 1.65 0 00-1 1.51V21a2 2 0 01-4 0v-.09A1.65 1.65 0 009 19.4a1.65 1.65 0 00-1.82.33l-.06.06a2 2 0 01-2.83-2.83l.06-.06A1.65 1.65 0 004.68 15a1.65 1.65 0 00-1.51-1H3a2 2 0 010-4h.09A1.65 1.65 0 004.6 9a1.65 1.65 0 00-.33-1.82l-.06-.06a2 2 0 012.83-2.83l.06.06A1.65 1.65 0 009 4.68a1.65 1.65 0 001-1.51V3a2 2 0 014 0v.09a1.65 1.65 0 001 1.51 1.65 1.65 0 001.82-.33l.06-.06a2 2 0 012.83 2.83l-.06.06A1.65 1.65 0 0019.4 9a1.65 1.65 0 001.51 1H21a2 2 0 010 4h-.09a1.65 1.65 0 00-1.51 1z"/>
                </svg>
              {:else if item.icon === "help"}
                <svg class="shrink-0" style="width:17px;height:17px;color:{view === item.id ? 'var(--color-accent)' : 'var(--color-muted)'}" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                  <path d="M9.879 7.519c1.171-1.025 3.071-1.025 4.242 0 1.172 1.025 1.172 2.687 0 3.712-.203.179-.43.326-.67.442-.745.361-1.45.999-1.45 1.827v.75M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0Zm-9 5.25h.008v.008H12v-.008Z"/>
                </svg>
              {/if}
              <span class="font-heading italic text-[8px] font-normal tracking-normal" style="color:{view === item.id ? 'var(--color-accent)' : 'var(--color-muted)'}">
                {item.label}
              </span>
              {#if item.id === "extract" && !canExtract()}
                <span class="absolute top-[5px] right-[5px] w-[5px] h-[5px] rounded-full bg-accent"></span>
              {/if}
              {#if item.id === "profiles" && isRefreshAvailable()}
                <span class="absolute top-[5px] right-[5px] w-[5px] h-[5px] rounded-full bg-secondary"></span>
              {/if}
            </button>
          {/each}
        </div>

        <!-- Bottom: announcements + branding -->
        <div class="mt-auto flex flex-col items-center gap-1.5 pb-0.5">
          <AnnouncementBell />
          <div class="opacity-50 hover:opacity-80 transition-opacity" style="color:var(--color-muted)">
            <NorenMark width={16} height={19} />
          </div>
        </div>
      </nav>

      <!-- Content area -->
      <div class="flex-1 min-h-0 flex flex-col overflow-hidden">
        <!-- Accessibility permission banner -->
        {#if !hasPermissions && view === "generate"}
          <div class="px-6 py-2.5 bg-tint border-b border-border shrink-0">
            <div class="flex items-center justify-between max-w-3xl mx-auto">
              <p class="text-sm text-warning">
                Accessibility access needed for text capture
              </p>
              <button
                onclick={handleRequestPermissions}
                class="text-sm text-primary hover:text-foreground font-medium cursor-pointer uppercase tracking-wide"
              >
                Grant
              </button>
            </div>
          </div>
        {/if}

        <!-- Extraction progress banner -->
        {#if getIsExtracting()}
          <div class="px-6 py-2 bg-primary/5 border-b border-primary/20 shrink-0">
            <div class="flex items-center gap-3 max-w-3xl mx-auto">
              <div class="w-3 h-3 border-2 border-primary border-t-transparent rounded-full animate-spin shrink-0"></div>
              <div class="flex-1 min-w-0">
                <p class="text-xs font-medium text-foreground">
                  Building voice profile{getTotalFormats() > 1 ? ` — ${getCurrentFormat()} (${getCurrentIndex()}/${getTotalFormats()})` : ""}
                </p>
                {#if getProgress()}
                  <p class="text-[10px] text-muted">{getProgress()?.progress}% complete</p>
                {/if}
              </div>
              <div class="w-20 h-1 bg-primary/10 rounded-full overflow-hidden shrink-0">
                <div
                  class="h-full bg-primary rounded-full transition-all duration-500"
                  style="width: {getProgress()?.progress ?? 0}%"
                ></div>
              </div>
            </div>
          </div>
        {:else if isExtractionDone()}
          <div class="px-6 py-2 bg-signal/5 border-b border-signal/20 shrink-0">
            <div class="flex items-center justify-between max-w-3xl mx-auto">
              <div class="flex items-center gap-2">
                <svg class="w-3.5 h-3.5 text-signal" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7" />
                </svg>
                <p class="text-xs font-medium text-foreground">
                  {isFirstCompletion() ? "Your voice profile is ready. Try your first generation." : "Voice profile ready"}
                </p>
              </div>
              <div class="flex items-center gap-3">
                {#if isFirstCompletion()}
                  <button onclick={() => { view = "profiles"; dismissExtraction(); }} class="text-[10px] text-secondary font-medium hover:text-foreground cursor-pointer uppercase tracking-wide">View profile</button>
                  <button onclick={() => { view = "generate"; dismissExtraction(); }} class="text-[10px] text-accent font-medium hover:text-foreground cursor-pointer uppercase tracking-wide">Try generating</button>
                {/if}
                <button onclick={dismissExtraction} class="text-[10px] text-muted hover:text-foreground cursor-pointer">Dismiss</button>
              </div>
            </div>
          </div>
        {:else if getExtractionError()}
          <div class="px-6 py-2 bg-warning/5 border-b border-warning/20 shrink-0">
            <div class="flex items-center justify-between max-w-3xl mx-auto">
              <p class="text-xs text-warning">{getExtractionError()}</p>
              <div class="flex items-center gap-3 shrink-0">
                {#if canRetryExtraction()}
                  <button onclick={retryExtraction} class="text-[10px] text-primary font-medium hover:text-foreground cursor-pointer uppercase tracking-wide">Retry</button>
                {/if}
                <button onclick={dismissExtraction} class="text-[10px] text-muted hover:text-foreground cursor-pointer">Dismiss</button>
              </div>
            </div>
          </div>
        {/if}

        <div class="flex-1 min-h-0 {view === 'generate' || view === 'chat' ? 'w-full' : 'max-w-3xl mx-auto w-full'}">
          {#if view === "generate"}
            <GenerateView />
          {:else if view === "repurpose"}
            <RepurposeView />
          {:else if view === "chat"}
            <ChatView />
          {:else if view === "profiles"}
            <ProfilesView />
          {:else if view === "extract"}
            <ExtractView />
          {:else if view === "account"}
            <AccountView />
          {:else if view === "help"}
            <HelpView />
          {:else}
            <SettingsView />
          {/if}
        </div>
      </div>
    </div>
  {/if}
  <SpotlightTour />
  <ToastContainer />
</div>
{/if}
