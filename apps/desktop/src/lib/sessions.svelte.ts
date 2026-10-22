import type { ZIconName, ZTreeNode } from '@shival99/z-ui-svelte';
import {
  clock,
  deleteHistory,
  inTauri,
  listHistory,
  loadHistory,
  renameHistory,
  toBlocks,
  type HistorySession,
} from './history';
import { messages as mockMessages, type Message } from './mock';
import { isLocalUrl, openInPreview } from './browser.svelte';
import { listenAgentEvents, saveAttachment, sendAgentTurn, stopAgentTurn, type AgentEvent } from './agent';
import { applyAgentEvent } from './agent-reply';
import { getPref, setPref } from './prefs';
import { modelCatalog } from './models';

// Mock workspace. Real data arrives from agentd (M2); the shapes follow crates/protocol.
export type Agent = 'claude' | 'codex' | 'gemini';
export type Status = 'working' | 'idle' | 'waiting';
/** Reasoning effort and the approval modes shared by the agent CLIs. */
export type Effort = 'low' | 'medium' | 'high' | 'max';
export type Permission = 'ask' | 'edits' | 'plan' | 'auto' | 'yolo';

export type Project = { id: string; name: string; path: string };
export type SessionItem = {
  id: string;
  projectId: string;
  title: string;
  agent: Agent;
  /** Every agent that took part when an orchestrated chat mixes several; falls back to `[agent]`. */
  agents?: Agent[];
  model: string;
  /** Git branch the session works on, if any. A conversation branch is `parentId`, not this. */
  gitBranch?: string;
  status: Status;
  /** Set on a fork: the session it was forked from. */
  parentId?: string;
  /** Imported from Claude Code / Codex history: where the transcript lives on disk. */
  source?: 'claude' | 'codex';
  path?: string;
  updatedAt?: string;
  messageCount?: number;
  /** Created in the app and still empty. */
  draft?: boolean;
  /** The agent CLI's conversation id: sent with `--session-id` on the first turn, `--resume` after. */
  agentSession?: string;
  /** The CLI already has this conversation (first turn sent): later turns resume it. */
  agentStarted?: boolean;
};

/**
 * Brand colour per agent. The OpenAI logo's path has a hard-coded `fill="#fff"`, so text colour alone
 * does not reach it: `[&_path]:fill-current` makes the path take the colour instead.
 */
export const AGENT_TEXT: Record<Agent, string> = {
  claude: 'text-orange-500',
  codex: 'text-emerald-500 [&_path]:fill-current',
  gemini: 'text-sky-500',
};

/** Brand logo per agent (z-icon brand set). */
export const AGENT_ICON: Record<Agent, ZIconName> = {
  claude: 'brandClaude' as ZIconName,
  codex: 'brandOpenai' as ZIconName,
  gemini: 'brandGemini' as ZIconName,
};

export const AGENTS: { value: Agent; label: string }[] = [
  { value: 'claude', label: 'Claude' },
  { value: 'codex', label: 'Codex' },
  { value: 'gemini', label: 'Gemini' },
];

/** Models per agent, from the catalogue in models.ts. */
export const MODELS = modelCatalog();

const mockWorkspace = {
  activeId: 's1',
  projects: [
    { id: 'p1', name: 'agentdeck', path: 'E:\\WORK\\PROJECTS\\agentdeck' },
    { id: 'p2', name: 'z-ui', path: 'E:\\WORK\\PROJECTS\\z-ui' },
    { id: 'p3', name: 'msg-new-template', path: 'E:\\WORK\\PROJECTS\\msg-new-template' },
  ] as Project[],
  sessions: [
    { id: 's1', projectId: 'p1', title: 'Build the settings panel', agent: 'claude', model: 'claude-sonnet-5-5', gitBranch: 'main', status: 'working' },
    { id: 's2', projectId: 'p1', title: 'Try it with Codex', agent: 'codex', agents: ['claude', 'codex'], model: 'gpt-6-luna', gitBranch: 'main', status: 'waiting', parentId: 's1' },
    { id: 's3', projectId: 'p1', title: 'Codex, second attempt', agent: 'codex', model: 'gpt-5.6-luna', gitBranch: 'main', status: 'idle', parentId: 's2' },
    { id: 's4', projectId: 'p1', title: 'Virtual chat list', agent: 'claude', agents: ['claude', 'codex', 'gemini'], model: 'claude-opus-5-5', gitBranch: 'feat/chat-virtual', status: 'idle' },
    { id: 's5', projectId: 'p2', title: 'Svelte 5 z-tabs styling', agent: 'claude', model: 'claude-sonnet-5-5', gitBranch: 'main', status: 'idle' },
    { id: 's6', projectId: 'p2', title: 'Review the carousel API', agent: 'gemini', model: 'gemini-3-pro', gitBranch: 'feat/carousel', status: 'idle' },
    { id: 's7', projectId: 'p3', title: 'Zoom check on mobile table', agent: 'claude', model: 'claude-haiku-5-5', gitBranch: 'fix/mobile-zoom', status: 'waiting' },
  ] as SessionItem[],
};

