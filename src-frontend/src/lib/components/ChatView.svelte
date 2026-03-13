<script lang="ts">
  import {
    chatSend,
    getProfileOverview,
    getSettings,
    listFormats,
    saveChat,
    listChats,
    loadChat,
    deleteChat,
    syncDeleteChat,
    syncChatsFromServer,
    readFileAsText,
    type ChatMessage,
    type ConversationSummary,
  } from "$lib/api/tauri";
  import { emit } from "@tauri-apps/api/event";
  import { open } from "@tauri-apps/plugin-dialog";
  import { friendlyError } from "$lib/utils/errors";
  import { marked } from "marked";
  import DOMPurify from "dompurify";
  import LoadingSpinner from "./LoadingSpinner.svelte";

  // Configure marked for inline rendering
  marked.setOptions({ breaks: true });

  function renderMarkdown(content: string): string {
    return DOMPurify.sanitize(marked.parse(content) as string);
  }

  // --- State ---
  let messages: ChatMessage[] = $state([]);
  let input = $state("");
  let isLoading = $state(false);
  let error = $state("");
  let format = $state("general");
  let formats = $state<string[]>([]);
  let totalTokens = $state(0);
  let messagesContainer: HTMLDivElement | undefined = $state();

  // Attachments
  let attachedFiles = $state<{ name: string; content: string }[]>([]);

  // Empty state awareness
  let hasProfile = $state(true);
  let noApiKey = $state(false);

  // History state
  let conversationId: string | null = $state(null);
  let conversationCreatedAt: string | null = $state(null);
  let conversations = $state<ConversationSummary[]>([]);
  let showHistory = $state(false);

  // --- Init ---
  $effect(() => {
    getProfileOverview().then((overview) => {
      hasProfile = overview.exists;
      let f = overview.formats;
      if (!f.includes("general")) {
        f = ["general", ...f];
      }
      formats = f;
      if (!f.includes(format)) {
        format = f[0];
      }
    });

    listFormats().then((f) => {
      if (formats.length <= 1) {
        if (!f.includes("general")) {
          f = ["general", ...f];
        }
        formats = f;
      }
    });

    getSettings().then((settings) => {
      if (settings.inference_mode === "byok" && !settings.has_key && settings.provider.requiresKey) {
        noApiKey = true;
      } else {
        noApiKey = false;
      }
    });

    // Pull remote chats then refresh list
    syncChatsFromServer().then(() => refreshHistory()).catch((e) => console.error("chat sync failed:", e));
    refreshHistory();
  });

  // Sync chats when window regains focus
  $effect(() => {
    const onFocus = () => {
      syncChatsFromServer().then(() => refreshHistory()).catch((e) => console.error("chat sync failed:", e));
    };
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  });

  // --- Helpers ---

  function generateId(): string {
    return Date.now().toString(36) + Math.random().toString(36).slice(2, 8);
  }

  function generateTitle(firstMessage: string): string {
    const clean = firstMessage.replace(/\n/g, " ").trim();
    return clean.length > 50 ? clean.slice(0, 50) + "..." : clean;
  }

  function nowISO(): string {
    return new Date().toISOString().replace(/\.\d+Z$/, "Z");
  }

  async function refreshHistory() {
    try {
      conversations = await listChats();
    } catch {
      // Ignore — history just won't show
    }
  }

  function scrollToBottom() {
    requestAnimationFrame(() => {
      if (messagesContainer) {
        messagesContainer.scrollTop = messagesContainer.scrollHeight;
      }
    });
  }

  function relativeTime(iso: string): string {
    const now = Date.now();
    const then = new Date(iso).getTime();
    const diffMs = now - then;
    const diffMin = Math.floor(diffMs / 60000);
    if (diffMin < 1) return "just now";
    if (diffMin < 60) return `${diffMin}m ago`;
    const diffHr = Math.floor(diffMin / 60);
    if (diffHr < 24) return `${diffHr}h ago`;
    const diffDay = Math.floor(diffHr / 24);
    if (diffDay === 1) return "yesterday";
    if (diffDay < 7) return `${diffDay}d ago`;
    return new Date(iso).toLocaleDateString();
  }

  let copiedIndex: number | null = $state(null);
  let editSnapshot: ChatMessage[] | null = $state(null);

  async function handleCopyMessage(content: string, index: number) {
    await navigator.clipboard.writeText(content);
    copiedIndex = index;
    setTimeout(() => { copiedIndex = null; }, 1500);
  }

  function handleEditMessage(index: number) {
    editSnapshot = [...messages];
    input = messages[index].content;
    messages = messages.slice(0, index);
  }

  function handleCancelEdit() {
    if (editSnapshot) {
      messages = editSnapshot;
      editSnapshot = null;
      input = "";
      scrollToBottom();
    }
  }

  // --- Actions ---

  async function handleAttachFile() {
    if (attachedFiles.length >= 3) {
      error = "Maximum 3 attachments allowed";
      return;
    }

    try {
      const selected = await open({
        multiple: false,
        filters: [
          {
            name: "Documents",
            extensions: ["txt", "md", "csv", "json", "xml", "html", "pdf", "yaml", "yml", "toml"],
          },
        ],
      });

      if (!selected) return;

      const path = selected as string;
      const content = await readFileAsText(path);
      const name = path.split("/").pop() || path;

      attachedFiles = [...attachedFiles, { name, content }];
    } catch (e) {
      error = friendlyError(e);
    }
  }

  function removeAttachment(index: number) {
    attachedFiles = attachedFiles.filter((_, i) => i !== index);
  }

  async function persistChat() {
    if (messages.length === 0) return;

    if (!conversationId) {
      conversationId = generateId();
      conversationCreatedAt = nowISO();
    }

    try {
      await saveChat({
        id: conversationId,
        title: generateTitle(messages[0].content),
        format,
        created_at: conversationCreatedAt!,
        updated_at: nowISO(),
        total_tokens: totalTokens,
        messages,
      });
      await refreshHistory();
    } catch (e) {
      console.error("persistChat failed:", e);
    }
  }

  async function handleSend() {
    const text = input.trim();
    if (!text || isLoading) return;

    const userMessage: ChatMessage = { role: "user", content: text };
    messages = [...messages, userMessage];
    input = "";
    error = "";
    editSnapshot = null;
    isLoading = true;
    scrollToBottom();

    // Ensure conversation ID exists before sending (needed for server-side chat sync)
    if (!conversationId) {
      conversationId = generateId();
      conversationCreatedAt = nowISO();
    }

    try {
      const attachmentContents = attachedFiles.length > 0
        ? attachedFiles.map((f) => f.content)
        : undefined;

      const result = await chatSend({
        messages,
        format,
        attachments: attachmentContents,
        chatId: conversationId,
        chatTitle: generateTitle(messages[0].content),
      });
      attachedFiles = [];
      const assistantMessage: ChatMessage = { role: "assistant", content: result.text };
      messages = [...messages, assistantMessage];
      totalTokens += result.input_tokens + result.output_tokens;
      scrollToBottom();

      // Auto-save after each successful exchange
      await persistChat();
    } catch (e) {
      error = friendlyError(e);
    } finally {
      isLoading = false;
    }
  }

  function handleNewChat() {
    conversationId = null;
    conversationCreatedAt = null;
    messages = [];
    totalTokens = 0;
    error = "";
    input = "";
    showHistory = false;
  }

  async function handleLoadChat(id: string) {
    try {
      const conv = await loadChat(id);
      conversationId = conv.id;
      conversationCreatedAt = conv.created_at;
      format = conv.format;
      totalTokens = conv.total_tokens;
      messages = conv.messages;
      showHistory = false;
      scrollToBottom();
    } catch (e) {
      error = friendlyError(e);
    }
  }

  async function handleDeleteChat(id: string, e: MouseEvent) {
    e.stopPropagation();
    try {
      await deleteChat(id);
      syncDeleteChat(id).catch((e) => console.error("syncDeleteChat failed:", e)); // fire-and-forget server sync
      if (conversationId === id) {
        handleNewChat();
      }
      await refreshHistory();
    } catch (err) {
      error = friendlyError(err);
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Enter" && e.metaKey) {
      e.preventDefault();
      handleSend();
    }
  }

  function autoResize(e: Event) {
    const textarea = e.target as HTMLTextAreaElement;
    textarea.style.height = "auto";
    textarea.style.height = Math.min(textarea.scrollHeight, 120) + "px";
  }
