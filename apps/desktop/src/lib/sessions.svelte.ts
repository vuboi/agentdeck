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

export const MODELS: Record<Agent, { value: string; label: string }[]> = {
  claude: [
    { value: 'opus-5-5', label: 'Opus 5.5' },
    { value: 'sonnet-5-5', label: 'Sonnet 5.5' },
    { value: 'haiku-5-5', label: 'Haiku 5.5' },
  ],
  codex: [
    { value: 'gpt-6-luna', label: 'GPT-6 Luna' },
    { value: 'gpt-5-6-luna', label: 'GPT-5.6 Luna' },
  ],
  gemini: [
    { value: 'gemini-3-pro', label: 'Gemini 3 Pro' },
    { value: 'gemini-3-flash', label: 'Gemini 3 Flash' },
  ],
};

const mockWorkspace = {
  activeId: 's1',
  projects: [
    { id: 'p1', name: 'agentdeck', path: 'E:\\WORK\\PROJECTS\\agentdeck' },
    { id: 'p2', name: 'z-ui', path: 'E:\\WORK\\PROJECTS\\z-ui' },
    { id: 'p3', name: 'msg-new-template', path: 'E:\\WORK\\PROJECTS\\msg-new-template' },
  ] as Project[],
  sessions: [
    { id: 's1', projectId: 'p1', title: 'Build the settings panel', agent: 'claude', model: 'sonnet-5-5', gitBranch: 'main', status: 'working' },
    { id: 's2', projectId: 'p1', title: 'Try it with Codex', agent: 'codex', agents: ['claude', 'codex'], model: 'gpt-6-luna', gitBranch: 'main', status: 'waiting', parentId: 's1' },
    { id: 's3', projectId: 'p1', title: 'Codex, second attempt', agent: 'codex', model: 'gpt-5-6-luna', gitBranch: 'main', status: 'idle', parentId: 's2' },
    { id: 's4', projectId: 'p1', title: 'Virtual chat list', agent: 'claude', agents: ['claude', 'codex', 'gemini'], model: 'opus-5-5', gitBranch: 'feat/chat-virtual', status: 'idle' },
    { id: 's5', projectId: 'p2', title: 'Svelte 5 z-tabs styling', agent: 'claude', model: 'sonnet-5-5', gitBranch: 'main', status: 'idle' },
    { id: 's6', projectId: 'p2', title: 'Review the carousel API', agent: 'gemini', model: 'gemini-3-pro', gitBranch: 'feat/carousel', status: 'idle' },
    { id: 's7', projectId: 'p3', title: 'Zoom check on mobile table', agent: 'claude', model: 'haiku-5-5', gitBranch: 'fix/mobile-zoom', status: 'waiting' },
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
/** Messages of sessions that live only in this app (new chats), by session id. */
const localMessages: Record<string, Message[]> = {};
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
export function newSession(projectId = activeSession()?.projectId ?? workspace.projects[0]?.id ?? NO_PROJECT) {
  if (projectId === NO_PROJECT) ensureNoProject();
  const empty = workspace.sessions.find((item) => item.draft);
  if (empty) {
    empty.projectId = projectId;
    workspace.activeId = empty.id;
    return;
  }
  const base = workspace.sessions.find((item) => item.projectId === projectId);
  const id = nextId();
  workspace.sessions.push({
    id,
    projectId,
    title: 'New session',
    agent: 'claude',
    model: 'sonnet-5-5',
    gitBranch: projectId === NO_PROJECT ? undefined : base?.gitBranch,
    status: 'idle',
    draft: true,
  });
  workspace.activeId = id;
}

/** A new chat outside any project. */
export function newChat() {
  newSession(NO_PROJECT);
}

/** Adds the typed message to the open session. The first message turns a draft into a real session. */
export function sendMessage(text: string) {
  const session = activeSession();
  const body = text.trim();
  if (!session || !body) return;
  if (chat.messages === mockMessages) chat.messages = [...mockMessages];
  chat.messages.push({
    id: nextId(),
    role: 'user',
    time: clock(new Date().toISOString()),
    blocks: [{ kind: 'response', text: body }],
  });
  // Imported sessions reload from disk; sessions that only exist here keep their messages in memory.
  if (!session.path) localMessages[session.id] = chat.messages;
  if (session.draft) {
    session.draft = false;
    const firstLine = body.split('\n')[0];
    session.title = firstLine.length > 60 ? `${firstLine.slice(0, 57)}…` : firstLine;
  }
}

/** A fork keeps the project and branch of its parent and nests under it in the tree. */
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
  const newest = (a: SessionItem, b: SessionItem) => (b.updatedAt ?? '').localeCompare(a.updatedAt ?? '');
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
export async function importHistory() {
  if (!inTauri()) {
    history.loading = false;
    return;
  }
  // Show the last known index at once, then refresh it: indexing every file takes a moment.
  try {
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

function applyHistory(found: HistorySession[]) {
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
    model: MODELS[item.source][0].value,
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
  const local = workspace.sessions.filter((item) => !item.path && (item.draft || localMessages[item.id]));
  const noProject = workspace.projects.find((item) => item.id === NO_PROJECT);
  const keepActive = local.some((item) => item.id === workspace.activeId) ? workspace.activeId : undefined;
  if (noProject && local.some((item) => item.projectId === NO_PROJECT)) projects.unshift(noProject);
  sessions.sort((a, b) => (b.updatedAt ?? '').localeCompare(a.updatedAt ?? ''));

  workspace.projects = projects;
  workspace.sessions = [...sessions, ...local.filter((item) => projects.some((project) => project.id === item.projectId))];
  workspace.activeId = keepActive ?? sessions[0].id;
}

let transcriptToken = 0;

/** Fills the chat with the transcript of a session: from disk for imported ones, mock otherwise. */
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
  chat.loading = true;
  try {
    const loaded = await loadHistory(session.source, session.path);
    if (token !== transcriptToken) return;
    chat.messages = loaded.map((message) => ({
      id: message.id,
      role: message.role,
      time: clock(message.time),
      blocks: toBlocks(message.blocks),
    }));
  } catch (error) {
    if (token !== transcriptToken) return;
    chat.messages = [];
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
  delete localMessages[id];
  patchCache((list) => list.filter((item) => item.id !== id));
  if (workspace.activeId === id) {
    const next = workspace.sessions.find((item) => item.projectId === session.projectId) ?? workspace.sessions[0];
    if (next) workspace.activeId = next.id;
    else newSession(session.projectId);
  }
}
