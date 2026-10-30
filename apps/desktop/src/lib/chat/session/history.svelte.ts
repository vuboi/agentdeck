import { messages as mockMessages, type Message, type Block } from '../mock';
import { loadHistory, inTauri, type HistorySession, listHistory, type HistoryMessage, clock, toBlocks, loadHistoryPage, renameHistory, deleteHistory } from '../../app/platform/history';
import { restoreCommandImages, stripCommandImageData, deleteCommandImages } from '../content/command-images';
import { samePath } from '../../files/paths';
import { getPref, setPref, setPrefs } from '../../app/platform/prefs';
import { splitAttachments, attachmentBlocks } from '../content/attachments';
import { isHandbackPrompt, stripLead } from '../runs/handback';
import { joinContinuations } from '../runs/continuations';
import { MODELS, NO_PROJECT, addedProjects, addedReady, draftElements, draftFiles, draftImages, draftMentions, drafts, isEmptyDraft, localMessages, localSessions, mockIds, newSession, noProjectRow, parents, rememberParents, restored, workspace, type Project, type SessionItem } from './state.svelte';
import { liveAsks, liveReplies, messagesOf, replies, sessionLanes, withLive } from './run.svelte';
import { hidden, signature } from './sending.svelte';
import { lastPrompt, queuePaused, queued, stopTurn, turns } from './turns.svelte';
import { lastEventAt, lastEventKind, recoveries, resumes } from './events.svelte';
import { backlog } from './delegate-task.svelte';

// ---------- real history (Claude Code and Codex, read-only) ----------
export const chat = $state({
  messages: mockMessages as Message[],
  loading: false,
  error: '',
  older: 0,
  loadingOlder: false,
});

/** Returns a complete, display-ready conversation without changing which chat is open. */
export async function conversationForExport(sessionId: string): Promise<{ title: string; messages: Message[]; sourcePath?: string }> {
  const session = workspace.sessions.find((item) => item.id === sessionId);
  if (!session) throw new Error('This conversation no longer exists.');
  if (session.source && session.path) {
    const raw = await loadHistory(session.source, session.path);
    const own = await toMessages(session, raw);
    const all = assemble(session, own, own[0]?.at);
    return {
      title: session.title,
      messages: await restoreCommandImages(session.id, all),
      sourcePath: session.path,
    };
  }
  const messages = messagesOf(sessionId);
  return { title: session.title, messages: JSON.parse(JSON.stringify(messages)) as Message[] };
}

export const history = $state({ imported: false, loading: inTauri() });

// Older indexes treated every cwd as a project; never paint those classifications during startup.
/** The chat history index, kept in IndexedDB (prefs.ts `index.` collection). */
const HISTORY_INDEX = 'index.history';

/** Last folder of a path, for either separator: `C:\work\demo` and `/home/me/demo` both give `demo`. */
export const folderName = (cwd: string) =>
  cwd
    .split(/[\\/]+/)
    .filter(Boolean)
    .at(-1) ?? 'Unknown';

