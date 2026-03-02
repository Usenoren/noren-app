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
  inference_mode: "byok" | "noren_pro";
  noren_pro_logged_in: boolean;
}

export interface NorenProStatus {
  logged_in: boolean;
  email: string | null;
  inference_mode: string;
  tokens_used: number | null;
  tokens_limit: number | null;
  requests_this_month: number | null;
}

export async function generate(params: {
  prompt: string;
  format: string;
  level: string;
  context?: string;
  attachments?: string[];
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

// --- Noren Pro ---

export async function getNorenProStatus(): Promise<NorenProStatus> {
  return invoke("get_noren_pro_status");
}

export async function norenProLogin(email: string, password: string): Promise<NorenProStatus> {
  return invoke("noren_pro_login", { email, password });
}

export async function norenProSignup(email: string, password: string): Promise<NorenProStatus> {
  return invoke("noren_pro_signup", { email, password });
}

export async function norenProLogout(): Promise<void> {
  return invoke("noren_pro_logout");
}

export async function getNorenProUsage(): Promise<NorenProStatus> {
  return invoke("get_noren_pro_usage");
}

export async function setInferenceMode(mode: "byok" | "noren_pro"): Promise<void> {
  return invoke("set_inference_mode", { mode });
}

// --- Google OAuth ---

export interface GoogleOAuthInitResult {
  auth_url: string;
  session_id: string;
}

export interface GoogleOAuthPollResult {
  status: string;
  complete: boolean;
}

export async function googleOAuthInit(): Promise<GoogleOAuthInitResult> {
  return invoke("google_oauth_init");
}

export async function googleOAuthPoll(sessionId: string): Promise<GoogleOAuthPollResult> {
  return invoke("google_oauth_poll", { sessionId });
}

// --- Billing ---

export interface SubscriptionStatus {
  tier: "free" | "extraction" | "living" | "pro" | "teams";
  active: boolean;
  can_extract: boolean;
  can_generate_bundled: boolean;
  can_living_profile: boolean;
  can_sync: boolean;
  tokens_limit: number;
  current_period_end: string | null;
  cancel_at_period_end: boolean;
}

export interface CheckoutResult {
  checkout_url: string;
  session_id: string;
}

export async function getSubscriptionStatus(): Promise<SubscriptionStatus> {
  return invoke("get_subscription_status");
}

export async function createCheckout(tier: string): Promise<CheckoutResult> {
  return invoke("create_checkout", { tier });
}

export async function openBillingPortal(): Promise<string> {
  return invoke("open_billing_portal");
}

// --- Extraction ---

export interface ExtractionProgress {
  status: string;
  progress: number;
  error: string | null;
}

export async function startExtraction(params: {
  samples: string;
  format: string;
}): Promise<void> {
  return invoke("start_extraction", params);
}

// --- Comparison ---

export interface ComparisonResult {
  with_voice: GenerateResult;
  without_voice: GenerateResult;
}

export async function generateComparison(params: {
  prompt: string;
  format: string;
  context?: string;
  attachments?: string[];
}): Promise<ComparisonResult> {
  return invoke("generate_comparison", params);
}

// --- Attachments ---

export async function readFileAsText(path: string): Promise<string> {
  return invoke("read_file_as_text", { path });
}

// --- Living Profile ---

export interface LivingProfileStatus {
  enabled: boolean;
  edit_count: number;
  last_upload: string | null;
}

export interface ProfilePatch {
  patch_id: string;
  section: string;
  change_type: string;
  description: string;
  original_text: string | null;
  new_text: string | null;
  confidence: number;
  status: string;
}

export interface RefreshResult {
  patches: ProfilePatch[];
  signals_found: number;
  entries_analyzed: number;
}

export async function getLivingProfileStatus(): Promise<LivingProfileStatus> {
  return invoke("get_living_profile_status");
}

export async function setLivingProfileEnabled(enabled: boolean): Promise<void> {
  return invoke("set_living_profile_enabled", { enabled });
}

export async function logEdit(ctx: string, orig: string, edit: string, app: string): Promise<void> {
  return invoke("log_edit", { ctx, orig, edit, app });
}

export async function uploadEditLog(): Promise<number> {
  return invoke("upload_edit_log");
}

export async function refreshLivingProfile(): Promise<RefreshResult> {
  return invoke("refresh_living_profile");
}

export async function getProfilePatches(): Promise<ProfilePatch[]> {
  return invoke("get_profile_patches");
}

export async function approveProfilePatch(patchId: string): Promise<void> {
  return invoke("approve_profile_patch", { patchId });
}

export async function rejectProfilePatch(patchId: string): Promise<void> {
  return invoke("reject_profile_patch", { patchId });
}

// --- Sync ---

export interface SyncStatus {
  has_remote: boolean;
  remote_version: number | null;
  updated_at: string | null;
  local_checksum: string;
}

export async function syncProfileUp(): Promise<string> {
  return invoke("sync_profile_up");
}

export async function syncProfileDown(): Promise<string> {
  return invoke("sync_profile_down");
}

export async function getSyncStatus(): Promise<SyncStatus> {
  return invoke("get_sync_status");
}

// --- Profiles ---

export interface ProfileOverview {
  exists: boolean;
  path: string;
  formats: string[];
  is_server?: boolean;
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

export async function migrateProfileToServer(): Promise<string> {
  return invoke("migrate_profile_to_server");
}

export async function exportProfile(): Promise<string> {
  return invoke("export_profile");
}
