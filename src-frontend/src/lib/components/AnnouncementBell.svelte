<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { marked } from "marked";
  import DOMPurify from "dompurify";
  import { fetchAnnouncements, getAnnouncementSeen, saveAnnouncementSeen } from "$lib/api/tauri";
  import type { Announcement } from "$lib/api/tauri";

  let announcements: Announcement[] = $state([]);
  let lastSeen: string | null = $state(null);
  let open = $state(false);

  const unreadCount = $derived(
    lastSeen
      ? announcements.filter(a => a.published_at > lastSeen!).length
      : announcements.length
  );

  function typeColor(type: string): string {
    switch (type) {
      case "feature": return "bg-primary/20 text-primary";
      case "update": return "bg-secondary/20 text-secondary";
      case "promo": return "bg-accent/20 text-accent";
      case "maintenance": return "bg-warning/20 text-warning";
      default: return "bg-muted/20 text-muted";
    }
  }

  function renderMarkdown(md: string): string {
    return DOMPurify.sanitize(marked.parse(md) as string);
  }

  async function handleOpen() {
    open = !open;
    if (open && announcements.length > 0) {
      const latest = announcements[0].published_at;
      lastSeen = latest;
      await saveAnnouncementSeen(latest).catch((e) => console.error("saveAnnouncementSeen failed:", e));
    }
  }

  function handleClickOutside(e: MouseEvent) {
    const target = e.target as HTMLElement;
    if (open && !target.closest(".announcement-bell")) {
      open = false;
    }
  }

  function refresh() {
    fetchAnnouncements().then((data) => { announcements = data; }).catch((e) => console.error("fetchAnnouncements failed:", e));
  }

  onMount(() => {
    getAnnouncementSeen()
      .then((ts) => { lastSeen = ts; return fetchAnnouncements(); })
      .then((data) => { announcements = data; })
      .catch((e) => console.error("announcements init failed:", e));

    let unlisten: (() => void) | undefined;
    listen("tauri://focus", refresh).then((fn) => { unlisten = fn; });

    document.addEventListener("click", handleClickOutside);
    return () => {
      document.removeEventListener("click", handleClickOutside);
      unlisten?.();
    };
  });
</script>

<div class="announcement-bell relative">
  <button
    onclick={handleOpen}
    class="relative p-1 rounded transition-colors cursor-pointer hover:bg-white/[0.08] {unreadCount > 0 ? 'bell-breathe' : ''}"
    title="Announcements"
  >
    <svg class="w-[14px] h-[14px]" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"
      style="color:rgba(255,255,255,{unreadCount > 0 ? '0.85' : '0.2'})">
      <path d="M18 8A6 6 0 006 8c0 7-3 9-3 9h18s-3-2-3-9M13.73 21a2 2 0 01-3.46 0"/>
    </svg>
    {#if unreadCount > 0}
      <span class="absolute -top-0.5 -right-0.5 w-[10px] h-[10px] rounded-full bg-accent text-[6px] font-bold text-white flex items-center justify-center">
        {unreadCount > 9 ? "9+" : unreadCount}
      </span>
    {/if}
  </button>

  {#if open && announcements.length > 0}
    <div class="absolute bottom-0 left-full ml-2 w-[280px] max-h-[360px] overflow-y-auto
                bg-surface border border-border rounded-xl shadow-xl z-50">
      <div class="p-2.5 border-b border-border">
        <h3 class="text-[10px] font-semibold uppercase tracking-wider text-muted">What's New</h3>
      </div>
      <div class="flex flex-col">
        {#each announcements as a}
          <div class="p-2.5 border-b border-border/50 last:border-0 {lastSeen && a.published_at > lastSeen ? 'bg-accent/[0.03]' : ''}">
            <div class="flex items-center gap-1.5 mb-1">
              <span class="text-[8px] font-semibold uppercase px-1.5 py-0.5 rounded-full {typeColor(a.type)}">
                {a.type}
              </span>
              <span class="text-[9px] text-muted">
                {new Date(a.published_at).toLocaleDateString()}
              </span>
            </div>
            <h4 class="text-xs font-medium text-foreground mb-1">{a.title}</h4>
            <div class="text-[11px] text-muted leading-relaxed prose-compact">
              {@html renderMarkdown(a.body)}
            </div>
            {#if a.cta_url}
              <a
                href={a.cta_url}
                target="_blank"
                rel="noopener"
                class="inline-block mt-1.5 text-[10px] font-medium text-secondary hover:text-foreground"
              >
                {a.cta_label || "Learn more"} &rarr;
              </a>
            {/if}
          </div>
        {/each}
      </div>
    </div>
  {/if}
</div>

<style>
  .bell-breathe {
    animation: bell-scale 2s ease-in-out infinite;
    transform-origin: center center;
  }
  @keyframes bell-scale {
    0%, 100% { transform: scale(1); }
    50% { transform: scale(1.3); }
  }
  .prose-compact :global(p) {
    margin: 0 0 0.25em 0;
  }
  .prose-compact :global(p:last-child) {
    margin-bottom: 0;
  }
  .prose-compact :global(a) {
    color: var(--color-secondary);
    text-decoration: underline;
  }
  .prose-compact :global(code) {
    font-size: 0.85em;
    background: rgba(255,255,255,0.05);
    padding: 0.1em 0.3em;
    border-radius: 3px;
  }
</style>