// In the desktop app start empty and show the real history; the mock only serves plain-browser dev.
export const workspace = $state(
  inTauri() ? { activeId: '', projects: [] as Project[], sessions: [] as SessionItem[] } : mockWorkspace,
);

export const activeSession = () => workspace.sessions.find((item) => item.id === workspace.activeId);
export const projectOf = (session: SessionItem) => workspace.projects.find((item) => item.id === session.projectId);

export type Mention = { kind: 'session' | 'project' | 'ai'; id: string; label: string };

/** What was typed but not sent, per session: switch away and back and it is still there. */
export const drafts = $state<Record<string, string>>({});
export const draftMentions = $state<Record<string, Mention[]>>({});

/** An image waiting to be sent: a data URL, already scaled down. */
export type Attachment = { id: string; name: string; mime: string; src: string };
export const draftImages = $state<Record<string, Attachment[]>>({});

/** A part of a web page picked in the AgentDeck browser, waiting to go with the next message. */
export type PageElement = { id: string; tag: string; text: string; selector: string; html: string; box: { x: number; y: number; w: number; h: number }; style: Record<string, string>; url: string; title: string };
export const draftElements = $state<Record<string, PageElement[]>>({});
/** Messages of sessions that live only in this app (new chats), by session id. */
const localMessages: Record<string, Message[]> = {};

/** Sessions that only exist in the app (drafts, new chats, live agent chats) and their messages. */
function localSessions(): SessionItem[] {
  return workspace.sessions.filter((item) => !item.path && (item.draft || localMessages[item.id]));
}

/** Save the local sessions, their messages and the open session to IndexedDB. Call after any change. */
export function persistLocal() {
  const sessions = localSessions();
  const messages = Object.fromEntries(sessions.map((item) => [item.id, localMessages[item.id] ?? []]));
  // Plain copies: the store holds proxies that IndexedDB cannot store.
  setPref('local.state', JSON.parse(JSON.stringify({ sessions, messages, activeId: workspace.activeId })));
}

/** Read the saved local sessions back. Runs once at start-up, before the history import merges them. */
const localReady = getPref<{ sessions: SessionItem[]; messages: Record<string, Message[]>; activeId?: string }>('local.state').then(
  (saved) => {
    if (!saved) return;
    for (const item of saved.sessions ?? []) {
      if (!workspace.sessions.some((existing) => existing.id === item.id)) workspace.sessions.push(item);
    }
    // A reply that was still running when the app closed is not running any more.
    for (const [id, list] of Object.entries(saved.messages ?? {})) localMessages[id] = list.map((message) => ({ ...message, live: false }));
    if (saved.activeId && !workspace.activeId) workspace.activeId = saved.activeId;
  },
  () => {
    // Nothing saved, or no IndexedDB: start empty.
  },
);

// The open session is saved on its own, so switching sessions survives a reload even without a send.
const activeReady = getPref<string>('workspace.activeId').then(
  (id) => id ?? '',
  () => '',
);
// Set the open session only after its local messages are back, so the chat opens with them.
const restored = Promise.all([localReady, activeReady]).then(([, id]) => {
  if (id && !workspace.activeId) workspace.activeId = id;
});
$effect.root(() => {
  $effect(() => {
    const id = workspace.activeId;
    if (id) setPref('workspace.activeId', id);
  });
});
const mockIds = new Set(mockWorkspace.sessions.map((item) => item.id));

