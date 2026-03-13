<script lang="ts">
  import { getToasts, dismiss } from "$lib/stores/toast.svelte";
</script>

{#if getToasts().length > 0}
  <div class="fixed bottom-4 right-4 z-50 flex flex-col gap-2 max-w-xs pointer-events-none">
    {#each getToasts() as toast (toast.id)}
      <div
        class="pointer-events-auto rounded-lg border px-3 py-2.5 text-xs animate-toast-in flex items-start gap-2
          {toast.type === 'error' ? 'bg-error/5 border-error/20 text-error' :
           toast.type === 'warning' ? 'bg-warning/5 border-warning/20 text-warning' :
           toast.type === 'success' ? 'bg-signal/5 border-signal/20 text-signal' :
           'bg-tint border-border text-muted'}"
        style="box-shadow: var(--shadow-dropdown)"
      >
        <svg class="w-3.5 h-3.5 shrink-0 mt-px" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
          {#if toast.type === 'error'}
            <circle cx="12" cy="12" r="10"/><path stroke-linecap="round" d="M15 9l-6 6M9 9l6 6"/>
          {:else if toast.type === 'warning'}
            <path stroke-linecap="round" stroke-linejoin="round" d="M12 9v4m0 4h.01M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z"/>
          {:else if toast.type === 'success'}
            <path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7"/>
          {:else}
            <circle cx="12" cy="12" r="10"/><path stroke-linecap="round" d="M12 16v-4m0-4h.01"/>
          {/if}
        </svg>
        <span class="flex-1 leading-relaxed">{toast.message}</span>
        <button
          onclick={() => dismiss(toast.id)}
          class="shrink-0 opacity-40 hover:opacity-100 transition-opacity cursor-pointer"
          aria-label="Dismiss"
        >
          <svg class="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
            <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12"/>
          </svg>
        </button>
      </div>
    {/each}
  </div>
{/if}
