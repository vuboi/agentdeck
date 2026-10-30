import { connectors } from '../agents/connectors.svelte';
import { creditModel } from '../agents/credit-model';
import { convertFileSrc } from '@tauri-apps/api/core';

// The UI side of the agent event stream. The Rust side (crates/adapters/src/agent) turns each CLI's output
// into these events; nothing here knows a CLI's format.

export type AgentEvent =
  | { kind: 'delta'; text: string }
  | { kind: 'thinking'; text: string }
  | { kind: 'tool_call'; id: string; name: string; input: string; change: FileChange | null; background: boolean }
  | { kind: 'tool_result'; id: string; output: string; is_error: boolean; image?: string; change?: FileChange | null }
  | { kind: 'tool_output_delta'; id: string; delta: string }
  | { kind: 'permission_denied'; requests: PermissionRequest[] }
  | { kind: 'done'; session_id: string | null; text: string; cost_usd: number | null }
  | { kind: 'usage'; input_tokens: number; output_tokens: number; cache_read_tokens: number; cache_write_tokens: number; new_call: boolean }
  | { kind: 'error'; message: string }
  /** An event of child work (a sub-agent): drawn under the tool call `parent`, never in the main answer. */
  | { kind: 'scoped'; parent: string; event: AgentEvent }
  /** Where a sub-agent or a background job stands; `tool_use_id` is the call that started it. */
  | { kind: 'task'; id: string; tool_use_id: string | null; status: TaskStatus; label: string | null; summary: string | null; tools: number | null }
  /** The CLI went on by itself (background work ended): what follows is a new reply, not an answer to a message. */
  | { kind: 'auto_turn'; reason: string }
  | { kind: 'raw'; source: string; label: string; line: string };

export type TaskStatus = 'started' | 'progress' | 'done' | 'failed';

/** A tool the CLI would not run without approval. `allow` is what approves exactly this call. */
export type PermissionRequest = { id: string; tool: string; summary: string; allow: string };

/** What a Write or Edit does to a file. */
export type FileChange = { path: string; added: number; removed: number; before: string; after: string };

export type AgentEnvelope = { session: string; turn: string; message_id?: string; event: AgentEvent };

/** What a turn is started with, apart from its message. */
export type AgentTurnSettings = {
  session: string;
  agent: string;
  agentSession: string | null;
  resume: boolean;
  model: string | null;
  permissionMode: string | null;
  effort: string | null;
  allowedTools: string[];
  cwd: string | null;
};

/** The model and key a turn runs on: a `credit:` Claude model runs the same CLI on the Console API key, so the turn
 *  spends API credit (models.svelte.ts). */
function billing(args: AgentTurnSettings): { model: string | null; apiKey: string | null } {
  const credit = args.agent === 'claude' ? creditModel(args.model) : null;
  if (!credit) return { model: args.model, apiKey: null };
  const apiKey = connectors.claudeCredit.apiKey.trim();
  // Never falls back to the plan's subscription: the turn stops and names what is missing.
  if (!apiKey) throw new Error('chưa có key Console');
  return { model: credit, apiKey };
}

/** Start one turn. Resolves as soon as the CLI is started; the answer arrives as `agent-event`s. */
/** `keepOpen: false`: the turn runs once and its CLI is not kept for the next one (a side answer or delegated worker). */
export async function sendAgentTurn(args: AgentTurnSettings & { turn: string; prompt: string; messageId?: string; keepOpen?: boolean }): Promise<void> {
  const { invoke } = await import('@tauri-apps/api/core');
  await invoke('agent_send', { ...args, ...billing(args) });
}

/** The user's answer to a Copilot permission card: the SDK is waiting on it inside the same turn. */
export async function answerCopilotPermission(requestId: string, allow: boolean): Promise<void> {
  const { invoke } = await import('@tauri-apps/api/core');
  await invoke('copilot_permission_answer', { requestId, allow });
}

/** Starts the chat's CLI ahead of its next turn (the user is typing it), with the settings that turn will have. */
export async function prewarmAgent(args: AgentTurnSettings): Promise<void> {
  const { invoke } = await import('@tauri-apps/api/core');
  await invoke('agent_prewarm', { ...args, ...billing(args) });
}