/** Sessions that belong to no project: a plain chat with the computer. */
export const NO_PROJECT = 'none';
const nextId = () => `local-${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`;

function ensureNoProject() {
  if (!workspace.projects.some((item) => item.id === NO_PROJECT)) {
    workspace.projects.unshift({ id: NO_PROJECT, name: 'Chats', path: '' });
  }
}

/**
 * Like Codex: one untouched "New session" is reused instead of piling up empty ones, so pressing the
 * button from another session brings you back to the draft you left, input included. Only once a
 * message has been sent does the button start a new one.
 */
export function newSession(projectId = activeSession()?.projectId ?? workspace.projects[0]?.id ?? NO_PROJECT, { reuseDraft = true } = {}) {
  if (projectId === NO_PROJECT) ensureNoProject();
  const empty = reuseDraft ? workspace.sessions.find((item) => item.draft) : undefined;
  if (empty) {
    empty.projectId = projectId;
    workspace.activeId = empty.id;
    return;
  }
  const base = workspace.sessions.find((item) => item.projectId === projectId);
  const uuid = crypto.randomUUID();
  const id = `claude:${uuid}`;
  workspace.sessions.push({
    id,
    projectId,
    title: 'New session',
    agent: 'claude',
    agentSession: uuid,
    model: 'claude-sonnet-5-5',
    gitBranch: projectId === NO_PROJECT ? undefined : base?.gitBranch,
    status: 'idle',
    draft: true,
  });
  workspace.activeId = id;
  persistLocal();
}

/** A new chat outside any project. */
export function newChat() {
  newSession(NO_PROJECT);
}

/** The last thing asked in each chat: "open it", "run the app" decides whether a served page opens by itself. */
const lastPrompt: Record<string, string> = {};
const OPEN_WORDS = /(open|opened|preview|browser|run|running|start|started|serve)|mở|chạy|xem thử/i;

/** After a turn: if you asked to open or run something and a local address came out of it, show it in the AgentDeck browser. */
function openServedPage(sessionId: string, reply: Message) {
  if (!inTauri()) return;
  const text = reply.blocks
    .map((block) => (block.kind === 'response' ? block.text : block.kind === 'command' ? `${block.command}\n${block.output}` : ''))
    .join('\n');
  // You asked for it, or the answer says the page is open or running.
  if (!OPEN_WORDS.test(lastPrompt[sessionId] ?? '') && !OPEN_WORDS.test(text)) return;
  const found = text.match(/https?:\/\/(?:localhost|127\.0\.0\.1|\[::1\])(?::\d+)?[^\s`'")\]>]*/gi)?.find(isLocalUrl);
  if (found) openInPreview(found);
}

/** The picked page elements as text the agent can use: where they are, what they look like, their markup. */
function withElements(text: string, elements: PageElement[]): string {
  if (elements.length === 0) return text;
  const parts = elements.map((item, index) =>
    [
      `Element ${index + 1} on ${item.title || 'the page'} (${item.url}): \`${item.selector}\` ${item.box.w}×${item.box.h}`,
      `Text: ${item.text || '(none)'}`,
      `Styles: ${Object.entries(item.style).map(([key, value]) => `${key}: ${value}`).join('; ')}`,
      '```html',
      item.html,
      '```',
    ].join('\n'),
  );
  return `${text || 'Look at the element(s) below.'}\n\n${parts.join('\n\n')}`;
}

/** How the Composer's permission setting maps onto the CLI's `--permission-mode`; "ask" passes none. */
const PERMISSION_MODE: Record<Permission, string | null> = { ask: null, edits: 'acceptEdits', plan: 'plan', auto: 'auto', yolo: 'bypassPermissions' };
/** The permission setting of the Composer, read when a turn starts. */
export const turnOptions = $state<{ permission: Permission; effort: Effort; /** What Enter does while a reply is running. */ whileRunning: 'queue' | 'interrupt' }>({
  permission: 'ask',
  effort: 'medium',
  whileRunning: 'queue',
});
void getPref<'queue' | 'interrupt'>('composer.whileRunning').then((saved) => {
  if (saved === 'queue' || saved === 'interrupt') turnOptions.whileRunning = saved;
});
export function setWhileRunning(mode: 'queue' | 'interrupt') {
  turnOptions.whileRunning = mode;
  setPref('composer.whileRunning', mode);
}

