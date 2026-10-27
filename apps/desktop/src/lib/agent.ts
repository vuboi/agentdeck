import { connectors } from './connectors.svelte';
import { creditModel } from './credit-model';

// The UI side of the agent event stream. The Rust side (crates/adapters/src/agent) turns each CLI's output
// into these events; nothing here knows a CLI's format.

export type AgentEvent =
  | { kind: 'delta'; text: string }
  | { kind: 'thinking'; text: string }
  | { kind: 'tool_call'; id: string; name: string; input: string; change: FileChange | null; background: boolean }
  | { kind: 'tool_result'; id: string; output: string; is_error: boolean; image?: string }
  | { kind: 'tool_output_delta'; id: string; delta: string }
  | { kind: 'permission_denied'; requests: PermissionRequest[] }
  | { kind: 'done'; session_id: string | null; text: string; cost_usd: number | null }
  | { kind: 'usage'; input_tokens: number; output_tokens: number; cache_read_tokens: number; cache_write_tokens: number; new_call: boolean }
  | { kind: 'error'; message: string }
  | { kind: 'raw'; source: string; label: string; line: string };

/** A tool the CLI would not run without approval. `allow` is what approves exactly this call. */
export type PermissionRequest = { id: string; tool: string; summary: string; allow: string };

/** What a Write or Edit does to a file. */
export type FileChange = { path: string; added: number; removed: number; before: string; after: string };

export type AgentEnvelope = { session: string; turn: string; event: AgentEvent };

/** Start one turn. Resolves as soon as the CLI is started; the answer arrives as `agent-event`s. */
export async function sendAgentTurn(args: {
  session: string;
  turn: string;
  agent: string;
  prompt: string;
  agentSession: string | null;
  resume: boolean;
  model: string | null;
  permissionMode: string | null;
  effort: string | null;
  allowedTools: string[];
  cwd: string | null;
}): Promise<void> {
  const { invoke } = await import('@tauri-apps/api/core');
  // A `credit:` Claude model runs the same CLI on the Console API key, so the turn spends API credit (models.svelte.ts).
  const credit = args.agent === 'claude' ? creditModel(args.model) : null;
  const apiKey = credit ? connectors.claudeCredit.apiKey.trim() || null : null;
  await invoke('agent_send', { ...args, model: credit ?? args.model, apiKey });
}

/** Start one turn on a custom provider: an OpenAI-compatible server that gets the whole conversation. */
export async function sendCustomTurn(args: {
  session: string;
  turn: string;
  kind: string;
  baseUrl: string;
  apiKey: string | null;
  model: string;
  messages: { role: string; content: string; images?: string[]; files?: string[] }[];
  permissionMode: string | null;
  allowedTools: string[];
  cwd: string | null;
  effort: string | null;
  /** A free-tier provider: no file, shell or browser tool, only the public web (see custom_chat.rs). */
  restricted?: boolean;
}): Promise<void> {
  const { invoke } = await import('@tauri-apps/api/core');
  await invoke('custom_send', args);
}

/** Saves an image (base64, no prefix) where the agent CLI can open it, and returns its path. */
export async function saveAttachment(mime: string, data: string): Promise<string> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<string>('attachment_save', { mime, data });
}

/** Saves a document (base64, no prefix) where the agent CLI can open it, and returns its path. */
export async function saveFileAttachment(name: string, data: string): Promise<string> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<string>('attachment_save_file', { name, data });
}

/** Size in bytes of a file on disk (a dropped one); null for a folder. */
export async function fileSize(path: string): Promise<number | null> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<number | null>('attachment_stat', { path });
}

/** A picture saved on disk (a tool's screenshot) as a data URL; rejects for anything that is not a small image file. */
export async function readImageDataUrl(path: string): Promise<string> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<string>('read_image_data_url', { path });
}

/** The bytes of a file on disk (at most 50 MB), for the document preview. */
export async function readFileBytes(path: string): Promise<ArrayBuffer> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<ArrayBuffer>('read_file_bytes', { path });
}

/** The Gemini provider a `gemini` model is generated on (its address and key). */
export type ImageProvider = { kind: string; base_url: string; api_key: string | null; key_env: string | null };
/** `backend` names the generator that makes the image (image_backend.rs); a new one is added there and listed in the panel. */
export type ImageModelChoice = { model: string; backend: 'responses' | 'images' | 'gemini'; custom?: ImageProvider };
export type GeneratedImage = { model: string; backend: string; path: string; variant: number };
export type ImageGenerationFailure = { model: string; backend: string; variant: number; error: string };
export type ImageGenerateResult = { run_id: string; images: GeneratedImage[]; failures: ImageGenerationFailure[] };
export type ImageHistoryImage = { id: string; model: string; backend: string; variant: number; path: string; sha256: string; bytes: number };
export type ImageHistoryRun = {
  id: string;
  created_at: number;
  prompt: string;
  models: string[];
  variants: number;
  status: 'done' | 'stopped' | 'failed';
  failures: string[];
  images: ImageHistoryImage[];
};
/** The settings picked under the prompt box, as sentences for the model. `null` is Auto: nothing is sent for it. */
export type ImageSettings = { style: string | null; domain: string | null; audience: string | null; ratio: string | null; resolution: string | null };
export type SuggestedKeyword = { category: string; value: string };
export type ImagePromptSuggestResult = { model: string; suggestions: string[]; keywords: SuggestedKeyword[] };

