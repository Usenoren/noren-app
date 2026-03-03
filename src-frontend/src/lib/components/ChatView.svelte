<script lang="ts">
  import { chatSend, getProfileOverview, listFormats, type ChatMessage } from "$lib/api/tauri";
  import { friendlyError } from "$lib/utils/errors";
  import LoadingSpinner from "./LoadingSpinner.svelte";

  let messages: ChatMessage[] = $state([]);
  let input = $state("");
  let isLoading = $state(false);
  let error = $state("");
  let format = $state("general");
  let formats = $state<string[]>([]);
  let totalTokens = $state(0);
  let messagesContainer: HTMLDivElement | undefined = $state();

  $effect(() => {
    getProfileOverview().then((overview) => {
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
  });

  function scrollToBottom() {
    requestAnimationFrame(() => {
      if (messagesContainer) {
        messagesContainer.scrollTop = messagesContainer.scrollHeight;
      }
    });
  }

  async function handleSend() {
    const text = input.trim();
    if (!text || isLoading) return;

    const userMessage: ChatMessage = { role: "user", content: text };
    messages = [...messages, userMessage];
    input = "";
    error = "";
    isLoading = true;
    scrollToBottom();

    try {
      const result = await chatSend({ messages, format });
      const assistantMessage: ChatMessage = { role: "assistant", content: result.text };
      messages = [...messages, assistantMessage];
      totalTokens += result.input_tokens + result.output_tokens;
      scrollToBottom();
    } catch (e) {
      error = friendlyError(e);
    } finally {
      isLoading = false;
    }
  }

  function handleNewChat() {
    messages = [];
    totalTokens = 0;
    error = "";
    input = "";
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
            <div class="flex justify-end animate-fade-in-up">
              <div class="max-w-[80%] px-3.5 py-2.5 bg-primary/10 text-foreground rounded-2xl rounded-br-md">
                <p class="text-sm whitespace-pre-wrap leading-relaxed">{msg.content}</p>
              </div>
            </div>
          {:else}
            <div class="flex justify-start animate-fade-in-up">
              <div class="max-w-[80%] px-3.5 py-2.5 bg-surface border border-border text-foreground rounded-2xl rounded-bl-md">
                <p class="text-sm whitespace-pre-wrap leading-relaxed">{msg.content}</p>
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
    <div class="mx-4 mb-2 p-2.5 bg-tint border border-border rounded-md text-xs text-muted leading-relaxed">
      {error}
    </div>
  {/if}

  <!-- Input -->
  <div class="px-4 py-3 border-t border-border shrink-0">
    <div class="flex items-end gap-2 max-w-2xl mx-auto">
      <textarea
        bind:value={input}
        onkeydown={handleKeydown}
        oninput={autoResize}
        class="flex-1 p-3 text-sm border border-border resize-none bg-surface text-foreground placeholder-muted rounded-xl focus:outline-none focus:border-secondary"
        rows={1}
        placeholder="Message..."
        disabled={isLoading}
      ></textarea>
      <button
        onclick={handleSend}
        disabled={!input.trim() || isLoading}
        class="p-2.5 rounded-xl transition-colors cursor-pointer shrink-0
          {!input.trim() || isLoading
            ? 'bg-surface text-muted border border-border cursor-not-allowed opacity-50'
            : 'bg-primary text-white hover:bg-primary-hover'}"
        aria-label="Send"
      >
        <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
          <path stroke-linecap="round" stroke-linejoin="round" d="M6 12L3.269 3.126A59.768 59.768 0 0121.485 12 59.77 59.77 0 013.27 20.876L5.999 12zm0 0h7.5" />
        </svg>
      </button>
    </div>
    <p class="text-[10px] text-muted text-center mt-1.5">Cmd+Enter to send</p>
  </div>
</div>
