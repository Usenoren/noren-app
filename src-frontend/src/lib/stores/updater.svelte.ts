/**
 * Auto-updater state machine for the desktop app.
 *
 * idle      -> no update, or check hasn't run yet
 * available -> server returned a new version, banner offers Install/Later
 * downloading -> user clicked Install, .app.tar.gz is downloading
 * ready     -> install + relaunch pending; banner shows Restart
 *
 * Transitions:
 *   idle -> available  (check() returns update.available)
 *   available -> downloading (user clicks Install)
 *   downloading -> ready (download + install finished, awaiting restart)
 *   any -> idle (user clicks Later -> dismissedThisSession = true)
 *
 * Later semantics: hides the banner for the rest of the session only.
 * On next launch, check() runs again and the banner reappears if applicable.
 */

import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

export type UpdateStatus = "idle" | "available" | "downloading" | "ready" | "error";

interface UpdaterState {
  status: UpdateStatus;
  version: string | null;
  progress: number; // 0..100
  error: string | null;
}

const state = $state<UpdaterState>({
  status: "idle",
  version: null,
  progress: 0,
  error: null,
});

let dismissedThisSession = false;
let pendingUpdate: Update | null = null;

export function getStatus(): UpdateStatus { return state.status; }
export function getVersion(): string | null { return state.version; }
export function getProgress(): number { return state.progress; }
export function getError(): string | null { return state.error; }

export async function checkForUpdate(): Promise<void> {
  if (dismissedThisSession) return;
  try {
    const update = await check();
    if (update?.available) {
      pendingUpdate = update;
      state.version = update.version;
      state.status = "available";
    }
  } catch (e) {
    // Network failures, server 5xx, signature mismatches all land here.
    // Stay silent on first check, log to console for debugging.
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
            state.progress = Math.min(99, Math.round((downloaded / total) * 100));
          }
          break;
        case "Finished":
          state.progress = 100;
          state.status = "ready";
          break;
      }
    });
    // downloadAndInstall completes after Finished. relaunch() ends the process.
    await relaunch();
  } catch (e) {
    state.status = "error";
    state.error = String(e);
  }
}

export function dismiss(): void {
  dismissedThisSession = true;
  state.status = "idle";
  pendingUpdate = null;
}
