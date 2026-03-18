import { listen } from "@tauri-apps/api/event";
import {
  startExtraction,
  startExtractionMulti,
  type ExtractionProgress,
  type FormatGroup,
} from "$lib/api/tauri";

function friendlyExtractionError(raw: string): string {
  const lower = raw.toLowerCase();
  if (lower.includes("verified") || lower.includes("verification") || lower.includes("403"))
    return "Please verify your email first. Check Account settings.";
  if (lower.includes("rate limit") || lower.includes("429"))
    return "Extraction was rate-limited. Please try again in a minute.";
  if (lower.includes("authentication") || lower.includes("invalid") || lower.includes("401"))
    return "Server error. Please try again shortly.";
  if (lower.includes("timeout"))
    return "Extraction timed out. Please try again.";
  if (lower.includes("connection") || lower.includes("network"))
    return "Connection error. Check your internet and try again.";
  return "Extraction failed. Please try again.";
}

let isExtracting = $state(false);
let currentFormat = $state("");
let currentIndex = $state(0);
let totalFormats = $state(0);
let progress = $state<ExtractionProgress | null>(null);
let error = $state("");
let done = $state(false);

let lastFormats: { samples: string; format: string; calibration?: object }[] = [];
let initialized = false;
let resolveCurrentJob: (() => void) | null = null;

export function init() {
  if (initialized) return;
  initialized = true;

  listen<ExtractionProgress>("extraction-progress", (event) => {
    progress = event.payload;
    if (progress.status === "saved" || progress.status === "stored_server") {
      if (resolveCurrentJob) {
        resolveCurrentJob();
        resolveCurrentJob = null;
      }
    } else if (progress.status === "failed") {
      error = friendlyExtractionError(progress.error || "Extraction failed");
      if (resolveCurrentJob) {
        resolveCurrentJob();
        resolveCurrentJob = null;
      }
    }
  });
}

export async function startQueue(formats: { samples: string; format: string; calibration?: object }[]) {
  if (isExtracting || formats.length === 0) return;

  lastFormats = formats;
  isExtracting = true;
  error = "";
  done = false;
  totalFormats = formats.length;
  currentIndex = 1;
  currentFormat = formats.map((f) => f.format).join(", ");

  try {
    if (formats.length === 1) {
      // Single format — use the original endpoint
      await startExtraction(formats[0]);
    } else {
      // Multi-format — single job with shared core identity
      const formatGroups: FormatGroup[] = formats.map((f) => ({
        format: f.format,
        samples: f.samples,
      }));
      await startExtractionMulti({ formatGroups });
    }

    // Wait for completion via event listener (5-min timeout)
    await new Promise<void>((resolve) => {
      const timer = setTimeout(() => {
        if (resolveCurrentJob) {
          error = "Extraction timed out. Check your connection and try again.";
          resolveCurrentJob = null;
          resolve();
        }
      }, 5 * 60 * 1000);
      resolveCurrentJob = () => {
        clearTimeout(timer);
        resolve();
      };
    });
  } catch (e) {
    error = friendlyExtractionError(String(e));
  }

  isExtracting = false;
  if (!error) {
    done = true;
    // Auto-dismiss after 5 seconds
    setTimeout(() => {
      done = false;
    }, 5000);
  }
}

// Getters (Svelte 5 reactive exports)
export function getIsExtracting(): boolean { return isExtracting; }
export function getCurrentFormat(): string { return currentFormat; }
export function getCurrentIndex(): number { return currentIndex; }
export function getTotalFormats(): number { return totalFormats; }
export function getProgress(): ExtractionProgress | null { return progress; }
export function getError(): string { return error; }
export function isDone(): boolean { return done; }

export function canRetry(): boolean { return !!error && lastFormats.length > 0 && !isExtracting; }

export function retry() {
  if (!canRetry()) return;
  error = "";
  startQueue(lastFormats);
}

export function dismiss() {
  error = "";
  done = false;
}
