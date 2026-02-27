import { invoke } from "@tauri-apps/api/core";

export interface GenerateResult {
  text: string;
  input_tokens: number;
  output_tokens: number;
}

export interface ProviderConfig {
  name: string;
  type: "anthropic" | "openai_compatible";
  baseUrl: string;
  model: string;
  requiresKey: boolean;
}

export interface SettingsInfo {
  provider: ProviderConfig;
  has_key: boolean;
}

export async function generate(params: {
  prompt: string;
  format: string;
  level: string;
  context?: string;
}): Promise<GenerateResult> {
  return invoke("generate", params);
}

export async function getContextText(): Promise<string | null> {
  return invoke("get_context_text");
}

export async function listFormats(): Promise<string[]> {
  return invoke("list_formats");
}

export async function injectGeneratedText(text: string): Promise<void> {
  return invoke("inject_generated_text", { text });
}

export async function checkPermissions(): Promise<boolean> {
  return invoke("check_permissions");
}

export async function requestPermissions(): Promise<boolean> {
  return invoke("request_permissions");
}

// --- Settings ---

export async function getSettings(): Promise<SettingsInfo> {
  return invoke("get_settings");
}

export async function setProvider(provider: {
  name: string;
  type?: string;
  baseUrl?: string;
  model?: string;
  requiresKey?: boolean;
}): Promise<void> {
  return invoke("set_provider", { provider });
}

export async function saveApiKey(key: string): Promise<void> {
  return invoke("save_api_key", { key });
}

export async function removeApiKey(): Promise<void> {
  return invoke("remove_api_key");
}

export async function updateModel(model: string): Promise<void> {
  return invoke("update_model", { model });
}

export async function updateBaseUrl(baseUrl: string): Promise<void> {
  return invoke("update_base_url", { baseUrl });
}

export async function testConnection(key?: string): Promise<string> {
  return invoke("test_connection", { key: key || null });
}

// --- Profiles ---

export interface ProfileOverview {
  exists: boolean;
  path: string;
  formats: string[];
}

export interface ProfileContent {
  core_identity: string;
  contexts: Record<string, string>;
  quality_check: string | null;
}

export async function getProfileOverview(): Promise<ProfileOverview> {
  return invoke("get_profile_overview");
}

export async function readProfileContent(): Promise<ProfileContent> {
  return invoke("read_profile_content");
}

export async function saveProfileEdit(params: {
  coreIdentity: string;
  contextFormat?: string;
  contextContent?: string;
}): Promise<void> {
  return invoke("save_profile_edit", {
    coreIdentity: params.coreIdentity,
    contextFormat: params.contextFormat,
    contextContent: params.contextContent,
  });
}