/** The run of each chat that is current: events of an older (stopped) run are ignored. */
const turns: Record<string, string> = {};

/** Messages typed while the agent was still busy, waiting their turn. */
export type Queued = { text: string; images: Attachment[]; elements: PageElement[] };
export const queued = $state<Record<string, Queued[]>>({});
/** Whether a chat has a reply in progress (for the send / stop button). */
export const isRunning = (sessionId: string) => workspace.sessions.find((item) => item.id === sessionId)?.status === 'working';

/** Stops the reply of a chat now. What was written so far stays. */
export function stopTurn(sessionId: string) {
  if (!replies[sessionId]) {
    // Nothing is running any more but the chat still says so (a lost event): clear it.
    const stuck = workspace.sessions.find((item) => item.id === sessionId);
    if (stuck?.status === 'working') stuck.status = 'idle';
    return;
  }
  const turn = turns[sessionId];
  void stopAgentTurn(sessionId).catch(() => {});
  handleAgentEvent(sessionId, { kind: 'error', message: 'stopped' }, turn);
}

/** Tools the user approved with "Always allow", per chat. */
const approved: Record<string, string[]> = {};
void getPref<Record<string, string[]>>('permissions.allowed').then((saved) => {
  if (saved) Object.assign(approved, saved);
});

/** One `claude -p` run in `session`, with a fresh live reply. Images go to disk first so the CLI can open them. */
function startTurn(session: SessionItem, text: string, images: Attachment[] = [], allow: string[] = []) {
  if (images.length || !allow.length) lastPrompt[session.id] = text;
  const cliId = session.agentSession ?? (session.id.startsWith('claude:') ? session.id.slice('claude:'.length) : crypto.randomUUID());
  session.agentSession = cliId;
  const resume = session.agentStarted ?? !!session.path;
  const turn = nextId();
  turns[session.id] = turn;
  const reply: Message = { id: nextId(), role: 'assistant', time: clock(new Date().toISOString()), at: new Date().toISOString(), blocks: [], live: true, startedAt: Date.now() };
  const list = messagesOf(session.id);
  list.push(reply);
  liveReplies[session.id] = list[list.length - 1]; // the stored (reactive) object
  replies[session.id] = reply.id;
  session.status = 'working';
  const cwd = workspace.projects.find((project) => project.id === session.projectId)?.path ?? null;
  void (async () => {
    const paths = await Promise.all(images.map((image) => saveAttachment(image.mime, image.src.slice(image.src.indexOf(',') + 1))));
    const prompt = paths.length
      ? `${text || 'Look at the attached image.'}\n\nAttached image${paths.length > 1 ? 's' : ''} (open with the Read tool):\n${paths.map((path) => `- ${path}`).join('\n')}`
      : text;
    await sendAgentTurn({
      session: session.id,
      turn,
      agent: session.agent,
      prompt,
      agentSession: cliId,
      resume,
      model: session.model ?? null,
      permissionMode: PERMISSION_MODE[turnOptions.permission],
      effort: turnOptions.effort,
      allowedTools: [...new Set([...(approved[session.id] ?? []), ...allow])],
      cwd,
    });
  })().catch((error: unknown) => {
    handleAgentEvent(session.id, { kind: 'error', message: String(error) }, turn);
  });
}

/**
 * The user's answer to an approval card. "Allow" and "Always allow" let the CLI run the refused tools and
 * continue the work in a new turn; "Deny" just closes the card.
 */
export function answerPermission(messageId: string, blockIndex: number, answer: 'once' | 'always' | 'deny') {
  const session = activeSession();
  const block = chat.messages.find((message) => message.id === messageId)?.blocks[blockIndex];
  if (!session || block?.kind !== 'permission' || block.state !== 'pending' || replies[session.id]) return;
  if (answer === 'deny') {
    block.state = 'denied';
    persistLocal();
    return;
  }
  block.state = 'allowed';
  const entries = block.requests.map((request) => request.allow);
  if (answer === 'always') {
    approved[session.id] = [...new Set([...(approved[session.id] ?? []), ...entries])];
    setPref('permissions.allowed', JSON.parse(JSON.stringify(approved)));
  }
  persistLocal();
  startTurn(session, 'Permission granted. Please continue with what you were doing.', [], entries);
}

