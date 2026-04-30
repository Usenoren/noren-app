<script lang="ts">
  import {
    getStatus,
    getVersion,
    getProgress,
    installAndRestart,
    dismiss,
  } from "$lib/stores/updater.svelte";
  import { relaunch } from "@tauri-apps/plugin-process";

  function onInstall() { installAndRestart(); }
  function onLater() { dismiss(); }
  function onRestart() { relaunch(); }
</script>

{#if getStatus() !== "idle"}
  <div class="update-banner" role="status" aria-live="polite">

    {#if getStatus() === "available"}
      <span class="dot glow" aria-hidden="true"></span>
      <span class="banner-text">A new version of Noren is ready</span>
      <span class="v-pill">v{getVersion()}</span>
      <span class="banner-spacer"></span>
      <button class="banner-cta" onclick={onInstall}>
        <span>Install and restart</span>
      </button>
      <button class="banner-later" onclick={onLater}>Later</button>

    {:else if getStatus() === "downloading"}
      <span class="dot pulse" aria-hidden="true"></span>
      <span class="banner-text">Downloading v{getVersion()}…</span>
      <span class="banner-spacer"></span>
      <div class="progress">
        <div class="strand"></div>
        <span class="percent">{getProgress()}%</span>
      </div>

    {:else if getStatus() === "ready"}
      <span class="dot" aria-hidden="true"></span>
      <span class="banner-text">v{getVersion()} ready. Restart Noren to finish.</span>
      <span class="banner-spacer"></span>
      <button class="banner-cta" onclick={onRestart}>
        <span>Restart now</span>
      </button>

    {:else if getStatus() === "error"}
      <span class="dot" aria-hidden="true" style="background: rgba(200,212,221,0.4); box-shadow: none;"></span>
      <span class="banner-text">Update couldn't be installed. Please try again.</span>
      <span class="banner-spacer"></span>
      <button class="banner-later" onclick={onLater}>Dismiss</button>
    {/if}

  </div>
{/if}

<style>
  .update-banner {
    position: relative;
    flex: 0 0 44px;
    height: 44px;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 0 16px 0 18px;
    color: var(--color-primary);
    background:
      linear-gradient(90deg, rgba(196,74,47,0.06), rgba(196,74,47,0.04) 60%, rgba(196,74,47,0.05)),
      var(--color-background);
    overflow: hidden;
    flex-shrink: 0;
  }
  .update-banner::before {
    content: "";
    position: absolute; inset: 0;
    background-image:
      repeating-linear-gradient(0deg, transparent, transparent 27px, rgba(200,212,221,0.04) 27px, rgba(200,212,221,0.04) 28px),
      repeating-linear-gradient(90deg, transparent, transparent 27px, rgba(200,212,221,0.04) 27px, rgba(200,212,221,0.04) 28px);
    pointer-events: none;
    opacity: 0.9;
  }
  .update-banner::after {
    content: "";
    position: absolute; left: 0; right: 0; bottom: 0; height: 1px;
    background: linear-gradient(90deg,
      transparent,
      rgba(196,74,47,0.25) 30%,
      rgba(200,212,221,0.18) 70%,
      transparent);
    pointer-events: none;
  }
  .update-banner > * { position: relative; z-index: 1; }

  .dot {
    width: 8px; height: 8px; border-radius: 50%;
    background: var(--color-accent);
    box-shadow: 0 0 0 4px rgba(196,74,47,0.18);
    flex-shrink: 0;
  }
  .dot.glow {
    box-shadow:
      0 0 0 4px rgba(196,74,47,0.18),
      0 0 14px rgba(196,74,47,0.55);
    animation: dot-glow 2.4s ease-in-out infinite;
  }
  .dot.pulse {
    animation: dot-pulse 1.4s ease-in-out infinite;
  }
  @keyframes dot-glow {
    0%, 100% { box-shadow: 0 0 0 4px rgba(196,74,47,0.18), 0 0 10px rgba(196,74,47,0.5); }
    50%      { box-shadow: 0 0 0 5px rgba(196,74,47,0.22), 0 0 20px rgba(196,74,47,0.75); }
  }
  @keyframes dot-pulse {
    0%, 100% { transform: scale(1);    box-shadow: 0 0 0 4px rgba(196,74,47,0.16); }
    50%      { transform: scale(1.18); box-shadow: 0 0 0 6px rgba(196,74,47,0.30); }
  }

  .banner-text {
    font-family: var(--font-heading, "Cormorant Garamond", Georgia, serif);
    font-style: italic;
    font-weight: 500;
    font-size: 14px;
    line-height: 1.2;
    letter-spacing: 0.2px;
    flex-shrink: 0;
  }

  .v-pill {
    font-size: 10.5px;
    font-weight: 500;
    letter-spacing: 0.4px;
    color: #f4d4cc;
    background: rgba(196,74,47,0.18);
    border: 1px solid rgba(196,74,47,0.42);
    border-radius: 999px;
    padding: 2px 9px 3px;
    flex-shrink: 0;
    margin-left: 2px;
  }

  .banner-spacer { flex: 1 1 auto; }

  .banner-cta {
    border: none;
    cursor: pointer;
    font: inherit;
    background: var(--color-accent);
    color: #fff;
    font-size: 11.5px;
    font-weight: 600;
    letter-spacing: 0.3px;
    padding: 7px 14px;
    border-radius: 8px;
    box-shadow: 0 1px 4px var(--color-accent-glow), 0 6px 14px var(--color-accent-glow);
    position: relative;
    overflow: hidden;
    transition: background 0.15s, transform 0.15s, box-shadow 0.15s;
    flex-shrink: 0;
  }
  .banner-cta::before {
    content: "";
    position: absolute; inset: 0;
    background-image: repeating-linear-gradient(90deg, transparent, transparent 11px, var(--color-accent-wash) 11px, var(--color-accent-wash) 12px);
    pointer-events: none;
    opacity: 0.85;
  }
  .banner-cta span { position: relative; z-index: 1; }
  .banner-cta:hover {
    background: var(--color-accent-hover);
    transform: translateY(-1px);
    box-shadow: 0 2px 6px var(--color-accent-glow), 0 8px 18px var(--color-accent-glow);
  }

  .banner-later {
    background: none;
    border: none;
    cursor: pointer;
    font: inherit;
    font-size: 10.5px;
    letter-spacing: 1px;
    text-transform: uppercase;
    color: rgba(200,212,221,0.5);
    padding: 6px 4px;
    transition: color 0.15s;
    flex-shrink: 0;
  }
  .banner-later:hover { color: var(--color-primary); }

  .progress {
    display: flex; align-items: center; gap: 10px;
    flex-shrink: 0;
  }
  .strand {
    width: 120px; height: 1px;
    background: rgba(200,212,221,0.16);
    position: relative; overflow: hidden;
    border-radius: 999px;
  }
  .strand::after {
    content: ""; position: absolute; inset: 0;
    background: linear-gradient(90deg, transparent, var(--color-accent), transparent);
    transform: translateX(-100%);
    animation: strand-sweep 1.6s ease-in-out infinite;
  }
  @keyframes strand-sweep {
    0%   { transform: translateX(-100%); }
    100% { transform: translateX(100%); }
  }
  .percent {
    font-family: var(--font-heading, "Cormorant Garamond", Georgia, serif);
    font-style: italic;
    font-weight: 500;
    font-size: 13px;
    color: var(--color-secondary);
    letter-spacing: 0.3px;
  }
</style>
