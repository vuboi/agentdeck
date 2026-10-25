// Client side of the read-only history readers in crates/adapters (Tauri commands `history_list`
// and `history_load`). Outside Tauri (plain browser dev) there is no disk access: lists are empty.
import type { Block } from './mock';

export type HistorySource = 'claude' | 'codex';

export type HistorySession = {
  id: string;
  source: HistorySource;
  title: string;
  model: string | null;
  cwd: string;
  started_at: string | null;
  updated_at: string | null;
  message_count: number;
  path: string;
  parent_id: string | null;
  fork_seq: number | null;
};

export type HistoryBlock =
  | { kind: 'thinking'; text: string }
  | { kind: 'response'; text: string }
  | { kind: 'command'; command: string; output: string; status: string; change?: { path: string; added: number; removed: number; before: string; after: string } };

export type HistoryUsage = { input: number; output: number; cache_read: number; cache_write: number };
export type HistoryMessage = { id: string; role: 'user' | 'assistant'; time: string; blocks: HistoryBlock[]; usage?: HistoryUsage; model?: string };

const bridge = () => (typeof window === 'undefined' ? undefined : (window as unknown as { __TAURI_INTERNALS__?: { metadata?: { currentWebview?: { label?: string } } } }).__TAURI_INTERNALS__);

/**
 * Whether this page is the Preview's own copy of the app (the Preview opened the app's dev address in a webview of its
 * own). That copy shares the backend, the events and the saved data with the real window, so it only reads: it saves
 * nothing, listens to no events and does not reset the pages.
 */
export const isPreviewCopy = () => Boolean(bridge()?.metadata?.currentWebview?.label?.startsWith('browser-'));

/**
 * Whether this page can call the app's commands. A release build grants them to the app's own window only, so the
 * Preview's copy runs as a plain web page there (the same as in a browser). A debug build grants them to that copy too
 * (the capability debug-preview, src-tauri/build.rs): it shows the real data, to look at and to try.
 */
export const inTauri = () => bridge() !== undefined && (import.meta.env.DEV || !isPreviewCopy());

async function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke: call } = await import('@tauri-apps/api/core');
  return call<T>(command, args);
}

export async function listHistory(): Promise<HistorySession[]> {
  return inTauri() ? invoke<HistorySession[]>('history_list') : [];
}

export async function loadHistory(source: HistorySource, path: string): Promise<HistoryMessage[]> {
  return inTauri() ? invoke<HistoryMessage[]>('history_load', { source, path }) : [];
}

export type HistoryPage = { total: number; start: number; messages: HistoryMessage[] };

/** At most `limit` messages ending just before index `before` (the end of the chat when omitted). */
export async function loadHistoryPage(source: HistorySource, path: string, before: number | null, limit: number): Promise<HistoryPage> {
  return inTauri() ? invoke<HistoryPage>('history_load_page', { source, path, before, limit }) : { total: 0, start: 0, messages: [] };
}

/** Local clock time of an ISO timestamp, or an empty string. */
export function clock(iso: string): string {
  const date = new Date(iso);
  return Number.isNaN(date.getTime()) ? '' : date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

export function toBlocks(blocks: HistoryBlock[]): Block[] {
  return blocks.map((block) =>
    block.kind === 'command'
      ? {
          kind: 'command',
          command: block.command,
          output: block.output,
          status: block.status === 'fail' ? 'fail' : 'ok',
          // A saved file change keeps its counts after a reload; a Write is a new file.
          change: block.change ? { ...block.change, create: block.command.split(/\s+/, 1)[0] === 'Write' } : undefined,
        }
      : block,
  );
}

/** Renames the session in Claude's or Codex's own storage. Rejects with a readable message. */
export async function renameHistory(source: HistorySource, path: string, title: string): Promise<void> {
  if (inTauri()) await invoke<void>('history_rename', { source, path, title });
}

/** Moves the session to ~/.agentdeck/trash, so it disappears from Claude / Codex as well. */
export async function deleteHistory(source: HistorySource, path: string): Promise<void> {
  if (inTauri()) await invoke<void>('history_delete', { source, path });
}
