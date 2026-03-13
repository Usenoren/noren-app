/**
 * Parse raw API/system errors into clean, user-friendly messages.
 */
export function friendlyError(raw: unknown): string {
  const msg = String(raw);

  // Rate limit errors (Gemini, OpenAI, Anthropic, etc.)
  if (msg.includes("429") || msg.includes("Too Many Requests") || msg.includes("rate limit") || msg.includes("RESOURCE_EXHAUSTED")) {
    if (msg.includes("gemini") || msg.includes("googleapis")) {
      return "Gemini rate limit reached. The free tier allows ~20 requests per day. Wait a bit or switch to a different provider in Settings.";
    }
    if (msg.includes("anthropic")) {
      return "Anthropic rate limit reached. Wait a moment and try again, or check your plan's usage limits.";
    }
    if (msg.includes("openai")) {
      return "OpenAI rate limit reached. Wait a moment and try again.";
    }
    return "Rate limit reached. Wait a moment and try again.";
  }

  // Quota exhausted
  if (msg.includes("quota") || msg.includes("Quota exceeded") || msg.includes("insufficient_quota")) {
    return "API quota exceeded. Check your provider's billing and plan limits.";
  }

  // Connection refused / unreachable
  if (msg.includes("error sending request") || msg.includes("Connection refused") || msg.includes("ConnectError")) {
    const urlMatch = msg.match(/\(https?:\/\/[^\s)]+\)/);
    const host = urlMatch ? ` (${extractHost(urlMatch[0])})` : "";
    if (msg.includes("usenoren.ai")) {
      return "Can't reach Noren servers. The service may not be available yet.";
    }
    if (msg.includes("localhost") || msg.includes("127.0.0.1")) {
      return `Can't connect to local server${host}. Make sure it's running.`;
    }
    return `Connection failed${host}. Check your internet and try again.`;
  }

  // DNS / host not found
  if (msg.includes("dns error") || msg.includes("Name or service not known") || msg.includes("getaddrinfo")) {
    return "Server not found. Check the URL in your provider settings.";
  }

  // Timeout
  if (msg.includes("timed out") || msg.includes("timeout") || msg.includes("Timeout")) {
    return "Request timed out. The server may be busy — try again.";
  }

  // Auth errors
  if (msg.includes("401") || msg.includes("Unauthorized") || msg.includes("invalid_api_key") || msg.includes("Invalid API key")) {
    return "Invalid API key. Check your key in Settings.";
  }
  if (msg.includes("403") || msg.includes("Forbidden")) {
    return "Access denied. Your API key may not have permission for this model.";
  }

  // Model not found
  if (msg.includes("404") && (msg.includes("model") || msg.includes("Model"))) {
    return "Model not found. Check the model ID in Settings.";
  }

  // No profile
  if (msg.includes("No voice profile") || msg.includes("profile not found")) {
    return "No voice profile found. Create one in the Profiles tab first.";
  }

  // Not logged in
  if (msg.includes("Not logged in")) {
    return "Not signed in to Noren Pro. Go to Settings to sign in.";
  }

  // Generic server errors
  if (msg.includes("500") || msg.includes("Internal Server Error")) {
    return "Server error. Try again in a moment.";
  }
  if (msg.includes("503") || msg.includes("Service Unavailable")) {
    return "Noren is temporarily down for maintenance. Try again in a few minutes.";
  }
  if (msg.includes("502") || msg.includes("Bad Gateway")) {
    return "Server temporarily unavailable. Try again in a moment.";
  }

  // If the raw message is reasonably short and readable, show it
  // Otherwise truncate the noise
  const cleaned = msg
    .replace(/^(LLM error: |Error: |error: )/, "")
    .trim();

  if (cleaned.length > 200) {
    // Try to extract just the first sentence or meaningful part
    const firstSentence = cleaned.match(/^[^.{[\n]+/);
    if (firstSentence && firstSentence[0].length > 10) {
      return firstSentence[0].trim() + ".";
    }
    return cleaned.slice(0, 150).trim() + "...";
  }

  return cleaned;
}

function extractHost(urlStr: string): string {
  try {
    const clean = urlStr.replace(/[()]/g, "");
    const url = new URL(clean);
    return url.host;
  } catch {
    return urlStr;
  }
}