/** Generates one or more independent image candidates through the local codex-imagen skill. */
export async function generateImages(args: {
  prompt: string;
  models: ImageModelChoice[];
  variants: number;
  referencePaths?: string[];
  settings?: ImageSettings;
  cwd?: string | null;
}): Promise<ImageGenerateResult> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<ImageGenerateResult>('image_generate', {
    request: {
      prompt: args.prompt,
      models: args.models,
      variants: args.variants,
      reference_paths: args.referencePaths ?? [],
      config: args.settings ?? {},
      cwd: args.cwd ?? null,
    },
  });
}

/** Stops the image run in progress: candidates already made stay, the running generator is killed. */
export async function stopImageGeneration(): Promise<void> {
  const { invoke } = await import('@tauri-apps/api/core');
  await invoke('image_stop');
}

/** The newest image runs, saved on this machine, with their candidate files. */
export async function listImageHistory(limit = 50): Promise<ImageHistoryRun[]> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<ImageHistoryRun[]>('image_history_list', { limit });
}

/** Asks the authenticated Codex Responses API to improve an image prompt. */
export async function suggestImagePrompts(args: { prompt: string; model: string; provider?: string; custom?: { kind: string; baseUrl: string; apiKey: string | null; keyEnv: string | null }; referencePaths?: string[]; settings?: ImageSettings }): Promise<ImagePromptSuggestResult> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<ImagePromptSuggestResult>('image_prompt_suggest', {
    request: {
      prompt: args.prompt,
      model: args.model,
      provider: args.provider ?? null,
      custom: args.custom ? { kind: args.custom.kind, base_url: args.custom.baseUrl, api_key: args.custom.apiKey, key_env: args.custom.keyEnv } : null,
      reference_paths: args.referencePaths ?? [],
      config: args.settings ?? {},
    },
  });
}

/** Shows an attached file, selected, in the system file manager. */
export async function revealFile(path: string): Promise<void> {
  const { invoke } = await import('@tauri-apps/api/core');
  await invoke('attachment_reveal', { path });
}

/** Subscribe to every agent event. Returns the unlisten function. */
export async function listenAgentEvents(onEvent: (envelope: AgentEnvelope) => void): Promise<() => void> {
  const { listen } = await import('./tauri-event');
  return listen<AgentEnvelope>('agent-event', (event) => onEvent(event.payload));
}

/** Stops the run of a chat. With `turn`, only that run: a stop that arrives late never ends the chat's next run. */
export async function stopAgentTurn(session: string, turn?: string): Promise<void> {
  const { invoke } = await import('@tauri-apps/api/core');
  await invoke('agent_stop', { session, turn: turn ?? null });
}

/** A delegation tool called by an agent through the app's MCP server (src-tauri/src/delegate.rs). */
export type DelegateCall = {
  callId: string;
  tool: 'delegate_task' | 'list_agents' | 'task_status' | 'task_stop';
  /** The chat whose agent called; empty when the agent's MCP link names none (Antigravity's stored link). */
  caller: string;
  args: Record<string, unknown>;
};

/** Subscribe to the calls of the delegation tools. Answer each with `replyDelegateCall`. */
export async function listenDelegateCalls(onCall: (call: DelegateCall) => void): Promise<() => void> {
  const { listen } = await import('./tauri-event');
  return listen<DelegateCall>('agent-delegate', (event) => onCall(event.payload));
}

/** A blocking delegate_task call that nobody waits for any more (its agent was stopped): its worker should stop. */
export async function listenDelegateAborts(onAbort: (callId: string) => void): Promise<() => void> {
  const { listen } = await import('./tauri-event');
  return listen<{ callId: string }>('agent-delegate-abort', (event) => onAbort(event.payload.callId));
}

/** The answer to a delegation tool call: the text the agent reads, and whether it is an error. */
export async function replyDelegateCall(callId: string, text: string, isError: boolean): Promise<void> {
  const { invoke } = await import('@tauri-apps/api/core');
  await invoke('delegate_reply', { callId, text, isError });
}

/** Tells the Rust side this page just loaded: calls still waiting for the page before it are answered with an error. */
export async function delegateReady(): Promise<void> {
  const { invoke } = await import('@tauri-apps/api/core');
  await invoke('delegate_ready');
}