/** Adds the typed message to the open session. The first message turns a draft into a real session. */
/** The assistant message each live turn is writing into, per session. */
const replies: Record<string, string> = {};
/** The reply object of each running turn. */
const liveReplies: Record<string, Message> = {};

/** Messages of a session: the live list for the active one, the remembered list for the others. */
function messagesOf(sessionId: string): Message[] {
  if (sessionId === workspace.activeId) {
    if (chat.messages === mockMessages) chat.messages = [...mockMessages];
    return chat.messages;
  }
  return (localMessages[sessionId] ??= []);
}

/** Apply one agent event to the reply it belongs to. */
export function handleAgentEvent(sessionId: string, event: AgentEvent, turn?: string) {
  if (turn !== undefined && turns[sessionId] !== turn) return; // a run that was stopped or replaced
  const session = workspace.sessions.find((item) => item.id === sessionId);
  // The live reply is kept by reference: the chat list can be swapped under it (another chat opened, a transcript
  // reloaded) and the turn must still finish, or the chat would spin for ever.
  const reply = liveReplies[sessionId] ?? messagesOf(sessionId).find((item) => item.id === replies[sessionId]);
  if (!session) return;
  if (!reply) {
    if (event.kind === 'done' || event.kind === 'error') {
      delete replies[sessionId];
      session.status = 'idle';
    }
    return;
  }
  const update = applyAgentEvent(reply.blocks, event);
  reply.blocks = update.blocks;
  // The CLI has the conversation once it answers; only then is `--resume` valid for the next turn.
  if (event.kind !== 'error') session.agentStarted = true;
  if (update.agentSession) session.agentSession = update.agentSession;
  if (update.finished) {
    reply.live = false;
    if (reply.startedAt) reply.ms = Date.now() - reply.startedAt;
    openServedPage(sessionId, reply);
    delete replies[sessionId];
    delete liveReplies[sessionId];
    session.status = 'idle';
    persistLocal();
    // The CLI wrote the conversation to disk: pick it up so the row gets its path, in place (same id).
    if (!session.path) void importHistory({ refresh: true });
    // Messages typed while it worked go next, one at a time.
    const next = queued[sessionId]?.shift();
    if (next) setTimeout(() => send(sessionId, next.text, next.images, next.elements), 0);
  }
}

if (inTauri()) {
  // A fast stream sends dozens of deltas a second; each one re-rendered the chat. Apply them once per frame.
  let queue: { session: string; turn: string; event: AgentEvent }[] = [];
  const flush = () => {
    const batch = queue;
    queue = [];
    for (const { session, turn, event } of batch) handleAgentEvent(session, event, turn);
  };
  void listenAgentEvents((envelope) => {
    if (queue.push(envelope) === 1) requestAnimationFrame(flush);
  });
}

/** Send from the message box. While the agent is busy: wait in line, or stop it and go first, as chosen. */
export function sendMessage(text: string, images: Attachment[] = [], elements: PageElement[] = []) {
  const session = activeSession();
  if (!session || (!text.trim() && images.length === 0 && elements.length === 0)) return;
  if (replies[session.id]) {
    if (turnOptions.whileRunning === 'queue') {
      (queued[session.id] ??= []).push({ text, images, elements });
      return;
    }
    stopTurn(session.id);
  }
  send(session.id, text, images, elements);
}