/** Replaces the mock workspace with what Claude Code and Codex already saved on this machine. */
export async function importHistory({ refresh = false } = {}) {
  if (!inTauri()) {
    history.loading = false;
    return;
  }
  await Promise.all([restored, addedReady]);
  // Show the last known index at once, then refresh it: indexing every file takes a moment.
  if (!refresh)
    try {
      const cached = (await getPref<HistorySession[]>(HISTORY_INDEX)) ?? [];
      if (cached.length > 0) {
        // A stale cache must not replace the restored active id with its first row. That used to make the
        // subsequent full import preserve the wrong session even when it found the session the user had open.
        const activeId = workspace.activeId;
        const hasActive = cached.some((item) => item.id === activeId);
        const hasLocalActive = localSessions().some((item) => item.id === activeId);
        if (!activeId || hasActive || hasLocalActive) {
          applyHistory(cached);
          history.imported = true;
        }
      }
    } catch {
      // A bad cache is simply ignored.
    }
  history.loading = true;
  try {
    const found = await listHistory();
    const keep = workspace.activeId;
    const keepIsLocal = localSessions().some((item) => item.id === keep);
    if (found.length === 0 && (!keep || keepIsLocal)) return;
    applyHistory(found);
    if (found.some((item) => item.id === keep)) workspace.activeId = keep;
    else if (keep && !keepIsLocal) newSession(NO_PROJECT);
    history.imported = true;
    // Quietly warm the transcripts of the newest chats while the app is idle.
    const warm = () =>
      workspace.sessions
        .filter((item) => item.path)
        .sort((a, b) => (b.updatedAt ?? '').localeCompare(a.updatedAt ?? ''))
        .slice(0, 4)
        .forEach((item) => void prefetchTranscript(item));
    if ('requestIdleCallback' in window)
      window.requestIdleCallback(warm, { timeout: 3000 });
    else setTimeout(warm, 800);
    try {
      // The index lives in IndexedDB (no size limit like localStorage): the next start shows it at once.
      void setPref(HISTORY_INDEX, JSON.parse(JSON.stringify(found)));
    } catch {
      // Only an optimisation.
    }
  } catch (error) {
    chat.error = String(error);
  } finally {
    history.loading = false;
  }
}

/** The parent remembered for a sub chat, when that parent still exists (in the history, or as a chat of this app). */
function knownParent(id: string, ids: Set<string>): string | undefined {
  const parent = parents[id];
  if (!parent || parent === id) return undefined;
  return ids.has(parent) ||
    workspace.sessions.some((item) => item.id === parent)
    ? parent
    : undefined;
}

