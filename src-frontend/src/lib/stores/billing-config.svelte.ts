import { getBillingPublicConfig, type BillingPublicConfig } from "$lib/api/tauri";

const DEFAULT_CONFIG: BillingPublicConfig = {
  pro_monthly_amount_label: "$12",
  pro_monthly_interval_label: "/mo",
  pro_monthly_full_label: "$12/mo",
  pro_pricing_note: "",
  pro_founding_monthly_amount_label: "$7",
  pro_founding_monthly_full_label: "$7/mo",
  pro_founding_pricing_note: "founding member pricing",
  extraction_amount_label: "$49.99",
  extraction_cta_label: "$49.99 one-time",
  extraction_founding_amount_label: "$19",
  extraction_founding_cta_label: "$19 one-time",
  default_trial_days: 7,
};

const CACHE_KEY = "noren:billing_public_config:v4";
const CACHE_TTL_MS = 24 * 60 * 60 * 1000;

type CachedBillingConfig = {
  savedAt: number;
  config: Partial<BillingPublicConfig>;
};

let config = $state<BillingPublicConfig>(DEFAULT_CONFIG);
let loadPromise: Promise<void> | null = null;
let cacheLoaded = false;

function mergeConfig(next: Partial<BillingPublicConfig>): BillingPublicConfig {
  return { ...DEFAULT_CONFIG, ...next };
}

function readCachedConfig(): CachedBillingConfig | null {
  if (typeof localStorage === "undefined") return null;
  try {
    const raw = localStorage.getItem(CACHE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as CachedBillingConfig;
    if (!parsed || typeof parsed.savedAt !== "number" || !parsed.config) return null;
    return parsed;
  } catch {
    return null;
  }
}

function writeCachedConfig(next: BillingPublicConfig) {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(CACHE_KEY, JSON.stringify({ savedAt: Date.now(), config: next }));
  } catch {}
}

function loadFreshCachedConfig(): boolean {
  cacheLoaded = true;
  const cached = readCachedConfig();
  if (!cached) return false;

  config = mergeConfig(cached.config);
  return Date.now() - cached.savedAt < CACHE_TTL_MS;
}

export function proMonthlyAmountLabel(isFoundingMember = false): string {
  return isFoundingMember
    ? config.pro_founding_monthly_amount_label
    : config.pro_monthly_amount_label;
}

export function proMonthlyIntervalLabel(): string {
  return config.pro_monthly_interval_label;
}

export function proMonthlyFullLabel(isFoundingMember = false): string {
  return isFoundingMember
    ? config.pro_founding_monthly_full_label
    : config.pro_monthly_full_label;
}

export function proPricingNote(isFoundingMember = false): string {
  return isFoundingMember
    ? config.pro_founding_pricing_note
    : config.pro_pricing_note;
}

export function extractionAmountLabel(isFoundingMember = false): string {
  return isFoundingMember
    ? config.extraction_founding_amount_label
    : config.extraction_amount_label;
}

export function extractionCtaLabel(isFoundingMember = false): string {
  return isFoundingMember
    ? config.extraction_founding_cta_label
    : config.extraction_cta_label;
}

export async function refresh(): Promise<void> {
  if (loadPromise) return loadPromise;
  if (!cacheLoaded && loadFreshCachedConfig()) return;

  loadPromise = getBillingPublicConfig()
    .then((next) => {
      config = mergeConfig(next);
      writeCachedConfig(config);
    })
    .catch((e) => {
      loadPromise = null;
      console.error("getBillingPublicConfig failed:", e);
    })
    .finally(() => {
      loadPromise = null;
    });

  return loadPromise;
}
