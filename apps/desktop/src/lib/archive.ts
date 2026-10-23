import { getPref, listPrefs, setPref } from './prefs';
import type { Message } from './mock';

// AgentDeck's own copy of every conversation turn: which agent and model wrote it, when, and what it said.
// The agent CLIs keep their own history in their own formats; this is one format of ours, kept in IndexedDB, and
// the single place a sync (to a GitHub repo, a Gist, a folder…) reads from and writes to later. A sync target
// only has to implement `SyncTarget`; nothing else in the app needs to know where the data goes.
//
// Records are keyed by chat and message id, so saving the same turn again replaces it instead of piling up.
// Each carries `savedAt`; a target remembers the last `savedAt` it pushed and gets only what changed after that.

export const ARCHIVE_VERSION = 1;

export type ArchiveSession = {
  v: typeof ARCHIVE_VERSION;
  id: string;
  title: string;
  agent: string;
  model: string | null;
  projectPath: string | null;
  savedAt: string;
};

export type ArchiveMessage = {
  v: typeof ARCHIVE_VERSION;
  id: string;
  sessionId: string;
  role: 'user' | 'assistant';
  /** ISO time of the message. */
  at: string | null;
  /** Who wrote an assistant message, and on which model. */
  agent: string | null;
  model: string | null;
  /** Answer text only; tool output stays in the CLI's own history. */
  text: string;
  tools: number;
  tokens: { input: number; output: number; cacheRead: number; cacheWrite: number } | null;
  cost: number | null;
  ms: number | null;
  savedAt: string;
};

/** What moves between devices: everything saved after `from` (exclusive), up to `to`. */
export type ArchiveBatch = {
  version: typeof ARCHIVE_VERSION;
  from: string | null;
  to: string;
  sessions: ArchiveSession[];
  messages: ArchiveMessage[];
};

/** A place the archive can be copied to (and read back from). */
export interface SyncTarget {
  id: string;
  label: string;
  push(batch: ArchiveBatch): Promise<void>;
  /** Everything the target holds that was saved after `since`. Optional: a target may be write-only. */
  pull?(since: string | null): Promise<ArchiveBatch>;
}

const KEY_SESSION = (id: string) => `archive.s.${id}`;
const KEY_MESSAGE = (session: string, id: string) => `archive.m.${session}.${id}`;
const KEY_PUSHED = (target: string) => `archivecursor.push.${target}`;
const KEY_PULLED = (target: string) => `archivecursor.pull.${target}`;

const textOf = (message: Message) =>
  message.blocks
    .filter((block) => block.kind === 'response')
    .map((block) => (block as { text: string }).text)
    .join('\n\n')
    .trim();

/** Saves the messages of a finished turn, and the chat they belong to. */
export function archiveTurn(
  session: { id: string; title: string; agent: string; model: string | null | undefined; projectPath: string | null },
  messages: Message[],
) {
  const savedAt = new Date().toISOString();
  setPref(KEY_SESSION(session.id), {
    v: ARCHIVE_VERSION,
    id: session.id,
    title: session.title,
    agent: session.agent,
    model: session.model ?? null,
    projectPath: session.projectPath,
    savedAt,
  } satisfies ArchiveSession);
  for (const message of messages) {
    const record: ArchiveMessage = {
      v: ARCHIVE_VERSION,
      id: message.id,
      sessionId: session.id,
      role: message.role,
      at: message.at ?? null,
      agent: message.role === 'assistant' ? (message.agent ?? session.agent) : null,
      model: message.role === 'assistant' ? (message.model ?? session.model ?? null) : null,
      text: textOf(message),
      tools: message.blocks.filter((block) => block.kind === 'command').length,
      tokens: message.tokens ? { ...message.tokens } : null,
      cost: message.cost ?? null,
      ms: message.ms ?? null,
      savedAt,
    };
    setPref(KEY_MESSAGE(session.id, message.id), record);
  }
}

/** Everything saved after `since` (all of it when `null`). */
export async function exportArchive(since: string | null = null): Promise<ArchiveBatch> {
  const [sessions, messages] = await Promise.all([listPrefs<ArchiveSession>('archive.s.'), listPrefs<ArchiveMessage>('archive.m.')]);
  const newer = <T extends { savedAt: string }>(items: [string, T][]) =>
    items.map(([, item]) => item).filter((item) => since === null || item.savedAt > since);
  const picked = { sessions: newer(sessions), messages: newer(messages) };
  const stamps = [...picked.sessions, ...picked.messages].map((item) => item.savedAt).sort();
  return { version: ARCHIVE_VERSION, from: since, to: stamps.at(-1) ?? since ?? new Date().toISOString(), ...picked };
}

/** Merges a batch from elsewhere: a record is replaced only by a newer copy of itself. */
export async function importArchive(batch: ArchiveBatch) {
  if (batch.version !== ARCHIVE_VERSION) throw new Error(`archive version ${batch.version} is not supported`);
  const merge = async <T extends { savedAt: string }>(key: string, incoming: T) => {
    const have = await getPref<T>(key);
    if (!have || have.savedAt < incoming.savedAt) setPref(key, incoming);
  };
  await Promise.all([
    ...batch.sessions.map((item) => merge(KEY_SESSION(item.id), item)),
    ...batch.messages.map((item) => merge(KEY_MESSAGE(item.sessionId, item.id), item)),
  ]);
}

const targets = new Map<string, SyncTarget>();
export const registerSyncTarget = (target: SyncTarget) => void targets.set(target.id, target);
export const syncTargets = () => [...targets.values()];

/** Sends what changed since the last push to a target, then moves its cursor. Returns how many messages went. */
export async function pushArchive(targetId: string): Promise<number> {
  const target = targets.get(targetId);
  if (!target) throw new Error(`no sync target "${targetId}"`);
  const since = (await getPref<string>(KEY_PUSHED(targetId))) ?? null;
  const batch = await exportArchive(since);
  if (batch.messages.length === 0 && batch.sessions.length === 0) return 0;
  await target.push(batch);
  setPref(KEY_PUSHED(targetId), batch.to);
  return batch.messages.length;
}

/** Reads what a target holds that is newer than the last pull and merges it in. Returns how many messages came. */
export async function pullArchive(targetId: string): Promise<number> {
  const target = targets.get(targetId);
  if (!target?.pull) throw new Error(`"${targetId}" cannot be read back`);
  const since = (await getPref<string>(KEY_PULLED(targetId))) ?? null;
  const batch = await target.pull(since);
  await importArchive(batch);
  setPref(KEY_PULLED(targetId), batch.to);
  return batch.messages.length;
}
