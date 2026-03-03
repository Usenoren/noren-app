<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import { checkPermissions, requestPermissions, getSettings, getProfileOverview, migrateProfileToServer } from "$lib/api/tauri";
  import { refresh as refreshSubscription, canExtract } from "$lib/stores/subscription.svelte";
  import GenerateView from "./GenerateView.svelte";
  import ChatView from "./ChatView.svelte";
  import SettingsView from "./SettingsView.svelte";
  import ProfilesView from "./ProfilesView.svelte";
  import ExtractView from "./ExtractView.svelte";
  import OnboardingView from "./OnboardingView.svelte";

  type View = "generate" | "chat" | "profiles" | "extract" | "settings" | "onboarding";
  let view: View = $state("generate");
  let hasPermissions = $state(true);
  let needsOnboarding = $state(false);
  let loading = $state(true);

  const navItems: { id: View; label: string; icon: string }[] = [
    { id: "generate", label: "Generate", icon: "pen" },
    { id: "chat", label: "Chat", icon: "chat" },
    { id: "profiles", label: "Profiles", icon: "user" },
    { id: "extract", label: "Extract", icon: "wand" },
    { id: "settings", label: "Settings", icon: "gear" },
  ];

  onMount(() => {
    document.documentElement.style.fontSize = "15px";
  });

  $effect(() => {
    const cleanups: (() => void)[] = [];

    listen<string>("navigate", (event) => {
      const target = event.payload as View;
      if (["generate", "chat", "profiles", "extract", "settings"].includes(target)) {
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

        if (settings.inference_mode === "noren_pro" && profile.exists && !profile.is_server) {
          migrateProfileToServer().catch(() => {});
        }
      }
    }).catch(() => {
      loading = false;
    });

    return () => cleanups.forEach((fn) => fn());
  });

  function handleOnboardingComplete() {
    needsOnboarding = false;
    view = "generate";
  }

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
      <!-- Sidebar -->
      <nav class="w-[180px] shrink-0 bg-surface border-r border-border flex flex-col py-3 px-2">
        <div class="flex flex-col gap-0.5">
          {#each navItems as item}
            <button
              onclick={() => { view = item.id; }}
              class="flex items-center gap-2.5 px-3 py-2 text-sm rounded-md transition-colors cursor-pointer relative
                {view === item.id
                  ? 'bg-primary/10 text-primary font-medium'
                  : 'text-muted hover:bg-tint hover:text-foreground'}"
            >
              {#if item.icon === "pen"}
                <svg class="w-4 h-4 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M16.862 4.487l1.687-1.688a1.875 1.875 0 112.652 2.652L10.582 16.07a4.5 4.5 0 01-1.897 1.13L6 18l.8-2.685a4.5 4.5 0 011.13-1.897l8.932-8.931zm0 0L19.5 7.125M18 14v4.75A2.25 2.25 0 0115.75 21H5.25A2.25 2.25 0 013 18.75V8.25A2.25 2.25 0 015.25 6H10" />
                </svg>
              {:else if item.icon === "chat"}
                <svg class="w-4 h-4 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M8.625 12a.375.375 0 11-.75 0 .375.375 0 01.75 0zm0 0H8.25m4.125 0a.375.375 0 11-.75 0 .375.375 0 01.75 0zm0 0H12m4.125 0a.375.375 0 11-.75 0 .375.375 0 01.75 0zm0 0h-.375M21 12c0 4.556-4.03 8.25-9 8.25a9.764 9.764 0 01-2.555-.337A5.972 5.972 0 015.41 20.97a5.969 5.969 0 01-.474-.065 4.48 4.48 0 00.978-2.025c.09-.457-.133-.901-.467-1.226C3.93 16.178 3 14.189 3 12c0-4.556 4.03-8.25 9-8.25s9 3.694 9 8.25z" />
                </svg>
              {:else if item.icon === "user"}
                <svg class="w-4 h-4 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M15.75 6a3.75 3.75 0 11-7.5 0 3.75 3.75 0 017.5 0zM4.501 20.118a7.5 7.5 0 0114.998 0A17.933 17.933 0 0112 21.75c-2.676 0-5.216-.584-7.499-1.632z" />
                </svg>
              {:else if item.icon === "wand"}
                <svg class="w-4 h-4 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M9.813 15.904L9 18.75l-.813-2.846a4.5 4.5 0 00-3.09-3.09L2.25 12l2.846-.813a4.5 4.5 0 003.09-3.09L9 5.25l.813 2.846a4.5 4.5 0 003.09 3.09L15.75 12l-2.846.813a4.5 4.5 0 00-3.09 3.09zM18.259 8.715L18 9.75l-.259-1.035a3.375 3.375 0 00-2.455-2.456L14.25 6l1.036-.259a3.375 3.375 0 002.455-2.456L18 2.25l.259 1.035a3.375 3.375 0 002.455 2.456L21.75 6l-1.036.259a3.375 3.375 0 00-2.455 2.456zM16.894 20.567L16.5 21.75l-.394-1.183a2.25 2.25 0 00-1.423-1.423L13.5 18.75l1.183-.394a2.25 2.25 0 001.423-1.423l.394-1.183.394 1.183a2.25 2.25 0 001.423 1.423l1.183.394-1.183.394a2.25 2.25 0 00-1.423 1.423z" />
                </svg>
              {:else if item.icon === "gear"}
                <svg class="w-4 h-4 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M9.594 3.94c.09-.542.56-.94 1.11-.94h2.593c.55 0 1.02.398 1.11.94l.213 1.281c.063.374.313.686.645.87.074.04.147.083.22.127.324.196.72.257 1.075.124l1.217-.456a1.125 1.125 0 011.37.49l1.296 2.247a1.125 1.125 0 01-.26 1.431l-1.003.827c-.293.24-.438.613-.431.992a6.759 6.759 0 010 .255c-.007.378.138.75.43.99l1.005.828c.424.35.534.954.26 1.43l-1.298 2.247a1.125 1.125 0 01-1.369.491l-1.217-.456c-.355-.133-.75-.072-1.076.124a6.57 6.57 0 01-.22.128c-.331.183-.581.495-.644.869l-.213 1.28c-.09.543-.56.941-1.11.941h-2.594c-.55 0-1.02-.398-1.11-.94l-.213-1.281c-.062-.374-.312-.686-.644-.87a6.52 6.52 0 01-.22-.127c-.325-.196-.72-.257-1.076-.124l-1.217.456a1.125 1.125 0 01-1.369-.49l-1.297-2.247a1.125 1.125 0 01.26-1.431l1.004-.827c.292-.24.437-.613.43-.992a6.932 6.932 0 010-.255c.007-.378-.138-.75-.43-.99l-1.004-.828a1.125 1.125 0 01-.26-1.43l1.297-2.247a1.125 1.125 0 011.37-.491l1.216.456c.356.133.751.072 1.076-.124.072-.044.146-.087.22-.128.332-.183.582-.495.644-.869l.214-1.281z" />
                  <path stroke-linecap="round" stroke-linejoin="round" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                </svg>
              {/if}
              {item.label}
              {#if item.id === "extract" && !canExtract()}
                <span class="absolute top-1 right-1.5 text-[8px] font-medium text-secondary">$</span>
              {/if}
            </button>
          {/each}
        </div>

        <!-- Bottom branding -->
        <div class="mt-auto pt-3 px-3">
          <span class="text-xs text-muted/50 font-heading tracking-wide">noren</span>
        </div>
      </nav>

      <!-- Content area -->
      <div class="flex-1 min-h-0 overflow-hidden">
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

        <div class="h-full max-w-3xl mx-auto">
          {#if view === "generate"}
            <GenerateView />
          {:else if view === "chat"}
            <ChatView />
          {:else if view === "profiles"}
            <ProfilesView />
          {:else if view === "extract"}
            <ExtractView />
          {:else}
            <SettingsView />
          {/if}
        </div>
      </div>
    </div>
  {/if}
</div>
{/if}