function applyHistory(all: HistorySession[]) {
  // The sub chats that exist now keep their parent through this import.
  rememberParents();
  // One entry per id (first = newest): an older cache can still hold a duplicated Codex thread,
  // and a repeated id breaks the sidebar's keyed tree.
  const seen = new Set<string>();
  const found = all.filter((item) => !seen.has(item.id) && seen.add(item.id));
  const ids = new Set(found.map((item) => item.id));
  const projectIds = new Map<string, string>();
  const projects: Project[] = [];
  for (const item of found) {
    if (!item.is_project && !addedProjects.some((path) => samePath(path, item.cwd))) continue;
    if (!projectIds.has(item.cwd)) {
      const id = `dir:${item.cwd}`;
      projectIds.set(item.cwd, id);
      projects.push({
        id,
        name: item.cwd ? folderName(item.cwd) : 'Unknown',
        path: item.cwd,
      });
    }
  }
  const sessions: SessionItem[] = found.map((item) => ({
    // A saved chat continued with another agent: which agents it has had, and their own conversation ids.
    ...(mixed[item.id] ?? {}),
    id: item.id,
    projectId: projectIds.get(item.cwd) ?? NO_PROJECT,
    workingDirectory: item.cwd || undefined,
    title: item.title,
    agent: mixed[item.id]?.agents?.at(-1) ?? item.source,
    // Sonnet unless the chat was changed: the first model of the list is the biggest one.
    model:
      item.model ??
      (item.source === 'claude'
        ? 'claude-sonnet-5-5'
        : MODELS[item.source][0].value),
    conversationModel: item.model ?? undefined,
    status: 'idle',
    parentId:
      item.parent_id && ids.has(item.parent_id)
        ? item.parent_id
        : knownParent(item.id, ids),
    source: item.source,
    path: item.path,
    updatedAt: item.updated_at ?? undefined,
    messageCount: item.message_count,
  }));
  // Most recently used project first.
  const latest = (projectId: string) =>
    sessions
      .filter((item) => item.projectId === projectId)
      .reduce(
        (max, item) =>
          (item.updatedAt ?? '') > max ? (item.updatedAt ?? '') : max,
        '',
      );
  projects.sort((a, b) => latest(b.id).localeCompare(latest(a.id)));

  // Sessions that only exist here (drafts, new chats) survive the import.
  const local = localSessions().filter((item) => !ids.has(item.id));
  // The open empty draft keeps its project too, so its row stays highlighted.
  const draft = workspace.sessions.find(
    (item) => item.id === workspace.activeId && isEmptyDraft(item),
  );
  if (draft) local.push(draft);
  // The open chat stays open: after F5 it is an imported session, after a send it may still be a local one.
  const keepActive =
    ids.has(workspace.activeId) ||
    local.some((item) => item.id === workspace.activeId)
      ? workspace.activeId
      : undefined;
  // Folders added by hand stay even with no chat in them yet, right after "This computer".
  const added = addedProjects
    .filter((path) => !projects.some((project) => samePath(project.path, path)))
    .map((path) => ({ id: `dir:${path}`, name: folderName(path), path }));
  projects.unshift(noProjectRow(), ...added);
  sessions.sort((a, b) => (b.updatedAt ?? '').localeCompare(a.updatedAt ?? ''));

  // A chat already listed keeps its object, updated in place: a turn running in it (in this chat, or on another
  // chat's import) still holds that object and its "working" state. A new object with `status: 'idle'` dropped both,
  // so the chat looked done while its agent was still answering.
  const current = new Map(workspace.sessions.map((item) => [item.id, item]));
  const merged = sessions.map((item) => {
    const existing = current.get(item.id);
    if (!existing) return item;
    return Object.assign(existing, item, { status: existing.status });
  });

  // Local chats saved under the old cwd-based projects survive reclassification too.
  for (const item of local) {
    if (projects.some((project) => project.id === item.projectId)) continue;
    item.workingDirectory ??= workspace.projects.find((project) => project.id === item.projectId)?.path;
    item.projectId = NO_PROJECT;
  }
  workspace.projects = projects;
  workspace.sessions = [
    ...merged,
    ...local.filter((item) =>
      projects.some((project) => project.id === item.projectId),
    ),
  ];
  workspace.activeId = keepActive ?? sessions[0]?.id ?? '';
}

let transcriptToken = 0;

// ---------- transcripts: memory + IndexedDB cache, so opening a chat is instant ----------
// A transcript is valid while the session's index entry (last update + message count) is unchanged.
// `start` is where the first loaded message sits in the chat's file: above 0, older messages are still on disk.
type Transcript = { stamp: string; messages: Message[]; start: number };
export const transcripts = new Map<string, Transcript>();
// The format version leads the stamp: v5 limits persisted transcript pages to 30 messages.
const stampOf = (session: SessionItem) =>
  `v5|${session.updatedAt ?? ''}|${session.messageCount ?? 0}`;
const MAX_SAVED = 12;
/** Keep one page in the reload cache; older pages are fetched from the history cursor on demand. */
const PAGE_LIMIT = 20;
const SAVED_TAIL = PAGE_LIMIT;

/** Agents and per-agent ids of saved chats that switched agent (the chat files cannot hold them). */
const mixed: Record<string, Pick<SessionItem, 'agents' | 'agentIds'>> = {};
void getPref<typeof mixed>('chat.mixed').then((saved) => {
  if (saved) Object.assign(mixed, saved);
});
export function rememberMixed(session: SessionItem) {
  if (!session.path || !session.agents || session.agents.length < 2) return;
  mixed[session.id] = $state.snapshot({
    agents: session.agents,
    agentIds: session.agentIds,
  });
  setPref('chat.mixed', mixed);
}

/**
 * Turns of another agent in a saved chat (a Claude chat continued with Codex, say). The chat's file only holds
 * its own agent's turns, so these are kept here and merged back in by time when the chat is read again.
 */
