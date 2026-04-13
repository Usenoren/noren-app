import { getSettings } from "./tauri";

export type AnalyticsEventName = "generation_used";

const DEFAULT_SERVER_URL = "https://api.usenoren.ai";
const INSTALL_TOKEN_KEY = "noren:analytics_install_token";

async function getServerUrl(): Promise<string> {
  const settings = await getSettings();
  return (settings.server_url || DEFAULT_SERVER_URL).replace(/\/+$/, "");
}

async function bootstrapInstallToken(serverUrl: string): Promise<string | null> {
  try {
    const resp = await fetch(`${serverUrl}/v1/analytics/bootstrap`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ product: "desktop" }),
    });
    if (!resp.ok) return null;
    const data = await resp.json();
    const token = typeof data?.install_token === "string" ? data.install_token : null;
    if (!token) return null;
    localStorage.setItem(INSTALL_TOKEN_KEY, token);
    return token;
  } catch {
    return null;
  }
}

async function getInstallToken(serverUrl: string): Promise<string | null> {
  const existing = localStorage.getItem(INSTALL_TOKEN_KEY);
  if (existing) return existing;
  return bootstrapInstallToken(serverUrl);
}

export async function trackAnalyticsEvent(eventName: AnalyticsEventName): Promise<void> {
  try {
    const settings = await getSettings();
    const authState = settings.noren_pro_logged_in ? "signed_in" : "signed_out";
    const inferenceMode = settings.noren_pro_logged_in ? "noren_managed" : "byok";
    const serverUrl = (settings.server_url || DEFAULT_SERVER_URL).replace(/\/+$/, "");
    let installToken = await getInstallToken(serverUrl);
    if (!installToken) return;

    let resp = await fetch(`${serverUrl}/v1/analytics/events`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        events: [{
          install_token: installToken,
          event_name: eventName,
          product: "desktop",
          auth_state: authState,
          inference_mode: inferenceMode,
        }],
      }),
    });

    if (resp.status === 400) {
      localStorage.removeItem(INSTALL_TOKEN_KEY);
      installToken = await bootstrapInstallToken(serverUrl);
      if (!installToken) return;
      resp = await fetch(`${serverUrl}/v1/analytics/events`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          events: [{
            install_token: installToken,
            event_name: eventName,
            product: "desktop",
            auth_state: authState,
            inference_mode: inferenceMode,
          }],
        }),
      });
    }

    void resp;
  } catch {
    // Analytics must never break product flows.
  }
}

export async function trackGenerationUsedDaily(): Promise<void> {
  const today = new Date().toISOString().slice(0, 10);
  const settings = await getSettings();
  const mode = settings.noren_pro_logged_in ? "noren_managed" : "byok";
  const key = `noren:analytics_last_generation_day:${mode}`;
  if (localStorage.getItem(key) === today) return;
  localStorage.setItem(key, today);
  await trackAnalyticsEvent("generation_used");
}

export async function ensureAnalyticsBootstrap(): Promise<void> {
  try {
    const serverUrl = await getServerUrl();
    await getInstallToken(serverUrl);
  } catch {
    // Analytics must never break product flows.
  }
}
