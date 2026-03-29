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
  import loomIdleUrl from "../../assets/loom-idle.png";
  import { toastInfo, toastWarning } from "$lib/stores/toast.svelte";

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
    syncChatsFromServer().then(() => refreshHistory()).catch(() => toastInfo("Chat sync unavailable"));
    refreshHistory();
  });

  // Sync chats when window regains focus
  $effect(() => {
    const onFocus = () => {
      syncChatsFromServer().then(() => refreshHistory()).catch(() => toastInfo("Chat sync unavailable"));
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
      // Ignore
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
    } catch {
      toastWarning("Couldn't save chat locally");
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

  async function handleDeleteChat(id: string, e: MouseEvent | KeyboardEvent) {
    e.stopPropagation();
    try {
      await deleteChat(id);
      syncDeleteChat(id).catch(() => toastInfo("Chat deletion not synced to server"));
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
</script>

<div class="c-root">
  <!-- Toolbar -->
  <div class="c-toolbar">
    <span class="c-toolbar-title">Converse</span>

    <button class="c-toolbar-pill" onclick={handleNewChat}>+ New</button>

    <div class="c-history-wrap">
      <button
        class="c-toolbar-pill"
        class:active={showHistory}
        onclick={() => { showHistory = !showHistory; }}
      >
        <svg class="pill-icon" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
          <path stroke-linecap="round" stroke-linejoin="round" d="M12 6v6h4.5m4.5 0a9 9 0 11-18 0 9 9 0 0118 0z" />
        </svg>
        History
        {#if conversations.length > 0}
          <span class="pill-badge">{conversations.length}</span>
        {/if}
      </button>

      {#if showHistory}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div class="c-history-backdrop" onclick={() => { showHistory = false; }}></div>

        <div class="c-history-dropdown">
          {#if conversations.length === 0}
            <p class="c-history-empty">No previous chats</p>
          {:else}
            {#each conversations as conv}
              <button
                class="c-history-item"
                class:active={conversationId === conv.id}
                onclick={() => handleLoadChat(conv.id)}
              >
                <div class="c-history-info">
                  <div class="c-history-title">{conv.title}</div>
                  <div class="c-history-meta">
                    {relativeTime(conv.updated_at)} · {conv.message_count} messages
                  </div>
                </div>
                <span
                  role="button"
                  tabindex="-1"
                  class="c-history-delete"
                  onclick={(e) => handleDeleteChat(conv.id, e)}
                  onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); handleDeleteChat(conv.id, e); } }}
                >&times;</span>
              </button>
            {/each}
          {/if}
        </div>
      {/if}
    </div>

    <select bind:value={format} class="c-toolbar-select">
      {#each formats as fmt}
        <option value={fmt}>{fmt}</option>
      {/each}
    </select>

    <div class="c-toolbar-spacer"></div>

    {#if totalTokens > 0}
      <span class="c-toolbar-tokens">{totalTokens.toLocaleString()} tokens</span>
    {/if}
  </div>

  <!-- API key nudge -->
  {#if noApiKey}
    <div class="c-nudge">
      <p>
        Set up your API key in Settings to start chatting.
        <button onclick={() => emit("navigate", "settings")} class="c-nudge-link">Go to Settings</button>
      </p>
    </div>
  {/if}

  <!-- Messages -->
  <div class="c-messages" bind:this={messagesContainer}>
    {#if messages.length === 0}
      <div class="c-empty">
        <img src={loomIdleUrl} alt="" class="c-empty-img" />
        <div class="c-empty-title">Your space to think</div>
        <div class="c-empty-sub">Brainstorm, explore, plan. No voice enforcement.</div>
      </div>
    {:else}
      <div class="c-messages-inner">
        {#each messages as msg, i}
          {#if msg.role === "user"}
            <div class="c-msg c-msg-user">
              <div class="c-msg-wrap">
                <div class="c-bubble c-bubble-user">
                  <p class="c-bubble-text">{msg.content}</p>
                </div>
                <div class="c-msg-actions c-msg-actions-right">
                  <button
                    class="c-msg-action"
                    onclick={() => handleEditMessage(i)}
                    disabled={isLoading}
                  >Edit</button>
                </div>
              </div>
            </div>
          {:else}
            <div class="c-msg c-msg-assistant">
              <div class="c-msg-wrap">
                <div class="c-bubble c-bubble-assistant">
                  <div class="c-prose">{@html renderMarkdown(msg.content)}</div>
                </div>
                <div class="c-msg-actions c-msg-actions-left">
                  <button
                    class="c-msg-action"
                    onclick={() => handleCopyMessage(msg.content, i)}
                  >{copiedIndex === i ? "Copied" : "Copy"}</button>
                </div>
              </div>
            </div>
          {/if}
        {/each}

        {#if isLoading}
          <div class="c-msg c-msg-assistant">
            <div class="c-msg-wrap">
              <div class="c-loading-bubble">
                <LoadingSpinner />
                Thinking
              </div>
            </div>
          </div>
        {/if}
      </div>
    {/if}
  </div>

  <!-- Error -->
  {#if error}
    <div class="c-error">
      <span class="c-error-text">{error}</span>
      <button class="c-error-dismiss" onclick={() => { error = ""; }}>Dismiss</button>
    </div>
  {/if}

  <!-- Input bar -->
  <div class="c-input-bar">
    <div class="c-input-inner">
      {#if editSnapshot}
        <div class="c-edit-banner">
          <span class="c-edit-label">Editing message</span>
          <button class="c-edit-cancel" onclick={handleCancelEdit}>Cancel</button>
        </div>
      {/if}

      {#if attachedFiles.length > 0}
        <div class="c-attachment-chips">
          {#each attachedFiles as file, i}
            <div class="c-chip">
              <span class="c-chip-name">{file.name}</span>
              <button class="c-chip-remove" onclick={() => removeAttachment(i)}>&times;</button>
            </div>
          {/each}
        </div>
      {/if}

      <div class="c-input-row">
        <button
          class="c-toolbar-pill c-attach-btn"
          onclick={handleAttachFile}
          disabled={attachedFiles.length >= 3 || isLoading || noApiKey}
          title="Attach a file (PDF, text, etc.)"
        >
          <svg width="14" height="14" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
            <path stroke-linecap="round" stroke-linejoin="round" d="M18.375 12.739l-7.693 7.693a4.5 4.5 0 01-6.364-6.364l10.94-10.94A3 3 0 1119.5 7.372L8.552 18.32m.009-.01l-.01.01m5.699-9.941l-7.81 7.81a1.5 1.5 0 002.112 2.13" />
          </svg>
        </button>
        <textarea
          bind:value={input}
          onkeydown={handleKeydown}
          class="c-input-textarea"
          rows={1}
          placeholder={noApiKey ? "API key required..." : "What do you want to explore?"}
          disabled={isLoading || noApiKey}
        ></textarea>
        <button
          class="c-send-btn"
          onclick={handleSend}
          disabled={!input.trim() || isLoading || noApiKey}
          title="Send (Cmd+Enter)"
        >
          <svg fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
            <path stroke-linecap="round" stroke-linejoin="round" d="M6 12L3.269 3.126A59.768 59.768 0 0121.485 12 59.77 59.77 0 013.27 20.876L5.999 12zm0 0h7.5" />
          </svg>
        </button>
      </div>
      <div class="c-input-hint">Cmd+Enter to send</div>
    </div>
  </div>
</div>

<style>
  /* ====== ROOT ====== */
  .c-root {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  /* ====== TOOLBAR ====== */
  .c-toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 24px;
    border-bottom: 1px solid var(--color-border);
    flex-shrink: 0;
    width: 100%;
    box-sizing: border-box;
  }
  .c-toolbar-title {
    font-family: "Newsreader", serif;
    font-style: italic;
    font-size: 17px;
    color: var(--color-foreground);
    opacity: 0.7;
  }
  .c-toolbar-pill {
    padding: 5px 10px;
    border: 1px solid var(--color-border);
    border-radius: 6px;
    background: transparent;
    color: var(--color-muted);
    font-size: 11px;
    font-family: inherit;
    cursor: pointer;
    transition: all 150ms ease;
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  .c-toolbar-pill:hover { border-color: var(--color-muted); color: var(--color-foreground); }
  .c-toolbar-pill.active { border-color: var(--color-secondary); color: var(--color-foreground); }
  .c-toolbar-pill:disabled { opacity: 0.4; cursor: not-allowed; }
  .c-toolbar-pill .pill-icon { width: 12px; height: 12px; opacity: 0.7; }
  .c-toolbar-pill .pill-badge { font-size: 9px; color: var(--color-secondary); font-weight: 600; }
  .c-toolbar-select {
    padding: 5px 10px;
    padding-right: 26px;
    border: 1px solid var(--color-border);
    border-radius: 6px;
    background: transparent;
    color: var(--color-muted);
    font-size: 11px;
    font-family: inherit;
    cursor: pointer;
    transition: all 150ms ease;
    appearance: none;
    -webkit-appearance: none;
    background-image: url("data:image/svg+xml;charset=utf-8,%3Csvg xmlns='http://www.w3.org/2000/svg' width='10' height='6'%3E%3Cpath d='M0 0l5 6 5-6z' fill='%23888'/%3E%3C/svg%3E");
    background-repeat: no-repeat;
    background-position: right 8px center;
    background-size: 10px 6px;
  }
  .c-toolbar-select:hover { border-color: var(--color-muted); color: var(--color-foreground); }
  .c-toolbar-select option { background: var(--color-surface); color: var(--color-foreground); }
  .c-toolbar-spacer { flex: 1; }
  .c-toolbar-tokens { font-size: 10px; color: var(--color-muted); opacity: 0.7; }

  /* ====== HISTORY DROPDOWN ====== */
  .c-history-wrap { position: relative; }
  .c-history-backdrop { position: fixed; inset: 0; z-index: 10; }
  .c-history-dropdown {
    position: absolute;
    top: calc(100% + 6px);
    left: 0;
    z-index: 20;
    width: 280px;
    max-height: 320px;
    overflow-y: auto;
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    box-shadow: 0 8px 24px rgba(0,0,0,0.4);
    animation: c-dropIn 200ms cubic-bezier(0.16, 1, 0.3, 1) both;
  }
  @keyframes c-dropIn {
    from { opacity: 0; transform: translateY(-6px); }
    to { opacity: 1; transform: translateY(0); }
  }
  .c-history-empty {
    padding: 12px;
    font-size: 11px;
    color: var(--color-muted);
    text-align: center;
  }
  .c-history-item {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 10px 12px;
    border: none;
    border-bottom: 1px solid var(--color-border);
    cursor: pointer;
    transition: background 150ms ease;
    width: 100%;
    background: none;
    font-family: inherit;
    text-align: left;
    color: inherit;
  }
  .c-history-item:last-child { border-bottom: none; }
  .c-history-item:hover { background: var(--color-tint); }
  .c-history-item.active { background: rgba(122,51,64,0.06); }
  .c-history-info { flex: 1; min-width: 0; }
  .c-history-title {
    font-size: 12px;
    color: var(--color-foreground);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    line-height: 1.4;
  }
  .c-history-meta { font-size: 10px; color: var(--color-muted); margin-top: 2px; }
  .c-history-delete {
    flex-shrink: 0;
    background: none;
    border: none;
    color: var(--color-muted);
    cursor: pointer;
    opacity: 0;
    transition: opacity 150ms ease, color 150ms ease;
    padding: 2px;
    font-size: 14px;
    line-height: 1;
  }
  .c-history-item:hover .c-history-delete { opacity: 0.6; }
  .c-history-delete:hover { color: var(--color-error); opacity: 1; }

  /* ====== NUDGE ====== */
  .c-nudge {
    margin: 12px 24px 0;
    padding: 8px 12px;
    background: var(--color-tint);
    border: 1px solid rgba(255,180,50,0.15);
    border-radius: 8px;
    font-size: 11px;
    color: var(--color-muted);
    line-height: 1.5;
    flex-shrink: 0;
  }
  .c-nudge-link {
    background: none;
    border: none;
    color: var(--color-secondary);
    font-weight: 500;
    cursor: pointer;
    font-family: inherit;
    font-size: inherit;
    padding: 0;
    transition: color 150ms ease;
  }
  .c-nudge-link:hover { color: var(--color-foreground); }

  /* ====== MESSAGES ====== */
  .c-messages {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 20px 24px;
  }
  .c-messages-inner {
    max-width: 680px;
    margin: 0 auto;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  /* Empty state */
  .c-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    gap: 8px;
    animation: c-fadeDown 500ms cubic-bezier(0.16, 1, 0.3, 1) both;
  }
  @keyframes c-fadeDown {
    from { opacity: 0; transform: translateY(-10px); }
    to { opacity: 1; transform: translateY(0); }
  }
  .c-empty-img { width: 80px; opacity: 0.4; filter: invert(1); }
  .c-empty-title {
    font-family: "Newsreader", serif;
    font-style: italic;
    font-size: 20px;
    font-weight: 400;
    color: var(--color-foreground);
    opacity: 0.6;
  }
  .c-empty-sub { font-size: 11px; color: var(--color-muted); opacity: 0.7; }

  /* Message bubbles */
  .c-msg { display: flex; }
  .c-msg-user { justify-content: flex-end; }
  .c-msg-assistant { justify-content: flex-start; }
  .c-msg-wrap { max-width: 75%; }

  .c-bubble {
    padding: 10px 14px;
    font-size: 13px;
    line-height: 1.65;
    border-radius: 14px;
  }
  .c-bubble-user {
    background: rgba(122, 51, 64, 0.12);
    border: 1px solid rgba(122, 51, 64, 0.18);
    border-bottom-right-radius: 4px;
    color: var(--color-foreground);
  }
  .c-bubble-text { white-space: pre-wrap; margin: 0; }
  .c-bubble-assistant {
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-bottom-left-radius: 4px;
    color: var(--color-foreground);
  }

  /* Hover actions */
  .c-msg-actions {
    display: flex;
    align-items: center;
    gap: 2px;
    margin-top: 4px;
    height: 18px;
    opacity: 0;
    transition: opacity 150ms ease;
  }
  .c-msg-actions-right { justify-content: flex-end; padding-right: 4px; }
  .c-msg-actions-left { justify-content: flex-start; padding-left: 4px; }
  .c-msg-wrap:hover .c-msg-actions { opacity: 1; }
  .c-msg-action {
    background: none;
    border: none;
    font-size: 10px;
    color: var(--color-muted);
    cursor: pointer;
    padding: 2px 6px;
    border-radius: 4px;
    font-family: inherit;
    transition: color 150ms ease;
  }
  .c-msg-action:hover { color: var(--color-foreground); }
  .c-msg-action:disabled { opacity: 0.4; cursor: not-allowed; }

  /* Markdown prose */
  .c-prose :global(p) { margin: 0 0 8px 0; }
  .c-prose :global(p:last-child) { margin-bottom: 0; }
  .c-prose :global(strong) { font-weight: 600; color: var(--color-foreground); }
  .c-prose :global(em) { font-style: italic; }
  .c-prose :global(code) {
    font-family: "SF Mono", "Fira Code", monospace;
    font-size: 11.5px;
    background: rgba(255,255,255,0.05);
    padding: 1px 5px;
    border-radius: 3px;
  }
  .c-prose :global(ul), .c-prose :global(ol) { margin: 6px 0; padding-left: 18px; }
  .c-prose :global(li) { margin-bottom: 3px; }
  .c-prose :global(pre) {
    background: rgba(255,255,255,0.03);
    border: 1px solid var(--color-border);
    border-radius: 6px;
    padding: 10px 12px;
    overflow-x: auto;
    margin: 8px 0;
  }
  .c-prose :global(pre code) {
    background: none;
    padding: 0;
    font-size: 11.5px;
    line-height: 1.5;
  }

  /* Loading state */
  .c-loading-bubble {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: 14px;
    border-bottom-left-radius: 4px;
    font-size: 12px;
    color: var(--color-muted);
    animation: c-breathe 2s ease-in-out infinite;
  }
  @keyframes c-breathe {
    0%, 100% { opacity: 0.5; }
    50% { opacity: 1; }
  }

  /* ====== ERROR ====== */
  .c-error {
    margin: 0 24px 8px;
    padding: 10px 14px;
    background: var(--color-tint);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    font-size: 12px;
    color: var(--color-muted);
    line-height: 1.5;
    display: flex;
    align-items: flex-start;
    gap: 8px;
    flex-shrink: 0;
  }
  .c-error-text { flex: 1; }
  .c-error-dismiss {
    flex-shrink: 0;
    padding: 2px 8px;
    font-size: 10px;
    font-weight: 500;
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    color: var(--color-foreground);
    border-radius: 4px;
    cursor: pointer;
    font-family: inherit;
    transition: border-color 150ms ease;
  }
  .c-error-dismiss:hover { border-color: var(--color-secondary); }

  /* ====== INPUT BAR ====== */
  .c-input-bar {
    flex-shrink: 0;
    border-top: 1px solid var(--color-border);
    padding: 12px 24px;
    width: 100%;
    box-sizing: border-box;
    display: flex;
    align-items: flex-end;
    justify-content: center;
  }
  .c-input-inner {
    width: 100%;
    max-width: 780px;
    display: flex;
    flex-direction: column;
    gap: 0;
  }
  .c-edit-banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 8px;
  }
  .c-edit-label {
    font-size: 10px;
    color: var(--color-secondary);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }
  .c-edit-cancel {
    font-size: 10px;
    color: var(--color-muted);
    background: none;
    border: none;
    cursor: pointer;
    font-family: inherit;
    transition: color 150ms ease;
  }
  .c-edit-cancel:hover { color: var(--color-foreground); }
  .c-attachment-chips {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
    margin-bottom: 8px;
  }
  .c-chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 3px 8px;
    background: var(--color-tint);
    border: 1px solid var(--color-border);
    border-radius: 4px;
    font-size: 10px;
    color: var(--color-secondary);
  }
  .c-chip-name {
    max-width: 120px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .c-chip-remove {
    background: none;
    border: none;
    color: var(--color-muted);
    cursor: pointer;
    font-size: 13px;
    line-height: 1;
    padding: 0;
    margin-left: 2px;
    transition: color 150ms ease;
  }
  .c-chip-remove:hover { color: var(--color-error); }
  .c-input-row {
    display: flex;
    align-items: flex-end;
    gap: 8px;
  }
  .c-attach-btn { padding: 9px 10px; }
  .c-input-textarea {
    flex: 1;
    padding: 10px 12px;
    font-size: 13px;
    font-family: inherit;
    color: var(--color-foreground);
    background: transparent;
    border: 1px solid var(--color-border);
    border-radius: 8px;
    resize: none;
    outline: none;
    transition: border-color 150ms ease;
    line-height: 1.5;
    field-sizing: content;
    min-height: 38px;
    max-height: 120px;
  }
  .c-input-textarea::placeholder { color: var(--color-muted); }
  .c-input-textarea:focus { border-color: var(--color-secondary); }
  .c-input-textarea:disabled { opacity: 0.5; }
  .c-send-btn {
    width: 38px;
    height: 38px;
    border-radius: 8px;
    border: none;
    cursor: pointer;
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all 150ms ease;
    background: var(--color-accent);
    color: white;
  }
  .c-send-btn svg { width: 15px; height: 15px; }
  .c-send-btn:hover { background: var(--color-accent-hover); }
  .c-send-btn:disabled {
    background: var(--color-surface);
    color: var(--color-muted);
    border: 1px solid var(--color-border);
    cursor: not-allowed;
    opacity: 0.5;
  }
  .c-input-hint {
    font-size: 10px;
    color: var(--color-muted);
    text-align: center;
    margin-top: 6px;
    opacity: 0.6;
  }
</style>
