import {
  getSubscriptionStatus,
  hasExtractionReceipt,
  type SubscriptionStatus,
} from "$lib/api/tauri";

let status = $state<SubscriptionStatus | null>(null);
let localExtraction = $state(false);

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
  return (status?.can_extract ?? false) || localExtraction;
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

export function exportUnlockRemainingCents(): number | null {
  return status?.export_unlock_remaining_cents ?? null;
}

export function exportUnlockProgress(): number | null {
  return status?.export_unlock_progress ?? null;
}

export function isTrial(): boolean {
  return status?.is_trial ?? false;
}

export function trialExpiresAt(): string | null {
  return status?.trial_expires_at ?? null;
}

/** Days remaining in trial, or null if not on trial. */
export function trialDaysLeft(): number | null {
  const expires = status?.trial_expires_at;
  if (!status?.is_trial || !expires) return null;
  const ms = new Date(expires).getTime() - Date.now();
  return Math.max(0, Math.ceil(ms / 86_400_000));
}

export async function refresh(): Promise<void> {
  // Always check local receipt (works without auth)
  try {
    localExtraction = await hasExtractionReceipt();
  } catch (e) {
    console.error("hasExtractionReceipt failed:", e);
  }

  // Check server status (requires auth, will fail for free BYOK users)
  try {
    status = await getSubscriptionStatus();
  } catch (e) {
    console.error("getSubscriptionStatus failed:", e);
  }
}
