import { getProfilePatches } from "$lib/api/tauri";
import { canLivingProfile } from "$lib/stores/subscription.svelte";

let count = $state(0);

export function getPatchCount(): number {
  return count;
}

export async function refreshPatches(): Promise<void> {
  if (!canLivingProfile()) {
    count = 0;
    return;
  }
  try {
    const patches = await getProfilePatches();
    count = patches.length;
  } catch {
    count = 0;
  }
}

export function setPatchCount(n: number): void {
  count = n;
}
