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
  hotkey: string;
  server_url: string | null;
  debug_mode: boolean;
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
  mode?: "generate" | "adapt";
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

export async function verifyEmail(code: string): Promise<string> {
  return invoke("verify_email", { code });
}

export async function resendOtp(): Promise<string> {
  return invoke("resend_otp");
}

export async function resendSetupEmail(email: string): Promise<string> {
  return invoke("resend_setup_email", { email });
}

export async function getNorenProUsage(): Promise<NorenProStatus> {
  return invoke("get_noren_pro_usage");
}

export async function setInferenceMode(mode: "byok" | "noren_pro"): Promise<void> {
  return invoke("set_inference_mode", { mode });
}

export async function updateHotkey(hotkeyStr: string): Promise<void> {
  return invoke("update_hotkey", { hotkeyStr });
}

export async function listOllamaModels(): Promise<string[]> {
  return invoke("list_ollama_models");
}

export async function listClaudeModels(): Promise<{ id: string; name: string }[]> {
  return invoke("list_claude_models");
}

export async function listGeminiModels(): Promise<{ id: string; name: string }[]> {
  return invoke("list_gemini_models");
}

export async function listOpenAIModels(): Promise<{ id: string; name: string }[]> {
  return invoke("list_openai_models");
}

export async function listCustomModels(): Promise<{ id: string; name: string }[]> {
  return invoke("list_custom_models");
}

export async function getThinkingSettings(): Promise<{ enabled: boolean; budget: number }> {
  return invoke("get_thinking_settings");
}

export async function setThinkingSettings(enabled: boolean, budget: number): Promise<void> {
  return invoke("set_thinking_settings", { enabled, budget });
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
  tier: "free" | "pro" | "teams";
  active: boolean;
  can_extract: boolean;
  can_generate_bundled: boolean;
  can_living_profile: boolean;
  can_sync: boolean;
  can_export: boolean;
  tokens_limit: number;
  current_period_end: string | null;
  cancel_at_period_end: boolean;
  one_time_purchases: string[];
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

// --- Guest Checkout ---

export interface GuestCheckoutStatus {
  paid: boolean;
  tier: string;
}

export interface RestoreResult {
  found: boolean;
  session_id: string | null;
}

export interface PendingCheckout {
  session_id: string;
  email: string;
  created_at: string;
}

export async function createGuestCheckout(email: string, tier: string): Promise<CheckoutResult> {
  return invoke("create_guest_checkout", { email, tier });
}

export async function pollGuestCheckout(sessionId: string): Promise<GuestCheckoutStatus> {
  return invoke("poll_guest_checkout", { sessionId });
}

export async function restoreGuestPurchase(email: string): Promise<RestoreResult> {
  return invoke("restore_guest_purchase", { email });
}

export async function storeExtractionReceipt(sessionId: string): Promise<void> {
  return invoke("store_extraction_receipt", { sessionId });
}

export async function hasExtractionReceipt(): Promise<boolean> {
  return invoke("has_extraction_receipt");
}

export async function hasUsedExtraction(): Promise<boolean> {
  return invoke("has_used_extraction");
}

export async function markExtractionUsed(): Promise<void> {
  return invoke("mark_extraction_used");
}

export async function storePendingCheckout(sessionId: string, email: string): Promise<void> {
  return invoke("store_pending_checkout", { sessionId, email });
}

export async function getPendingCheckout(): Promise<PendingCheckout | null> {
  return invoke("get_pending_checkout");
}

export async function clearPendingCheckout(): Promise<void> {
  return invoke("clear_pending_checkout");
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

export interface FormatGroup {
  format: string;
  samples: string;
}

export async function startExtractionMulti(params: {
  formatGroups: FormatGroup[];
}): Promise<void> {
  return invoke("start_extraction_multi", params);
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

// --- Chat ---

export interface ChatMessage {
  role: "user" | "assistant";
  content: string;
}

export interface Conversation {
  id: string;
  title: string;
  format: string;
  created_at: string;
  updated_at: string;
  total_tokens: number;
  messages: ChatMessage[];
}

export interface ConversationSummary {
  id: string;
  title: string;
  format: string;
  updated_at: string;
  message_count: number;
  total_tokens: number;
}

export async function chatSend(params: {
  messages: ChatMessage[];
  format: string;
  attachments?: string[];
  chatId?: string;
  chatTitle?: string;
}): Promise<GenerateResult> {
  return invoke("chat_send", params);
}

export async function saveChat(conversation: Conversation): Promise<void> {
  return invoke("save_chat", { conversation });
}

export async function listChats(): Promise<ConversationSummary[]> {
  return invoke("list_chats");
}

export async function loadChat(id: string): Promise<Conversation> {
  return invoke("load_chat", { id });
}

export async function deleteChat(id: string): Promise<void> {
  return invoke("delete_chat", { id });
}

export async function syncDeleteChat(id: string): Promise<void> {
  return invoke("sync_delete_chat", { id });
}

export async function syncChatsFromServer(): Promise<number> {
  return invoke("sync_chats_from_server");
}

export async function factoryReset(): Promise<void> {
  return invoke("factory_reset");
}

export async function showMainWindow(): Promise<void> {
  return invoke("show_main_window");
}

// ── Announcements ──────────────────────────────────────────────

export interface Announcement {
  id: string;
  type: string;
  title: string;
  body: string;
  cta_url: string | null;
  cta_label: string | null;
  published_at: string;
}

export async function fetchAnnouncements(since?: string): Promise<Announcement[]> {
  return invoke("fetch_announcements", { since: since ?? null });
}

export async function getAnnouncementSeen(): Promise<string | null> {
  return invoke("get_announcement_seen");
}

export async function saveAnnouncementSeen(ts: string): Promise<void> {
  return invoke("save_announcement_seen", { ts });
}
