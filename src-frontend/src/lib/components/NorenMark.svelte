<script lang="ts">
  let { width = 16, height = 19 }: { width?: number; height?: number } = $props();
</script>

<svg {width} {height} viewBox="0 0 80 96" fill="none">
  <path d="M8 8C8 8 8 68 8 74C8 80 14 88 22 88C30 88 33 80 33 74L33 8" stroke="currentColor" stroke-width="5" stroke-linecap="round" fill="none"/>
  <path d="M47 8C47 8 47 68 47 74C47 80 50 88 58 88C66 88 72 80 72 74L72 8" stroke="currentColor" stroke-width="5" stroke-linecap="round" fill="none"/>
  <path d="M33 64C37 74 43 74 47 64" stroke="#C44A2F" stroke-width="3.5" stroke-linecap="round" fill="none"/>
  <line x1="8" y1="8" x2="33" y2="8" stroke="currentColor" stroke-width="5" stroke-linecap="round"/>
  <line x1="47" y1="8" x2="72" y2="8" stroke="currentColor" stroke-width="5" stroke-linecap="round"/>
</svg>