export const otherTurns: Record<string, Message[]> = {};
void getPref<Record<string, Message[]>>('chat.otherTurns').then((saved) => {
  if (saved) Object.assign(otherTurns, saved);
});
export function keepOtherTurn(session: SessionItem, reply: Message) {
  const list = messagesOf(session.id);
  const index = list.indexOf(reply);
  const asked =
    index > 0 && list[index - 1].role === 'user' ? [list[index - 1]] : [];
  const kept = (otherTurns[session.id] ??= []);
  for (const message of [...asked, reply]) {
    if (!kept.some((item) => item.id === message.id))
      kept.push($state.snapshot(message) as Message);
  }
  saveOtherTurns();
}

function saveOtherTurns() {
  const saved = Object.fromEntries(
    Object.entries(otherTurns).map(([id, list]) => [
      id,
      stripCommandImageData($state.snapshot(list) as Message[]),
    ]),
  );
  setPref('chat.otherTurns', saved);
}

/**
 * A permission card was answered on screen: the copies of its message that are read back when the chat is reopened
 * (the kept turns and the transcript cache) must say so too, or the card asks again after a switch to another chat.
 */
export function settleKeptPermission(
  sessionId: string,
  messageId: string,
  blockIndex: number,
  state: 'allowed' | 'denied' | 'expired',
) {
  const settle = (list: Message[] | undefined) => {
    const block = list?.find((message) => message.id === messageId)?.blocks[blockIndex];
    if (block?.kind !== 'permission' || block.state === state) return false;
    block.state = state;
    return true;
  };
  settle(transcripts.get(sessionId)?.messages);
  if (settle(otherTurns[sessionId])) saveOtherTurns();
}

/**
 * The message that handed work out (a flow's steps, `@agent`) is never in the chat's file: the chat's agent only
 * speaks at the handback. Kept with the other turns, and saved again as its cards change, so the chat still shows
 * its Flow steps card once it is read back from the file.
 */
export function keepHandout(mainId: string, asked: Message) {
  const kept = (otherTurns[mainId] ??= []);
  const copy = $state.snapshot(asked) as Message;
  const index = kept.findIndex((item) => item.id === asked.id);
  if (index >= 0) kept[index] = copy;
  else kept.push(copy);
  const saved = Object.fromEntries(
    Object.entries(otherTurns).map(([id, list]) => [
      id,
      stripCommandImageData($state.snapshot(list) as Message[]),
    ]),
  );
  setPref('chat.otherTurns', saved);
}

/** Pictures and files a saved prompt named by path (see `splitAttachments`), as blocks; the words lose those lists. */
async function userAttachments(message: HistoryMessage): Promise<Block[]> {
  const found: { images: string[]; files: string[] } = {
    images: [],
    files: [],
  };
  for (const block of message.blocks) {
    if (block.kind !== 'response' || !block.text.includes('Attached '))
      continue;
    const split = splitAttachments(block.text);
    block.text = split.text;
    found.images.push(...split.images);
    found.files.push(...split.files);
  }
  return found.images.length || found.files.length
    ? attachmentBlocks(found.images, found.files)
    : [];
}

