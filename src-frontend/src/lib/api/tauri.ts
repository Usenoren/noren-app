import { invoke } from "@tauri-apps/api/core";

export interface GenerateResult {
  text: string;
  input_tokens: number;
  output_tokens: number;
}

export interface Config {
  provider: string;
  model: string;
  profileDir: string;
  anthropicApiKey?: string;
  openaiApiKey?: string;
  geminiApiKey?: string;
  serverUrl?: string;
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

export async function getConfig(): Promise<Config> {
  return invoke("get_config");
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

// --- Settings (M6) ---

export interface SettingsInfo {
  provider: string;
  model: string;
  has_anthropic_key: boolean;
  has_openai_key: boolean;
  has_gemini_key: boolean;
}

export async function getSettings(): Promise<SettingsInfo> {
  return invoke("get_settings");
}

export async function saveApiKey(provider: string, key: string): Promise<void> {
  return invoke("save_api_key", { provider, key });
}

export async function removeApiKey(provider: string): Promise<void> {
  return invoke("remove_api_key", { provider });
}

export async function updateProvider(provider: string): Promise<void> {
  return invoke("update_provider", { provider });
}

export async function updateModel(model: string): Promise<void> {
  return invoke("update_model", { model });
}

export async function testApiKey(
  provider: string,
  key: string,
  model?: string,
): Promise<string> {
  return invoke("test_api_key", { provider, key, model });
}

// --- Profiles (M7) ---

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