function send(sessionId: string, text: string, images: Attachment[], elements: PageElement[]) {
  const session = workspace.sessions.find((item) => item.id === sessionId);
  const body = text.trim();
  if (!session || replies[sessionId]) return;
  const messages = messagesOf(sessionId);
  messages.push({
    id: nextId(),
    role: 'user',
    time: clock(new Date().toISOString()),
    blocks: [
      ...images.map((image) => ({ kind: 'image' as const, src: image.src, name: image.name })),
      ...elements.map((item) => ({ kind: 'element' as const, tag: item.tag, text: item.text, selector: item.selector })),
      ...(body ? [{ kind: 'response' as const, text: body }] : []),
    ],
    at: new Date().toISOString(),
  });
  if (inTauri() && session.agent === 'claude') startTurn(session, withElements(body, elements), images);
  else if (inTauri()) {
    messages.push({
      id: nextId(),
      role: 'assistant',
      time: clock(new Date().toISOString()),
      blocks: [{ kind: 'response', text: `⚠ ${session.agent} chats are not connected yet. Only Claude runs from here for now.` }],
    });
  }
  // Imported sessions reload from disk; sessions that only exist here keep their messages in memory.
  if (!session.path) localMessages[session.id] = messages;
  persistLocal();
  if (session.draft) {
    session.draft = false;
    const firstLine = body.split('\n')[0] || 'Image';
    session.title = firstLine.length > 60 ? `${firstLine.slice(0, 57)}…` : firstLine;
  }
}

/** A fork keeps the project and branch of its parent and nests under it in the tree. */
// Leaving or reloading the page keeps the open session and the local chats.
if (typeof window !== 'undefined') {
  window.addEventListener('pagehide', () => persistLocal());
}

export function forkSession(sourceId: string) {
  const source = workspace.sessions.find((item) => item.id === sourceId);
  if (!source) return;
  const id = nextId();
  workspace.sessions.push({ ...source, id, title: `${source.title} (fork)`, status: 'idle', parentId: source.id });
  workspace.activeId = id;
}

// ---------- tree: Project > Session > Fork ----------
// A fork is a branch of the conversation (not a git branch): it nests under the session it came from.
export type TreeData = { kind: 'project' | 'session'; session?: SessionItem; path?: string };

export function buildTree(): ZTreeNode<TreeData>[] {
  // A session with no time yet is new: it goes first.
  const newest = (a: SessionItem, b: SessionItem) => {
    if (!a.updatedAt || !b.updatedAt) return Number(!b.updatedAt) - Number(!a.updatedAt);
    return b.updatedAt.localeCompare(a.updatedAt);
  };
  const forksOf = (parent: SessionItem): ZTreeNode<TreeData>[] =>
    workspace.sessions
      .filter((item) => item.parentId === parent.id)
      .sort(newest)
      .map(sessionNode);

  const sessionNode = (session: SessionItem): ZTreeNode<TreeData> => ({
    id: `session:${session.id}`,
    label: session.title,
    icon: session.parentId ? 'lucideGitFork' : 'lucideMessageSquare',
    children: forksOf(session),
    data: { kind: 'session', session },
  });

  return workspace.projects.map((project) => ({
    id: `project:${project.id}`,
    label: project.name,
    icon: 'lucideFolder',
    data: { kind: 'project', path: project.path },
    children: workspace.sessions
      .filter((item) => item.projectId === project.id && !item.parentId)
      .sort(newest)
      .map(sessionNode),
  }));
}

// ---------- real history (Claude Code and Codex, read-only) ----------
export const chat = $state({ messages: mockMessages as Message[], loading: false, error: '' });
export const history = $state({ imported: false, loading: inTauri() });

const CACHE_KEY = 'agentdeck.history.index';

/** Last folder of a path, for either separator: `C:\work\demo` and `/home/me/demo` both give `demo`. */
const folderName = (cwd: string) => cwd.split(/[\\/]+/).filter(Boolean).at(-1) ?? 'Unknown';

