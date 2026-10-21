// Client side of the read-only history readers in crates/adapters (Tauri commands `history_list`
// and `history_load`). Outside Tauri (plain browser dev) there is no disk access: lists are empty.
import type { Block } from './mock';

export type HistorySource = 'claude' | 'codex';

export type HistorySession = {
  id: string;
  source: HistorySource;
  title: string;
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
  | { kind: 'command'; command: string; output: string; status: string };

export type HistoryMessage = { id: string; role: 'user' | 'assistant'; time: string; blocks: HistoryBlock[] };

export const inTauri = () => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

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

/** Local clock time of an ISO timestamp, or an empty string. */
export function clock(iso: string): string {
  const date = new Date(iso);
  return Number.isNaN(date.getTime()) ? '' : date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

export function toBlocks(blocks: HistoryBlock[]): Block[] {
  return blocks.map((block) =>
    block.kind === 'command'
      ? { kind: 'command', command: block.command, output: block.output, status: block.status === 'fail' ? 'fail' : 'ok' }
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