async function toMessages(
  session: SessionItem,
  all: HistoryMessage[],
): Promise<Message[]> {
  // The prompt that handed other agents' answers back was never a message of the user's: only the reply to it is shown.
  const loaded = all.filter(
    (message) =>
      !(
        message.role === 'user' &&
        message.blocks.some(
          (block) => block.kind === 'response' && isHandbackPrompt(block.text),
        )
      ),
  );
  const restored = await Promise.all(
    loaded.map((message) =>
      message.role === 'user'
        ? userAttachments(message)
        : Promise.resolve([] as Block[]),
    ),
  );
  return joinContinuations(
    loaded.map((message, at) => ({
      ...(message.role === 'assistant' ? { agent: session.source } : {}),
      id: message.id,
      role: message.role,
      time: clock(message.time),
      at: message.time || undefined,
      // A prompt sent with the turns this agent missed shows only what the user typed.
      blocks: [
        ...restored[at],
        ...toBlocks(message.blocks)
          .map((block) => {
            // The paragraph AgentDeck puts before a prompt (how to open pages, how to delegate) is not the user's text.
            const typed =
              message.role === 'user' && block.kind === 'response'
                ? { ...block, text: stripLead(block.text) }
                : block;
            return typed.kind === 'response' &&
              message.role === 'user' &&
              typed.text.startsWith(
                'Earlier in this chat, other agents answered',
              )
              ? {
                  ...typed,
                  text: typed.text.slice(
                    typed.text.indexOf('Now the user says:\n') +
                      'Now the user says:\n'.length,
                  ),
                }
              : typed;
          })
          .flatMap((block) =>
            block.kind === 'response' && !block.text ? [] : [block],
          ),
      ],
      // Tokens come from the transcript itself, so every reply shows them again when the chat is reopened.
      ...(message.usage
        ? {
            tokens: {
              input: message.usage.input,
              output: message.usage.output,
              cacheRead: message.usage.cache_read,
              cacheWrite: message.usage.cache_write,
            },
            model: message.model,
          }
        : {}),
    })),
  );
}

/** The file's messages minus the deleted ones, with the other agents' turns that fall in the time range `[lower, upper)`. */
function assemble(
  session: SessionItem,
  own: Message[],
  lower?: string,
  upper?: string,
): Message[] {
  const others = (otherTurns[session.id] ?? []).filter(
    (message) =>
      (!lower || (message.at ?? '') >= lower) &&
      (!upper || (message.at ?? '') < upper),
  );
  const gone = new Set(hidden[session.id] ?? []);
  const out = (message: Message) =>
    gone.has(message.id) || gone.has(signature(message));
  const shown = gone.size ? own.filter((message) => !out(message)) : own;
  if (!others.length) return shown;
  return [...shown, ...others.filter((message) => !out(message))].sort((a, b) =>
    (a.at ?? '').localeCompare(b.at ?? ''),
  );
}

async function fetchTranscript(
  session: SessionItem,
): Promise<{ messages: Message[]; start: number }> {
  const page = await loadHistoryPage(
    session.source!,
    session.path!,
    null,
    PAGE_LIMIT,
  );
  const own = await toMessages(session, page.messages);
  const assembled = assemble(
    session,
    own,
    page.start > 0 ? own[0]?.at : undefined,
  );
  const messages = assembled.slice(-PAGE_LIMIT);
  const removedBefore =
    messages.length < assembled.length
      ? page.messages.filter(
          (message) => message.time < (messages[0]?.at ?? ''),
        ).length
      : 0;
  return {
    messages: await restoreCommandImages(session.id, messages),
    start: page.start + removedBefore,
  };
}

/** Reads the page of messages before the first one on screen and puts it in front. Returns how many it added. */
export async function loadOlder(
  session: SessionItem | undefined,
): Promise<number> {
  if (!session?.path || !session.source || chat.older <= 0 || chat.loadingOlder)
    return 0;
  chat.loadingOlder = true;
  try {
    const page = await loadHistoryPage(
      session.source,
      session.path,
      chat.older,
      PAGE_LIMIT,
    );
    if (workspace.activeId !== session.id) return 0;
    const own = await toMessages(session, page.messages);
    const added = await restoreCommandImages(
      session.id,
      assemble(
        session,
        own,
        page.start > 0 ? own[0]?.at : undefined,
        chat.messages[0]?.at,
      ),
    );
    // Another chat may have been opened while the page was read: it must not get this chat's older messages.
    if (workspace.activeId !== session.id) return 0;
    chat.messages = [...added, ...chat.messages];
    chat.older = page.start;
    const cached = transcripts.get(session.id);
    if (cached)
      transcripts.set(session.id, {
        ...cached,
        messages: [...added, ...cached.messages],
        start: page.start,
      });
    return added.length;
  } catch (error) {
    chat.error = String(error);
    return 0;
  } finally {
    chat.loadingOlder = false;
  }
}