/** Replaces the mock workspace with what Claude Code and Codex already saved on this machine. */
export async function importHistory({ refresh = false } = {}) {
  if (!inTauri()) {
    history.loading = false;
    return;
  }
  await restored;
  // Show the last known index at once, then refresh it: indexing every file takes a moment.
  if (!refresh) try {
    const cached = JSON.parse(localStorage.getItem(CACHE_KEY) ?? '[]') as HistorySession[];
    if (cached.length > 0) {
      applyHistory(cached);
      history.imported = true;
    }
  } catch {
    // A bad cache is simply ignored.
  }
  history.loading = true;
  try {
    const found = await listHistory();
    if (found.length === 0) return;
    const keep = workspace.activeId;
    applyHistory(found);
    if (found.some((item) => item.id === keep)) workspace.activeId = keep;
    history.imported = true;
    // Quietly warm the transcripts of the newest chats while the app is idle.
    const warm = () => workspace.sessions.filter((item) => item.path).sort((a, b) => (b.updatedAt ?? '').localeCompare(a.updatedAt ?? '')).slice(0, 4).forEach((item) => void prefetchTranscript(item));
    if ('requestIdleCallback' in window) window.requestIdleCallback(warm, { timeout: 3000 });
    else setTimeout(warm, 800);
    try {
      localStorage.setItem(CACHE_KEY, JSON.stringify(found));
    } catch {
      // Quota or private mode: the cache is only an optimisation.
    }
  } catch (error) {
    chat.error = String(error);
  } finally {
    history.loading = false;
  }
}

function applyHistory(all: HistorySession[]) {
  // One entry per id (first = newest): an older cache can still hold a duplicated Codex thread,
  // and a repeated id breaks the sidebar's keyed tree.
  const seen = new Set<string>();
  const found = all.filter((item) => !seen.has(item.id) && seen.add(item.id));
  const ids = new Set(found.map((item) => item.id));
  const projectIds = new Map<string, string>();
  const projects: Project[] = [];
  for (const item of found) {
    if (!projectIds.has(item.cwd)) {
      const id = `dir:${item.cwd}`;
      projectIds.set(item.cwd, id);
      projects.push({ id, name: item.cwd ? folderName(item.cwd) : 'Unknown', path: item.cwd });
    }
  }
  const sessions: SessionItem[] = found.map((item) => ({
    id: item.id,
    projectId: projectIds.get(item.cwd)!,
    title: item.title,
    agent: item.source,
    // Sonnet unless the chat was changed: the first model of the list is the biggest one.
    model: item.source === 'claude' ? 'claude-sonnet-5-5' : MODELS[item.source][0].value,
    status: 'idle',
    parentId: item.parent_id && ids.has(item.parent_id) ? item.parent_id : undefined,
    source: item.source,
    path: item.path,
    updatedAt: item.updated_at ?? undefined,
    messageCount: item.message_count,
  }));
  // Most recently used project first.
  const latest = (projectId: string) =>
    sessions.filter((item) => item.projectId === projectId).reduce((max, item) => (item.updatedAt ?? '') > max ? item.updatedAt ?? '' : max, '');
  projects.sort((a, b) => latest(b.id).localeCompare(latest(a.id)));

  // Sessions that only exist here (drafts, new chats) survive the import.
  const local = localSessions().filter((item) => !ids.has(item.id));
  const noProject = workspace.projects.find((item) => item.id === NO_PROJECT);
  // The open chat stays open: after F5 it is an imported session, after a send it may still be a local one.
  const keepActive = ids.has(workspace.activeId) || local.some((item) => item.id === workspace.activeId) ? workspace.activeId : undefined;
  if (noProject && local.some((item) => item.projectId === NO_PROJECT)) projects.unshift(noProject);
  sessions.sort((a, b) => (b.updatedAt ?? '').localeCompare(a.updatedAt ?? ''));

  workspace.projects = projects;
  workspace.sessions = [...sessions, ...local.filter((item) => projects.some((project) => project.id === item.projectId))];
  workspace.activeId = keepActive ?? sessions[0].id;
}

let transcriptToken = 0;

// ---------- transcripts: memory + IndexedDB cache, so opening a chat is instant ----------
// A transcript is valid while the session's index entry (last update + message count) is unchanged.
type Transcript = { stamp: string; messages: Message[] };
const transcripts = new Map<string, Transcript>();
const stampOf = (session: SessionItem) => `${session.updatedAt ?? ''}|${session.messageCount ?? 0}`;
const MAX_SAVED = 12;

async function fetchTranscript(session: SessionItem): Promise<Message[]> {
  const loaded = await loadHistory(session.source!, session.path!);
  return loaded.map((message) => ({ id: message.id, role: message.role, time: clock(message.time), at: message.time || undefined, blocks: toBlocks(message.blocks) }));
}