/** Start one turn on a custom provider: an OpenAI-compatible server that gets the whole conversation. */
export async function sendCustomTurn(args: {
  session: string;
  turn: string;
  messageId?: string;
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

/**
 * A picture on disk as a URL the page can show directly (Tauri's asset protocol, scoped in tauri.conf.json): the bytes are
 * read by the browser when drawn, not copied through IPC or kept as base64. Use for pictures under the scoped folders.
 */
export async function pictureUrl(path: string): Promise<string> {
  const { convertFileSrc } = await import('@tauri-apps/api/core');
  return convertFileSrc(path);
}

/** Where a picture is drawn from: its file when it has one (asset URL), else its data URL. Synchronous, for templates. */
export function pictureSrc(picture: { src?: string; path?: string }): string {
  return picture.path ? convertFileSrc(picture.path) : (picture.src ?? '');
}

/** The MIME type of a picture file, from its extension. */
export function mimeOfPath(path: string): string {
  const extension = path.slice(path.lastIndexOf('.') + 1).toLowerCase();
  return extension === 'jpg' || extension === 'jpeg' ? 'image/jpeg' : `image/${extension || 'png'}`;
}

/** The picture's bytes as a data URL, read from its file when it was kept as one (for APIs that take base64). */
export async function pictureData(picture: { src?: string; path?: string }): Promise<string> {
  return picture.src || readImageDataUrl(picture.path ?? '');
}

/** A data URL kept as its file: the URL to show it by (small, asset protocol) and the path it is sent and saved by. */
export async function keepPictureUrl(src: string): Promise<{ url: string; path?: string }> {
  const kept = await keepPicture<{ mime: string; src: string; path?: string }>({ mime: 'image/png', src });
  return kept.path ? { url: pictureSrc(kept), path: kept.path } : { url: src };
}

/**
 * A picture about to be attached is written to its file at once (WebP, see picture_store.rs): the attachment then
 * carries the path, and what gets saved, sent or stored is the path, not base64. Failing to save keeps the data URL.
 */
export async function keepPicture<T extends { mime: string; src: string; path?: string }>(picture: T): Promise<T> {
  if (picture.path || !picture.src.startsWith('data:')) return picture;
  try {
    const path = await saveAttachment(picture.mime, picture.src.slice(picture.src.indexOf(',') + 1));
    return { ...picture, path };
  } catch {
    return picture;
  }
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
  const { listen } = await import('../app/platform/tauri-event');
  return listen<AgentEnvelope>('agent-event', (event) => onEvent(event.payload));
}

/** Stops the run of a chat. With `turn`, only that run: a stop that arrives late never ends the chat's next run. */
export async function stopAgentTurn(session: string, turn?: string): Promise<void> {
  const { invoke } = await import('@tauri-apps/api/core');
  await invoke('agent_stop', { session, turn: turn ?? null });
}

/** Hands a message to the turn running in a chat (Claude only): the CLI reads it at its next step, in the same turn. False when it was not taken. */
export async function injectAgentMessage(session: string, text: string): Promise<boolean> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<boolean>('agent_inject', { session, text });
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
  const { listen } = await import('../app/platform/tauri-event');
  return listen<DelegateCall>('agent-delegate', (event) => onCall(event.payload));
}

/** A blocking delegate_task call that nobody waits for any more (its agent was stopped): its worker should stop. */
export async function listenDelegateAborts(onAbort: (callId: string) => void): Promise<() => void> {
  const { listen } = await import('../app/platform/tauri-event');
  return listen<{ callId: string }>('agent-delegate-abort', (event) => onAbort(event.payload.callId));
}

/** The answer to a delegation tool call: the text the agent reads, and whether it is an error. */
export async function replyDelegateCall(callId: string, text: string, isError: boolean): Promise<void> {
  const { invoke } = await import('@tauri-apps/api/core');
  await invoke('delegate_reply', { callId, text, isError });
}

/** A worker is about to start: true when it may (fewer than 3 run), false when it is queued until a place frees. */
export async function enterWorkerSlot(taskId: string): Promise<boolean> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<boolean>('delegate_enter', { taskId });
}

/** A task ended: its place goes to the next queued one. */
export async function releaseWorkerSlot(taskId: string): Promise<void> {
  const { invoke } = await import('@tauri-apps/api/core');
  await invoke('delegate_release', { taskId });
}

/** A queued task's turn has come: its worker may start. */
export async function listenWorkerSlots(onSlot: (taskId: string) => void): Promise<() => void> {
  const { listen } = await import('../app/platform/tauri-event');
  return listen<{ taskId: string }>('agent-delegate-slot', (event) => onSlot(event.payload.taskId));
}

/** Tells the Rust side this page just loaded: calls still waiting for the page before it are answered with an error. */
export async function delegateReady(): Promise<void> {
  const { invoke } = await import('@tauri-apps/api/core');
  await invoke('delegate_ready');
}
