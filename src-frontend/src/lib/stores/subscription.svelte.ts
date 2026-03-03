import { getSubscriptionStatus, type SubscriptionStatus } from "$lib/api/tauri";

let status = $state<SubscriptionStatus | null>(null);

export function getStatus(): SubscriptionStatus | null {
  return status;
}

// Svelte 5 requires exporting derived state via getter functions from .svelte.ts modules
export function isPro(): boolean {
  return status?.tier === "pro" || status?.tier === "teams";
}

export function isFree(): boolean {
  return !status || status.tier === "free";
}

export function canExtract(): boolean {
  return status?.can_extract ?? false;
}

export function canLivingProfile(): boolean {
  return status?.can_living_profile ?? false;
}

export function canSync(): boolean {
  return status?.can_sync ?? false;
}

export function canExport(): boolean {
  return status?.can_export ?? false;
}

export async function refresh(): Promise<void> {
  try {
    status = await getSubscriptionStatus();
  } catch {
    // Keep previous status for offline fallback — don't clear
  }
}
