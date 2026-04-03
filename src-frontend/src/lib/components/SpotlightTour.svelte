<script lang="ts">
  import { isActive, getCurrentStep, getStepIndex, getSteps, nextStep, skipTour } from "$lib/stores/tour.svelte";

  let targetRect = $state<DOMRect | null>(null);
  let tooltipEl: HTMLDivElement | undefined = $state();
  let nextBtn: HTMLButtonElement | undefined = $state();
  let tooltipPos = $state({ top: 0, left: 0 });
  let ready = $state(false);

  const PAD = 8;
  const GAP = 12;

  $effect(() => {
    const step = getCurrentStep();
    if (!step) {
      targetRect = null;
      ready = false;
      return;
    }

    if (!step.target) {
      targetRect = null;
      // Centered card mode — position after paint
      requestAnimationFrame(() => {
        ready = true;
        positionCentered();
        nextBtn?.focus();
      });
      return;
    }

    const el = document.querySelector(`[data-tour="${step.target}"]`);
    if (!el) {
      // Element not found, skip to next
      nextStep();
      return;
    }

    const rect = el.getBoundingClientRect();
    targetRect = rect;
    requestAnimationFrame(() => {
      ready = true;
      positionTooltip(rect, step.placement);
      nextBtn?.focus();
    });
  });

  // Reposition on resize
  $effect(() => {
    if (!isActive()) return;

    function handleResize() {
      const step = getCurrentStep();
      if (!step) return;
      if (!step.target) {
        positionCentered();
        return;
      }
      const el = document.querySelector(`[data-tour="${step.target}"]`);
      if (!el) return;
      const rect = el.getBoundingClientRect();
      targetRect = rect;
      positionTooltip(rect, step.placement);
    }

    window.addEventListener("resize", handleResize);
    return () => window.removeEventListener("resize", handleResize);
  });

  function positionCentered() {
    if (!tooltipEl) return;
    const tt = tooltipEl.getBoundingClientRect();
    tooltipPos = {
      top: Math.max(12, (window.innerHeight - tt.height) / 2),
      left: Math.max(12, (window.innerWidth - tt.width) / 2),
    };
  }

  function positionTooltip(rect: DOMRect, placement: string) {
    if (!tooltipEl) return;
    const tt = tooltipEl.getBoundingClientRect();
    let top = 0;
    let left = 0;

    if (placement === "bottom") {
      top = rect.bottom + PAD + GAP;
      left = rect.left + rect.width / 2 - tt.width / 2;
    } else if (placement === "top") {
      top = rect.top - PAD - GAP - tt.height;
      left = rect.left + rect.width / 2 - tt.width / 2;
    } else if (placement === "right") {
      top = rect.top + rect.height / 2 - tt.height / 2;
      left = rect.right + PAD + GAP;
    } else if (placement === "left") {
      top = rect.top + rect.height / 2 - tt.height / 2;
      left = rect.left - PAD - GAP - tt.width;
    }

    // Clamp to viewport
    left = Math.max(12, Math.min(left, window.innerWidth - tt.width - 12));
    top = Math.max(12, Math.min(top, window.innerHeight - tt.height - 12));

    tooltipPos = { top, left };
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") skipTour();
  }
</script>

{#if isActive() && ready}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="tour-overlay"
    onclick={skipTour}
    onkeydown={handleKeydown}
  >
    {#if targetRect}
      <!-- Spotlight cutout -->
      <div
        class="tour-cutout"
        style="
          top: {targetRect.top - PAD}px;
          left: {targetRect.left - PAD}px;
          width: {targetRect.width + PAD * 2}px;
          height: {targetRect.height + PAD * 2}px;
        "
      ></div>
    {/if}
  </div>

  <!-- Tooltip -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    bind:this={tooltipEl}
    class="tour-tooltip {!targetRect ? 'tour-tooltip-card' : ''}"
    style="top: {tooltipPos.top}px; left: {tooltipPos.left}px;"
    onclick={(e) => e.stopPropagation()}
    onkeydown={handleKeydown}
  >
    {#if !targetRect}
      <!-- Centered card gets a subtle icon -->
      <div class="tour-card-icon">
        <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
          <path d="M18 3a3 3 0 0 0-3 3v12a3 3 0 0 0 3 3 3 3 0 0 0 3-3 3 3 0 0 0-3-3H6a3 3 0 0 0-3 3 3 3 0 0 0 3 3 3 3 0 0 0 3-3V6a3 3 0 0 0-3-3 3 3 0 0 0-3 3 3 3 0 0 0 3 3h12a3 3 0 0 0 3-3 3 3 0 0 0-3-3z"/>
        </svg>
      </div>
    {/if}
    <p class="tour-description">{getCurrentStep()?.description}</p>
    <div class="tour-footer">
      <span class="tour-counter">{getStepIndex() + 1} of {getSteps().length}</span>
      <div class="tour-actions">
        <button onclick={skipTour} class="tour-skip">Skip</button>
        <button bind:this={nextBtn} onclick={nextStep} class="tour-next">
          {getStepIndex() === getSteps().length - 1 ? "Done" : "Next"}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .tour-overlay {
    position: fixed;
    inset: 0;
    z-index: 9990;
    pointer-events: auto;
  }

  .tour-cutout {
    position: absolute;
    border-radius: 10px;
    box-shadow: 0 0 0 9999px rgba(0, 0, 0, 0.5);
    pointer-events: none;
    z-index: 9991;
    transition: top 0.3s ease, left 0.3s ease, width 0.3s ease, height 0.3s ease;
  }

  .tour-tooltip {
    position: fixed;
    z-index: 9995;
    padding: 16px 18px;
    border-radius: 14px;
    border: 1px solid var(--color-border);
    background: var(--color-surface);
    box-shadow: 0 4px 24px rgba(30, 49, 72, 0.14), 0 1px 4px rgba(0, 0, 0, 0.06);
    max-width: 280px;
    pointer-events: auto;
    animation: tour-enter 0.25s ease-out;
  }

  .tour-tooltip-card {
    max-width: 320px;
    padding: 20px 22px;
  }

  .tour-card-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 36px;
    height: 36px;
    border-radius: 10px;
    background: color-mix(in srgb, var(--color-accent) 10%, transparent);
    color: var(--color-accent);
    margin-bottom: 10px;
  }

  .tour-description {
    font-size: 13px;
    line-height: 1.55;
    color: var(--color-foreground);
    margin: 0;
  }

  .tour-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-top: 14px;
  }

  .tour-counter {
    font-family: var(--font-heading), serif;
    font-style: italic;
    font-size: 11px;
    color: var(--color-muted);
    opacity: 0.7;
  }

  .tour-actions {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .tour-skip {
    all: unset;
    font-size: 11px;
    color: var(--color-muted);
    cursor: pointer;
    transition: color 0.15s;
  }
  .tour-skip:hover {
    color: var(--color-foreground);
  }

  .tour-next {
    all: unset;
    padding: 5px 14px;
    font-size: 11px;
    font-weight: 600;
    color: #fff;
    background: var(--color-accent);
    border-radius: 8px;
    cursor: pointer;
    transition: opacity 0.15s;
    box-shadow: 0 0 10px var(--color-accent-glow), 0 1px 3px rgba(0, 0, 0, 0.1);
  }
  .tour-next:hover {
    opacity: 0.9;
  }
  .tour-next:focus-visible {
    outline: 2px solid var(--color-accent);
    outline-offset: 2px;
  }

  @keyframes tour-enter {
    from {
      opacity: 0;
      transform: translateY(6px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
</style>
