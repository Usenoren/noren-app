import { check, type Update } from "@tauri-apps/plugin-updater";
import { toastError } from "$lib/stores/toast.svelte";

export type UpdateStatus = "idle" | "available" | "downloading" | "ready";

interface UpdaterState {
  status: UpdateStatus;
  version: string | null;
  progress: number; // 0..100
}

const state = $state<UpdaterState>({
  status: "idle",
  version: null,
  progress: 0,
});

// Hide the banner for the rest of this session only. Next launch checks again.
let dismissedThisSession = false;
let pendingUpdate: Update | null = null;

export function getStatus(): UpdateStatus { return state.status; }
export function getVersion(): string | null { return state.version; }
export function getProgress(): number { return state.progress; }

export async function checkForUpdate(): Promise<void> {
  if (import.meta.env.DEV) return;
  if (dismissedThisSession) return;
  if (state.status !== "idle") return;
  try {
    const update = await check();
    if (update?.available) {
      pendingUpdate = update;
      state.version = update.version;
      state.status = "available";
    }
  } catch (e) {
    // Network failures, server 5xx, signature mismatches all land here.
    // Stay silent — a missed prompt is better than a scary one for a transient blip.
    console.warn("[updater] check failed:", e);
  }
}

export async function installAndRestart(): Promise<void> {
  if (!pendingUpdate) return;
  state.status = "downloading";
  state.progress = 0;
  let total = 0;
  let downloaded = 0;
  try {
    await pendingUpdate.downloadAndInstall((event) => {
      switch (event.event) {
        case "Started":
          total = event.data.contentLength ?? 0;
          break;
        case "Progress":
          downloaded += event.data.chunkLength;
          if (total > 0) {
            const next = Math.min(99, Math.round((downloaded / total) * 100));
            if (next !== state.progress) state.progress = next;
          }
          break;
        case "Finished":
          state.progress = 100;
          state.status = "ready";
          break;
      }
    });
    // Wait for the user to click Restart. The banner's ready state owns relaunch().
  } catch (e) {
    pendingUpdate = null;
    state.status = "idle";
    toastError("Update couldn't be installed. Please try again.");
    console.warn("[updater] install failed:", e);
  }
}

export function dismiss(): void {
  dismissedThisSession = true;
  state.status = "idle";
  pendingUpdate = null;
}