function remember(session: SessionItem, messages: Message[]) {
  const entry = { stamp: stampOf(session), messages };
  transcripts.set(session.id, entry);
  // The most recently opened few also survive a reload (F5 shows the chat at once).
  void getPref<string[]>('transcripts.keys').then((keys = []) => {
    const next = [session.id, ...keys.filter((key) => key !== session.id)];
    for (const old of next.slice(MAX_SAVED)) setPref(`transcript.${old}`, null);
    setPref('transcripts.keys', next.slice(0, MAX_SAVED));
    setPref(`transcript.${session.id}`, JSON.parse(JSON.stringify(entry)));
  });
}

/** Warms the cache for a chat the user is likely to open next (row hover, most recent ones). */
const prefetching = new Set<string>();
export async function prefetchTranscript(session: SessionItem | undefined) {
  if (!session?.path || !session.source || transcripts.get(session.id)?.stamp === stampOf(session) || prefetching.has(session.id)) return;
  prefetching.add(session.id);
  try {
    remember(session, await fetchTranscript(session));
  } catch {
    // Opening the chat will report the error.
  } finally {
    prefetching.delete(session.id);
  }
}

/** Fills the chat with the transcript of a session: from cache when it is current, from disk otherwise. */
export async function openTranscript(session: SessionItem | undefined) {
  const token = ++transcriptToken;
  chat.error = '';
  if (!session) {
    chat.messages = [];
    return;
  }
  if (!session.path || !session.source) {
    chat.messages = localMessages[session.id] ?? (mockIds.has(session.id) ? mockMessages : []);
    return;
  }
  let cached = transcripts.get(session.id);
  if (!cached) {
    const saved = await getPref<Transcript>(`transcript.${session.id}`);
    if (token !== transcriptToken) return;
    if (saved?.messages) transcripts.set(session.id, (cached = saved));
  }
  if (cached) {
    // Show what we have right away; a stale copy is replaced as soon as the fresh one arrives.
    chat.messages = cached.messages;
    if (cached.stamp === stampOf(session)) return;
  } else {
    chat.loading = true;
  }
  try {
    const fresh = await fetchTranscript(session);
    if (token !== transcriptToken) return;
    chat.messages = fresh;
    remember(session, fresh);
  } catch (error) {
    if (token !== transcriptToken) return;
    if (!cached) chat.messages = [];
    chat.error = String(error);
  } finally {
    if (token === transcriptToken) chat.loading = false;
  }
}

/** The cached index mirrors the files on disk, so a rename or delete has to reach it too. */
function patchCache(change: (list: HistorySession[]) => HistorySession[]) {
  try {
    const list = JSON.parse(localStorage.getItem(CACHE_KEY) ?? '[]') as HistorySession[];
    localStorage.setItem(CACHE_KEY, JSON.stringify(change(list)));
  } catch {
    // Only an optimisation.
  }
}

/** Renames the chat; for Claude and Codex sessions the name is written into the tool's own storage. */
export async function renameSession(id: string, title: string) {
  const session = workspace.sessions.find((item) => item.id === id);
  const next = title.trim();
  if (!session || !next) return;
  if (session.path && session.source) await renameHistory(session.source, session.path, next);
  session.title = next;
  patchCache((list) => list.map((item) => (item.id === id ? { ...item, title: next } : item)));
}

/** Deletes the chat; Claude and Codex sessions are moved to ~/.agentdeck/trash, so they vanish there too. */
export async function deleteSession(id: string) {
  const session = workspace.sessions.find((item) => item.id === id);
  if (!session) return;
  if (session.path && session.source) await deleteHistory(session.source, session.path);
  // Forks of it move up to its own parent.
  for (const child of workspace.sessions.filter((item) => item.parentId === id)) child.parentId = session.parentId;
  workspace.sessions = workspace.sessions.filter((item) => item.id !== id);
  delete drafts[id];
  delete draftMentions[id];
  delete draftImages[id];
  delete draftElements[id];
  delete localMessages[id];
  patchCache((list) => list.filter((item) => item.id !== id));
  if (workspace.activeId === id) {
    const next = workspace.sessions.find((item) => item.projectId === session.projectId) ?? workspace.sessions[0];
    if (next) workspace.activeId = next.id;
    else newSession(session.projectId);
  }
}