function remember(session: SessionItem, messages: Message[], start: number) {
  const entry = { stamp: stampOf(session), messages, start };
  transcripts.set(session.id, entry);
  // The most recently opened few also survive a reload (F5 shows the chat at once).
  // Cloning a long chat is slow, so it waits for an idle moment and keeps only the newest page: F5 then shows
  // the end at once, and a fresh page is read from the history file right after.
  const persist = () =>
    void getPref<string[]>('transcripts.keys').then((keys = []) => {
      const next = [session.id, ...keys.filter((key) => key !== session.id)];
      const tail = {
        stamp: '',
        messages: stripCommandImageData(messages.slice(-SAVED_TAIL)),
        start: start + Math.max(0, messages.length - SAVED_TAIL),
      };
      // The key list, the new tail and the removal of the chats that fell out go in one batch (one transaction).
      setPrefs([
        ...next.slice(MAX_SAVED).map((old): [string, null] => [`transcript.${old}`, null]),
        ['transcripts.keys', next.slice(0, MAX_SAVED)],
        [`transcript.${session.id}`, tail],
      ]);
    });
  if (typeof requestIdleCallback === 'function')
    requestIdleCallback(persist, { timeout: 3000 });
  else setTimeout(persist, 500);
}

/** Warms the cache for a chat the user is likely to open next (row hover, most recent ones). */
const prefetching = new Set<string>();
export async function prefetchTranscript(session: SessionItem | undefined) {
  if (
    !session?.path ||
    !session.source ||
    transcripts.get(session.id)?.stamp === stampOf(session) ||
    prefetching.has(session.id)
  )
    return;
  prefetching.add(session.id);
  try {
    const fresh = await fetchTranscript(session);
    remember(session, fresh.messages, fresh.start);
  } catch {
    // Opening the chat will report the error.
  } finally {
    prefetching.delete(session.id);
  }
}

let transcriptDone: Promise<void> = Promise.resolve();
/** Settles when the open chat's transcript is complete, the fresh read included: a saved copy can be older than the chat. */
export function transcriptSettled(): Promise<void> {
  return transcriptDone;
}

/** Fills the chat with the transcript of a session: from cache when it is current, from disk otherwise. */
export function openTranscript(session: SessionItem | undefined): Promise<void> {
  let finish = () => {};
  transcriptDone = new Promise<void>((resolve) => (finish = resolve));
  return readTranscript(session).finally(finish);
}