</script>

<div class="flex flex-col h-full animate-fade-in-up">
  <!-- Header -->
  <div class="flex items-center gap-3 px-4 py-3 border-b border-border shrink-0">
    <button
      onclick={handleNewChat}
      class="px-2.5 py-1 text-xs border border-border hover:border-secondary transition-colors cursor-pointer text-muted hover:text-foreground rounded-md"
    >
      New Chat
    </button>

    <div class="relative">
      <button
        onclick={() => { showHistory = !showHistory; }}
        class="px-2.5 py-1 text-xs border transition-colors cursor-pointer rounded-md
          {showHistory
            ? 'border-secondary text-foreground'
            : 'border-border text-muted hover:border-secondary hover:text-foreground'}"
      >
        <span class="inline-flex items-center gap-1.5">
          <svg class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
            <path stroke-linecap="round" stroke-linejoin="round" d="M12 6v6h4.5m4.5 0a9 9 0 11-18 0 9 9 0 0118 0z" />
          </svg>
          History
          {#if conversations.length > 0}
            <span class="text-[10px] text-secondary">{conversations.length}</span>
          {/if}
        </span>
      </button>

      {#if showHistory}
        <!-- Invisible backdrop to close dropdown on click-outside -->
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div class="fixed inset-0 z-10" onclick={() => { showHistory = false; }}></div>

        <div
          class="absolute top-full left-0 mt-1 z-20 w-72 max-h-80 overflow-y-auto bg-background border border-border rounded-lg"
          style="box-shadow: var(--shadow-dropdown)"
        >
          {#if conversations.length === 0}
            <p class="p-3 text-xs text-muted text-center">No previous chats</p>
          {:else}
            {#each conversations as conv}
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <!-- svelte-ignore a11y_click_events_have_key_events -->
              <div
                onclick={() => handleLoadChat(conv.id)}
                class="w-full flex items-start gap-2 px-3 py-2.5 text-left hover:bg-tint transition-colors cursor-pointer border-b border-border last:border-b-0 group
                  {conversationId === conv.id ? 'bg-primary/5' : ''}"
              >
                <div class="flex-1 min-w-0">
                  <p class="text-xs text-foreground truncate">{conv.title}</p>
                  <p class="text-[10px] text-muted mt-0.5">
                    {relativeTime(conv.updated_at)} · {conv.message_count} messages
                  </p>
                </div>
                <button
                  onclick={(e) => handleDeleteChat(conv.id, e)}
                  class="shrink-0 text-muted hover:text-error opacity-0 group-hover:opacity-100 transition-opacity cursor-pointer p-0.5"
                  aria-label="Delete conversation"
                >
                  <svg class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
                  </svg>
                </button>
              </div>
            {/each}
          {/if}
        </div>
      {/if}
    </div>

    <select
      bind:value={format}
      class="px-2 py-1 text-xs border border-border bg-surface text-foreground rounded-md focus:outline-none focus:border-secondary"
    >
      {#each formats as fmt}
        <option value={fmt}>{fmt}</option>
      {/each}
    </select>

    {#if totalTokens > 0}
      <span class="text-[10px] text-muted ml-auto">{totalTokens} tokens</span>
    {/if}
  </div>

  <!-- No API key banner (F5) -->
  {#if noApiKey}
    <div class="flex items-center gap-2 mx-4 mt-3 p-2 bg-tint border border-warning/20 rounded-lg">
      <p class="flex-1 text-[10px] text-muted leading-relaxed">
        Set up your API key in Settings to start chatting.
        <button
          onclick={() => emit("navigate", "settings")}
          class="text-secondary font-medium cursor-pointer hover:text-foreground"
        >Go to Settings</button>
      </p>
    </div>
  {/if}

  <!-- No profile nudge (F4) -->
  {#if !hasProfile && !noApiKey}
    <div class="flex items-center gap-2 mx-4 mt-3 p-2 bg-tint border border-secondary/20 rounded-lg">
      <p class="flex-1 text-[10px] text-muted leading-relaxed">
        Chat is using default voice. Create a profile for voice-matched responses.
        <button
          onclick={() => emit("navigate", "profiles")}
          class="text-secondary font-medium cursor-pointer hover:text-foreground"
        >Go to Profiles</button>
        <span class="text-border mx-0.5">|</span>
        <button
          onclick={() => emit("navigate", "extract")}
          class="text-secondary font-medium cursor-pointer hover:text-foreground"
        >Extract one</button>
      </p>
    </div>
  {/if}

  <!-- Messages -->
  <div
    bind:this={messagesContainer}
    class="flex-1 min-h-0 overflow-y-auto px-4 py-4"
  >
    {#if messages.length === 0}
      <div class="flex items-center justify-center h-full">
        <p class="text-sm text-muted">Start a conversation...</p>
      </div>
    {:else}
      <div class="flex flex-col gap-3 max-w-2xl mx-auto">
        {#each messages as msg, i}
          {#if msg.role === "user"}
            <div class="flex justify-end animate-fade-in-up group/usr">
              <div class="max-w-[80%]">
                <div class="px-3.5 py-2.5 bg-primary/10 text-foreground rounded-2xl rounded-br-md selectable">
                  <p class="text-sm whitespace-pre-wrap leading-relaxed">{msg.content}</p>
                </div>
                <div class="flex items-center justify-end gap-1 mt-1 mr-1 h-5 opacity-0 group-hover/usr:opacity-100 transition-opacity">
                  <button
                    onclick={() => handleEditMessage(i)}
                    class="px-1.5 py-0.5 text-[10px] text-muted hover:text-foreground cursor-pointer rounded transition-colors"
                    disabled={isLoading}
                  >
                    Edit
                  </button>
                </div>
              </div>
            </div>
          {:else}
            <div class="flex justify-start animate-weave-in group/msg">
              <div class="max-w-[80%]">
                <div class="px-3.5 py-2.5 bg-surface border border-border text-foreground rounded-2xl rounded-bl-md selectable animate-weave-shimmer">
                  <div class="text-sm leading-relaxed prose-chat">{@html renderMarkdown(msg.content)}</div>
                </div>
                <div class="flex items-center gap-1 mt-1 ml-1 h-5 opacity-0 group-hover/msg:opacity-100 transition-opacity">
                  <button
                    onclick={() => handleCopyMessage(msg.content, i)}
                    class="px-1.5 py-0.5 text-[10px] text-muted hover:text-foreground cursor-pointer rounded transition-colors"
                  >
                    {copiedIndex === i ? "Copied" : "Copy"}
                  </button>
                </div>
              </div>
            </div>
          {/if}
        {/each}

        {#if isLoading}
          <div class="flex justify-start animate-fade-in-up">
            <div class="px-3.5 py-2.5 bg-surface border border-border rounded-2xl rounded-bl-md">
              <span class="inline-flex items-center gap-2 text-sm text-muted animate-breathe">
                <LoadingSpinner /> Thinking
              </span>
            </div>
          </div>
        {/if}
      </div>
    {/if}
  </div>

  <!-- Error -->
  {#if error}
    <div class="mx-4 mb-2 p-2.5 bg-tint border border-border rounded-lg text-xs text-muted leading-relaxed">
      {error}
    </div>
  {/if}

  <!-- Input -->
  <div class="px-4 py-3 border-t border-border shrink-0">
    {#if editSnapshot}
      <div class="flex items-center justify-between max-w-2xl mx-auto mb-2">
        <span class="text-[10px] text-secondary font-medium uppercase tracking-wide">Editing message</span>
        <button
          onclick={handleCancelEdit}
          class="text-[10px] text-muted hover:text-foreground cursor-pointer transition-colors"
        >
          Cancel
        </button>
      </div>
    {/if}
    <div class="max-w-2xl mx-auto">
      <!-- Attachment chips -->
      {#if attachedFiles.length > 0}
        <div class="flex items-center gap-1.5 flex-wrap mb-1.5">
          {#each attachedFiles as file, i}
            <div class="inline-flex items-center gap-1 px-1.5 py-0.5 bg-tint border border-border rounded text-[10px] text-secondary">
              <span class="max-w-[120px] truncate">{file.name}</span>
              <button
                onclick={() => removeAttachment(i)}
                class="text-muted hover:text-error cursor-pointer ml-0.5"
                aria-label="Remove attachment"
              >&times;</button>
            </div>
          {/each}
        </div>
      {/if}

      <div class="flex items-end gap-2">
        <button
          onclick={handleAttachFile}
          class="p-2.5 rounded-xl transition-colors cursor-pointer shrink-0 text-muted hover:text-foreground border border-border hover:border-secondary"
          title="Attach a file (PDF, text, etc.)"
          disabled={attachedFiles.length >= 3 || isLoading || noApiKey}
          aria-label="Attach file"
        >
          <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
            <path stroke-linecap="round" stroke-linejoin="round" d="M18.375 12.739l-7.693 7.693a4.5 4.5 0 01-6.364-6.364l10.94-10.94A3 3 0 1119.5 7.372L8.552 18.32m.009-.01l-.01.01m5.699-9.941l-7.81 7.81a1.5 1.5 0 002.112 2.13" />
          </svg>
        </button>
        <textarea
          bind:value={input}
          onkeydown={handleKeydown}
          oninput={autoResize}
          class="flex-1 p-3 text-sm border border-border resize-none bg-surface text-foreground placeholder-muted rounded-xl focus:outline-none focus:border-secondary"
          rows={1}
          placeholder={noApiKey ? "API key required..." : "Message..."}
          disabled={isLoading || noApiKey}
        ></textarea>
        <button
          onclick={handleSend}
          disabled={!input.trim() || isLoading || noApiKey}
          class="p-2.5 rounded-xl transition-colors cursor-pointer shrink-0
            {!input.trim() || isLoading || noApiKey
              ? 'bg-surface text-muted border border-border cursor-not-allowed opacity-50'
              : 'bg-primary text-white hover:bg-primary-hover'}"
          aria-label="Send"
        >
          <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
            <path stroke-linecap="round" stroke-linejoin="round" d="M6 12L3.269 3.126A59.768 59.768 0 0121.485 12 59.77 59.77 0 013.27 20.876L5.999 12zm0 0h7.5" />
          </svg>
        </button>
      </div>
    </div>
    <p class="text-[10px] text-muted text-center mt-1.5">Cmd+Enter to send</p>
  </div>
</div>