async function readTranscript(session: SessionItem | undefined) {
  const token = ++transcriptToken;
  chat.error = '';
  chat.older = 0;
  if (!session) {
    chat.messages = [];
    chat.loading = false;
    return;
  }
  if (!session.path || !session.source) {
    // A read of the chat before may still be loading: its result is dropped, so it can no longer clear the flag.
    chat.loading = false;
    const messages = await restoreCommandImages(
      session.id,
      localMessages[session.id] ??
        (mockIds.has(session.id) ? mockMessages : []),
    );
    if (token === transcriptToken) chat.messages = messages;
    return;
  }
  // Show the empty chat with its skeleton first and let it paint: reading the saved copy and mounting a long chat
  // takes a moment on the main thread, and the window must not look frozen meanwhile.
  chat.messages = [];
  chat.loading = true;
  await new Promise<void>((resolve) =>
    requestAnimationFrame(() => setTimeout(resolve, 0)),
  );
  if (token !== transcriptToken) return;
  let cached = transcripts.get(session.id);
  if (!cached) {
    const saved = await getPref<Transcript>(`transcript.${session.id}`);
    if (token !== transcriptToken) return;
    if (saved?.messages) {
      // Older versions kept up to 200 messages in IndexedDB. Bound this first paint too, before the fresh page arrives.
      const removed = Math.max(0, saved.messages.length - PAGE_LIMIT);
      cached = removed
        ? {
            ...saved,
            messages: saved.messages.slice(-PAGE_LIMIT),
            start: saved.start + removed,
          }
        : saved;
      transcripts.set(session.id, cached);
    }
  }
  if (cached) {
    // Show what we have right away; a stale copy is replaced as soon as the fresh one arrives.
    chat.messages = await restoreCommandImages(
      session.id,
      withLive(session.id, cached.messages),
    );
    if (token !== transcriptToken) return;
    chat.older = cached.stamp === stampOf(session) ? (cached.start ?? 0) : 0;
    chat.loading = false;
    if (cached.stamp === stampOf(session)) return;
  } else {
    chat.loading = true;
  }
  try {
    const fresh = await fetchTranscript(session);
    if (token !== transcriptToken) return;
    chat.messages = withLive(session.id, fresh.messages);
    chat.older = fresh.start;
    remember(session, fresh.messages, fresh.start);
  } catch (error) {
    if (token !== transcriptToken) return;
    if (!cached) chat.messages = [];
    chat.error = String(error);
  } finally {
    if (token === transcriptToken) chat.loading = false;
  }
}

/** The cached index mirrors the files on disk, so a rename or delete has to reach it too. */
async function patchCache(change: (list: HistorySession[]) => HistorySession[]) {
  try {
    const list = (await getPref<HistorySession[]>(HISTORY_INDEX)) ?? [];
    void setPref(HISTORY_INDEX, change(list));
  } catch {
    // Only an optimisation.
  }
}

/** Renames the chat; for Claude and Codex sessions the name is written into the tool's own storage. */
export async function renameSession(id: string, title: string) {
  const session = workspace.sessions.find((item) => item.id === id);
  const next = title.trim();
  if (!session || !next) return;
  if (session.path && session.source)
    await renameHistory(session.source, session.path, next);
  session.title = next;
  patchCache((list) =>
    list.map((item) => (item.id === id ? { ...item, title: next } : item)),
  );
}

/** Deletes the chat; Claude and Codex sessions are moved to ~/.agentdeck/trash, so they vanish there too. */
export async function deleteSession(id: string) {
  const session = workspace.sessions.find((item) => item.id === id);
  if (!session) return;
  // A run of the chat ends with it: its CLI is stopped and the events that still arrive have nowhere to go.
  stopTurn(id);
  if (session.path && session.source)
    await deleteHistory(session.source, session.path);
  // Forks of it move up to its own parent.
  for (const child of workspace.sessions.filter((item) => item.parentId === id))
    child.parentId = session.parentId;
  workspace.sessions = workspace.sessions.filter((item) => item.id !== id);
  delete drafts[id];
  delete draftMentions[id];
  delete draftImages[id];
  delete draftFiles[id];
  delete draftElements[id];
  delete localMessages[id];
  // What the chat kept in memory for its runs, queue and recoveries: nothing reads it again, and it would only grow.
  delete queued[id];
  delete queuePaused[id];
  delete turns[id];
  delete lastPrompt[id];
  delete replies[id];
  delete liveReplies[id];
  delete liveAsks[id];
  delete sessionLanes[id];
  delete recoveries[id];
  delete resumes[id];
  delete otherTurns[id];
  delete hidden[id];
  transcripts.delete(id);
  backlog.delete(id);
  lastEventAt.delete(id);
  lastEventKind.delete(id);
  saveOtherTurns();
  deleteCommandImages(id);
  patchCache((list) => list.filter((item) => item.id !== id));
  if (workspace.activeId === id) {
    const next =
      workspace.sessions.find((item) => item.projectId === session.projectId) ??
      workspace.sessions[0];
    if (next) workspace.activeId = next.id;
    else newSession(session.projectId);
  }
}
