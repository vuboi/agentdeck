import { archiveTurn } from './archive';
import { expandSlash } from './slash.svelte';
import type { ZIconName, ZTreeNode } from '@shival99/z-ui-svelte';
import {
  clock,
  deleteHistory,
  inTauri,
  listHistory,
  loadHistoryPage,
  renameHistory,
  toBlocks,
  type HistoryMessage,
  type HistorySession,
} from './history';
import { messages as mockMessages, type Block, type Message } from './mock';
import { openInPreview } from './browser.svelte';
import { trackActivity } from './activity.svelte';
import { logTalk } from './activity.svelte';
import { parseQuestions, QUESTION_RULE } from './questions';
import { listenAgentEvents, saveAttachment, sendAgentTurn, sendCustomTurn, stopAgentTurn, type AgentEvent } from './agent';
import { applyAgentEvent } from './agent-reply';
import { getPref, setPref } from './prefs';
import { estimateCost, recordUsage } from './usage';
import { customProviderName, modelCatalog, modelLabel, parseCustomModel } from './models.svelte';
import { connectors } from './connectors.svelte';
import { dequeue, enqueue } from './queue';
import { inputsOf, normalizeGraph, presetOf, stepGroups, type WorkflowPreset, type WorkflowStep } from './workflows.svelte';

// Mock workspace. Real data arrives from agentd (M2); the shapes follow crates/protocol.
export type Agent = 'claude' | 'codex' | 'antigravity' | 'custom';
export type Status = 'working' | 'idle' | 'waiting';
/** Reasoning effort and the approval modes shared by the agent CLIs. */
export type Effort = 'off' | 'low' | 'medium' | 'high' | 'xhigh' | 'max' | 'ultra';
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
  /** Effort a workflow step asks for; it replaces the chat's own level for this chat's turns. */
  stepEffort?: string;
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
  /** Model used by the current CLI conversation; changing it starts a fresh conversation. */
  conversationModel?: string;
  /** Model used by each additional agent conversation in a mixed chat. */
  agentModels?: Partial<Record<Agent, string>>;
  /** Per agent: its own conversation id and whether it has one yet. `agentSession` / `agentStarted` belong to
   *  the chat's first agent; an agent the chat moved to later keeps its ids here. */
  agentIds?: Partial<Record<Agent, { id: string; started: boolean }>>;
};

/**
 * Brand colour per agent. The OpenAI logo's path has a hard-coded `fill="#fff"`, so text colour alone
 * does not reach it: `[&_path]:fill-current` makes the path take the colour instead.
 */
export const AGENT_TEXT: Record<Agent, string> = {
  claude: 'text-orange-500',
  codex: 'text-emerald-500 [&_path]:fill-current',
  antigravity: 'text-sky-500',
  custom: 'text-violet-500',
};

/** Brand logo per agent (z-icon brand set). */
export const AGENT_ICON: Record<Agent, ZIconName> = {
  claude: 'brandClaude' as ZIconName,
  codex: 'brandOpenai' as ZIconName,
  antigravity: 'brandGemini' as ZIconName,
  custom: 'lucideServer' as ZIconName,
};

export const AGENTS: { value: Agent; label: string }[] = [
  { value: 'claude', label: 'Claude' },
  { value: 'codex', label: 'Codex' },
  { value: 'antigravity', label: 'Antigravity' },
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
    { id: 's4', projectId: 'p1', title: 'Virtual chat list', agent: 'claude', agents: ['claude', 'codex', 'antigravity'], model: 'claude-opus-5-5', gitBranch: 'feat/chat-virtual', status: 'idle' },
    { id: 's5', projectId: 'p2', title: 'Svelte 5 z-tabs styling', agent: 'claude', model: 'claude-sonnet-5-5', gitBranch: 'main', status: 'idle' },
    { id: 's6', projectId: 'p2', title: 'Review the carousel API', agent: 'antigravity', model: 'gemini-4-argon', gitBranch: 'feat/carousel', status: 'idle' },
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
export type PageElement = { id: string; tag: string; text: string; selector: string; html: string; /** Its place among its neighbours. */ around?: string; box: { x: number; y: number; w: number; h: number }; style: Record<string, string>; url: string; title: string; /** What was written about it while marking up the page. */ note?: string };
export const draftElements = $state<Record<string, PageElement[]>>({});
/** Messages of sessions that live only in this app (new chats), by session id. */
const localMessages: Record<string, Message[]> = {};

/**
 * Which chat each sub chat belongs to (a flow step, a delegated task, a fork), by the sub chat's id. The chat files
 * do not hold it, so when the history is read again (a chat finishing, a reload) a sub chat would come back as a
 * chat of its own; this map puts it back under its parent.
 */
const parents: Record<string, string> = {};
void getPref<Record<string, string>>('chat.parents').then((saved) => {
  if (saved) Object.assign(parents, saved, { ...parents });
});
function rememberParents() {
  let changed = false;
  for (const item of workspace.sessions) {
    if (item.parentId && item.parentId !== item.id && parents[item.id] !== item.parentId) {
      parents[item.id] = item.parentId;
      changed = true;
    }
  }
  // A chat that delegated work or ran a flow holds a card for each sub chat it started: that names the parent of
  // sub chats made before this map existed (their own files, once read back, do not say).
  for (const [parentId, list] of Object.entries(localMessages)) {
    for (const message of list) {
      for (const block of message.blocks ?? []) {
        if (block.kind === 'delegate' && block.childId && block.childId !== parentId && parents[block.childId] === undefined) {
          parents[block.childId] = parentId;
          changed = true;
        }
      }
    }
  }
  if (changed) setPref('chat.parents', { ...parents });
}

/** Sessions that only exist in the app (drafts, new chats, live agent chats) and their messages. */
function localSessions(): SessionItem[] {
  return workspace.sessions.filter((item) => !item.path && (item.draft || localMessages[item.id]) && !isEmptyDraft(item));
}

/** Save the local sessions, their messages and the open session to IndexedDB. Call after any change. */
export function persistLocal() {
  rememberParents();
  const sessions = localSessions();
  const messages = Object.fromEntries(sessions.map((item) => [item.id, localMessages[item.id] ?? []]));
  // Plain copies: the store holds proxies that IndexedDB cannot store.
  setPref('local.state', JSON.parse(JSON.stringify({ sessions, messages, activeId: workspace.activeId })));
}

/** The Gemini placeholder became Antigravity: chats saved under the old name move to the new one. */
const renamedAgent = <T extends string | undefined>(agent: T): T => ((agent as string | undefined) === 'gemini' ? ('antigravity' as T) : agent);
function renamedSession(item: SessionItem): SessionItem {
  const moved = (map?: Record<string, unknown>) => (map && 'gemini' in map ? Object.fromEntries(Object.entries(map).map(([key, value]) => [key === 'gemini' ? 'antigravity' : key, value])) : map);
  return {
    ...item,
    agent: renamedAgent(item.agent),
    source: renamedAgent(item.source),
    agents: item.agents?.map((agent) => renamedAgent(agent)),
    agentIds: moved(item.agentIds) as SessionItem['agentIds'],
    agentModels: moved(item.agentModels) as SessionItem['agentModels'],
    // The placeholder models of Gemini do not exist in Antigravity: the picker chooses a real one.
    model: (item.agent as string) === 'gemini' ? '' : item.model,
  };
}

/** Read the saved local sessions back. Runs once at start-up, before the history import merges them. */
const localReady = getPref<{ sessions: SessionItem[]; messages: Record<string, Message[]>; activeId?: string }>('local.state').then(
  (saved) => {
    if (!saved) return;
    // A reply that was still running when the app closed is not running any more.
    for (const [id, list] of Object.entries(saved.messages ?? {})) localMessages[id] = list.map((message) => ({ ...message, agent: renamedAgent(message.agent), live: false }));
    for (const savedItem of saved.sessions ?? []) {
      const item = renamedSession(savedItem);
      // Empty new chats saved by an earlier version are not restored.
      if (isEmptyDraft(item)) continue;
      if (!workspace.sessions.some((existing) => existing.id === item.id)) workspace.sessions.push(item);
    }
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

/** Projects removed from the sidebar, by folder path. Their chats stay on disk; the list just leaves them out. */
export const hiddenProjects = $state<string[]>([]);
void getPref<string[]>('sidebar.hiddenProjects').then((saved) => {
  if (Array.isArray(saved)) hiddenProjects.push(...saved.filter((path) => !hiddenProjects.includes(path)));
});
export function hideProject(id: string) {
  const path = workspace.projects.find((project) => project.id === id)?.path;
  if (!path || hiddenProjects.includes(path)) return;
  hiddenProjects.push(path);
  setPref('sidebar.hiddenProjects', [...hiddenProjects]);
}
const nextId = () => `local-${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`;

/** The row for chats outside any project; always first in the list. */
const noProjectRow = (): Project => ({ id: NO_PROJECT, name: 'This computer', path: '' });
function ensureNoProject() {
  if (!workspace.projects.some((item) => item.id === NO_PROJECT)) workspace.projects.unshift(noProjectRow());
}
ensureNoProject();

/** Same folder whatever the separators, the case (Windows) or a trailing slash. */
export const samePath = (a: string, b: string) => {
  const clean = (path: string) => path.replace(/[\\/]+/g, '/').replace(/\/$/, '').toLowerCase();
  return clean(a) === clean(b);
};

/** Folders added by hand (no chat history yet, or never used by an agent) and the folders the user trusts. */
export const addedProjects = $state<string[]>([]);
export const trustedFolders = $state<string[]>([]);
const addedReady = Promise.all([
  getPref<string[]>('sidebar.addedProjects').then((saved) => Array.isArray(saved) && addedProjects.push(...saved)),
  getPref<string[]>('sidebar.trustedFolders').then((saved) => Array.isArray(saved) && trustedFolders.push(...saved)),
]).catch(() => undefined);

/** A folder agents already worked in (it has chats) counts as trusted: Claude and Codex asked about it then. */
export const isTrusted = (path: string) =>
  trustedFolders.some((item) => samePath(item, path)) || workspace.sessions.some((item) => item.path && samePath(projectOf(item)?.path ?? '', path));

export function trustFolder(path: string) {
  if (trustedFolders.some((item) => samePath(item, path))) return;
  trustedFolders.push(path);
  setPref('sidebar.trustedFolders', [...trustedFolders]);
}

/** Chats (by id) and project folders (by path) pinned to the top of the sidebar. */
export const pinned = $state<{ sessions: string[]; projects: string[] }>({ sessions: [], projects: [] });
void getPref<{ sessions: string[]; projects: string[] }>('sidebar.pinned').then((saved) => {
  if (Array.isArray(saved?.sessions)) pinned.sessions = saved.sessions;
  if (Array.isArray(saved?.projects)) pinned.projects = saved.projects;
});
const savePins = () => setPref('sidebar.pinned', { sessions: [...pinned.sessions], projects: [...pinned.projects] });
export const isPinnedSession = (id: string) => pinned.sessions.includes(id);
export const isPinnedProject = (path: string) => Boolean(path) && pinned.projects.some((item) => samePath(item, path));
export function togglePinSession(id: string) {
  pinned.sessions = isPinnedSession(id) ? pinned.sessions.filter((item) => item !== id) : [...pinned.sessions, id];
  savePins();
}
export function togglePinProject(id: string) {
  const path = workspace.projects.find((project) => project.id === id)?.path;
  if (!path) return;
  pinned.projects = isPinnedProject(path) ? pinned.projects.filter((item) => !samePath(item, path)) : [...pinned.projects, path];
  savePins();
}

/**
 * Puts a folder in the sidebar (or brings a removed one back) and opens a new session in it. The folder must be
 * trusted first; see `isTrusted` / `trustFolder`.
 */
export function addProject(path: string) {
  const hidden = hiddenProjects.findIndex((item) => samePath(item, path));
  if (hidden >= 0) {
    hiddenProjects.splice(hidden, 1);
    setPref('sidebar.hiddenProjects', [...hiddenProjects]);
  }
  let project = workspace.projects.find((item) => item.path && samePath(item.path, path));
  if (!project) {
    project = { id: `dir:${path}`, name: folderName(path), path };
    // Right under "This computer", where it is seen.
    const at = workspace.projects.findIndex((item) => item.id === NO_PROJECT) + 1;
    workspace.projects.splice(at, 0, project);
  }
  if (!addedProjects.some((item) => samePath(item, path))) {
    addedProjects.push(path);
    setPref('sidebar.addedProjects', [...addedProjects]);
  }
  newSession(project.id);
  return project;
}

/**
 * Like Codex: one untouched "New session" is reused instead of piling up empty ones, so pressing the
 * button from another session brings you back to the draft you left, input included. Only once a
 * message has been sent does the button start a new one.
 */
/** A new chat nobody wrote in is not kept: Codex does the same, so empty tabs never pile up in the list. */
function isEmptyDraft(item: SessionItem) {
  return item.draft && !(localMessages[item.id]?.length);
}
function dropEmptyDrafts() {
  for (let index = workspace.sessions.length - 1; index >= 0; index--) {
    const item = workspace.sessions[index];
    // A new session with words typed in it is kept: its project's "New session" brings it back.
    if (item.id !== workspace.activeId && isEmptyDraft(item) && !drafts[item.id]?.trim()) workspace.sessions.splice(index, 1);
  }
}

export function newSession(projectId =activeSession()?.projectId ?? workspace.projects[0]?.id ?? NO_PROJECT, { reuseDraft = true } = {}) {
  if (projectId === NO_PROJECT) ensureNoProject();
  // One unsent new session per project (or "This computer"): pressing "New session" there again returns to it,
  // typed text included; another project gets its own.
  const empty = reuseDraft ? workspace.sessions.find((item) => item.projectId === projectId && isEmptyDraft(item)) : undefined;
  if (empty) {
    workspace.activeId = empty.id;
    return;
  }
  const base = workspace.sessions.find((item) => item.projectId === projectId);
  dropEmptyDrafts();
  const uuid = crypto.randomUUID();
  const id = `claude:${uuid}`;
  const start = startingPoint();
  workspace.sessions.push({
    id,
    projectId,
    title: 'New session',
    agent: start.agent,
    agentSession: uuid,
    model: start.model,
    gitBranch: projectId === NO_PROJECT ? undefined : base?.gitBranch,
    status: 'idle',
    draft: true,
  });
  workspace.activeId = id;
  persistLocal();
}

/** Agent and model a new chat starts with: the last ones the user chatted with, else Claude's default. */
function startingPoint(): { agent: Agent; model: string } {
  if (lastChat.agent && (MODELS[lastChat.agent] ?? []).some((item) => item.value === lastChat.model)) {
    return { agent: lastChat.agent, model: lastChat.model };
  }
  return { agent: 'claude', model: 'claude-sonnet-5-5' };
}

/** A new chat outside any project. */
export function newChat() {
  newSession(NO_PROJECT);
}

/**
 * Said to the agent when a message is about running or opening something. Agents follow the project's own notes
 * (a skill that opens the user's Chrome, a shell command that starts the server), and a server started in the agent's
 * own shell dies when the command ends, which leaves the Preview on a refused connection. These tools do not.
 */
const AGENTDECK_HINT =
  '[AgentDeck] You run inside AgentDeck. To start a dev server or open a page, use the agentdeck tools: browser_serve (starts the server so it keeps running after your turn, and opens it in the Preview) or browser_open for an address that already answers. Then use browser_screenshot, browser_console and browser_network to look at it. Do not start servers in your own shell and do not open the system browser or Chrome: that server would stop when your command ends, and the user is looking at the AgentDeck Preview.' + '\n\n';

/** The last thing asked in each chat: "open it", "run the app" decides whether a served page opens by itself. */
const lastPrompt: Record<string, string> = {};
const OPEN_WORDS = /\b(open|opened|preview|browser|run|running|start|started|serve)\b|mở|chạy|xem thử/i;

/** Pick the requested framework's address when a turn starts more than one local dev server. */
function previewUrl(text: string, prompt: string): string | undefined {
  const wantsSvelte = /\bsvelte(?:\s*5)?\b|z-ui-svelte/i.test(`${prompt}\n${text}`);
  const urls = [...text.matchAll(/https?:\/\/(?:localhost|127\.0\.0\.1|\[::1\])(?::\d+)?[^\s`'")\]>]*/gi)].map((match) => ({
    url: match[0],
    at: match.index ?? 0,
  }));
  const unique = urls.filter((item, index) => urls.findIndex((other) => other.url === item.url) === index);
  return unique
    .map((item) => {
      const lineStart = text.lastIndexOf('\n', item.at) + 1;
      const lineEnd = text.indexOf('\n', item.at);
      const line = text.slice(lineStart, lineEnd < 0 ? text.length : lineEnd);
      let score = item.at;
      if (wantsSvelte) {
        if (/svelte|vite|z-ui-svelte/i.test(line)) score += 100000;
        if (/angular|4200/.test(line)) score -= 100000;
        if (/:(?:4310|4301)\b/.test(item.url)) score += 50000;
      }
      return { ...item, score };
    })
    .sort((a, b) => b.score - a.score)[0]?.url;
}

/** After a turn: if you asked to open or run something and a local address came out of it, show it in the AgentDeck browser. */
function openServedPage(sessionId: string, reply: Message) {
  if (!inTauri()) return;
  const text = reply.blocks
    .map((block) => (block.kind === 'response' ? block.text : block.kind === 'command' ? `${block.command}\n${block.output}` : ''))
    .join('\n');
  // You asked for it, or the answer says the page is open or running.
  if (!OPEN_WORDS.test(lastPrompt[sessionId] ?? '') && !OPEN_WORDS.test(text)) return;
  const found = previewUrl(text, lastPrompt[sessionId] ?? '');
  // An agent that opened the page itself (browser_open, browser_serve) has put it where it wanted: nothing to add.
  const opened = reply.blocks.some((block) => block.kind === 'command' && /browser_(open|serve)/.test(block.command));
  if (found && !opened) openInPreview(found);
}

/**
 * Words in a picked element's markup that are worth searching the project's files for: its text, ids, class names
 * and the values of its attributes. The source of a rendered element is found by what it says and is called, not by
 * its tag, so these are what a model should Grep for first.
 */
function searchHints(item: PageElement): string[] {
  const hints = new Set<string>();
  const add = (value: string | undefined | null, min = 3) => {
    const word = (value ?? '').trim();
    if (word.length >= min && word.length <= 60) hints.add(word);
  };
  add(item.text.length <= 60 ? item.text : item.text.slice(0, 40), 2);
  for (const match of item.html.matchAll(/\b(?:id|aria-label|title|placeholder|formcontrolname|name|data-[\w-]+|routerlink|ng-reflect-[\w-]+)="([^"]{2,60})"/gi)) add(match[1]);
  const generic = /^(flex|grid|block|inline|hidden|relative|absolute|items-|justify-|gap-|p[xytblr]?-|m[xytblr]?-|w-|h-|min-|max-|text-|bg-|border|rounded|shadow|font-|size-|z-|overflow|cursor|transition|hover:|focus:|disabled:|ring|outline|opacity|sr-only|truncate|shrink|grow|self-|col-|row-|space-|divide-)/;
  for (const match of item.html.matchAll(/\bclass="([^"]*)"/gi)) for (const name of match[1].split(/\s+/)) if (name && !generic.test(name) && !/^\d/.test(name)) add(name, 4);
  const custom = item.html.match(/^<([a-z]+-[a-z0-9-]+)/i)?.[1];
  add(custom, 4);
  return [...hints].slice(0, 12);
}

/** The picked page elements as text the agent can use: where they are, what they look like, their markup. */
function withElements(text: string, elements: PageElement[]): string {
  if (elements.length === 0) return text;
  const parts = elements.map((item, index) => {
    const hints = searchHints(item);
    return [
      `Element ${index + 1} on ${item.title || 'the page'} (${item.url}): \`${item.selector}\` ${item.box.w}×${item.box.h}`,
      ...(item.note ? [`Note: ${item.note}`] : []),
      `Text: ${item.text || '(none)'}`,
      `Styles: ${Object.entries(item.style).map(([key, value]) => `${key}: ${value}`).join('; ')}`,
      '```html',
      item.html,
      '```',
      ...(item.around ? ['Its place on the page:', '```', item.around, '```'] : []),
      ...(hints.length ? [`To find its source in the project, search the files for these (they come from this element's text, ids and classes): ${hints.map((hint) => `"${hint}"`).join(', ')}`] : []),
    ].join('\n');
  });
  const guide = 'These elements are what the request is about. Change the code that renders them, in the place they are rendered: find it by searching the project for the words listed, read the file around the match, and put the change exactly where the element sits (see "Its place on the page"), written like the markup around it. Do not add it anywhere else.';
  return `${text || 'Look at the element(s) below.'}\n\n${guide}\n\n${parts.join('\n\n')}`;
}

/** How the Composer's permission setting maps onto the CLI's `--permission-mode`; "ask" passes none. */
/**
 * The agent, model and effort of the last message the user actually sent. A new chat starts from them instead of a
 * fixed default. Opening a chat to read it never changes this: only sending does.
 */
export const lastChat = $state<{ agent: Agent; model: string; effort: Effort } | { agent: null }>({ agent: null });
void getPref<{ agent: Agent; model: string; effort: Effort }>('composer.lastChat').then((saved) => {
  if (saved?.agent && saved.model) Object.assign(lastChat, saved);
});
function rememberLastChat(agent: Agent, model: string | null, effort: Effort) {
  if (!model) return;
  const saved = { agent, model, effort };
  Object.assign(lastChat, saved);
  setPref('composer.lastChat', saved);
}

const PERMISSION_MODE: Record<Permission, string | null> = { ask: null, edits: 'acceptEdits', plan: 'plan', auto: 'auto', yolo: 'bypassPermissions' };
/** The permission setting of the Composer, read when a turn starts. */
export const turnOptions = $state<{ permission: Permission; effort: Effort }>({
  permission: 'auto',
  effort: 'medium',
});

/** The run of each chat that is current: events of an older (stopped) run are ignored. */
const turns: Record<string, string> = {};

/** Messages typed while the agent was still busy, waiting their turn. */
export type Queued = { text: string; images: Attachment[]; elements: PageElement[]; mentions?: Mention[] };
export const queued = $state<Record<string, Queued[]>>({});

/** Whether a chat has a reply in progress (for the send / stop button). */
export const isRunning = (sessionId: string) => workspace.sessions.find((item) => item.id === sessionId)?.status === 'working';

/** Stops the reply of a chat now. What was written so far stays. */
export function stopTurn(sessionId: string) {
  // Work handed to other agents (a flow's steps, `@agent`) stops with the chat that handed it out, all at once.
  for (const run of delegateRuns) {
    if (run.main !== sessionId || run.endedAt || run.handback === 'running') continue;
    run.cancelled = true;
    for (const node of run.nodes) {
      if (node.state !== 'running') continue;
      const waiter = stepWaiters.get(node.childId);
      // A step that waits for the user's answers has no reply to stop: it ends here.
      if (!replies[node.childId] && waiter) {
        stepWaiters.delete(node.childId);
        askingCards.delete(node.childId);
        waiter('', 'stopped', { id: nextId(), role: 'assistant', time: '', blocks: [] });
      } else stopTurn(node.childId);
    }
  }
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

/** The chat's model if its agent offers it, else that agent's first model. */
function modelFor(session: SessionItem): string | null {
  const list = MODELS[session.agent] ?? [];
  if (session.model && list.some((item) => item.value === session.model)) return session.model;
  const fallback = list[0]?.value ?? null;
  if (fallback) session.model = fallback;
  return fallback;
}

const agentLabel = (agent?: Agent, model?: string | null) => (agent === 'custom' && customProviderName(model)) || (agent === 'codex' ? 'Codex' : agent === 'antigravity' ? 'Antigravity' : agent === 'custom' ? 'Custom provider' : 'Claude');

/**
 * What the other agents of this chat said since this agent last spoke, as text to put before the prompt.
 * Each agent keeps its own conversation, so one that joins (or comes back to) a chat would not know the turns
 * it missed. `list` ends with the message being sent. The newest turns are kept when it is long.
 */
function missedTurns(session: SessionItem, list: Message[]): string {
  const before = list.slice(0, -1);
  const writer = (message: Message) => message.agent ?? session.source ?? session.agent;
  let start = 0;
  for (let index = before.length - 1; index >= 0; index--) {
    if (before[index].role === 'assistant' && writer(before[index]) === session.agent) {
      start = index + 1;
      break;
    }
  }
  const missed = before.slice(start);
  if (!missed.some((message) => message.role === 'assistant')) return '';
  const lines = missed.map((message) => {
    const text = message.blocks
      .flatMap((block) => (block.kind === 'response' ? [block.text] : block.kind === 'command' ? [`(ran: ${block.command.slice(0, 160)})`] : []))
      .join('\n')
      .trim();
    const cut = text.length > 2500 ? `${text.slice(0, 2500)}…` : text;
    return `${message.role === 'user' ? 'User' : agentLabel(writer(message), message.model)}: ${cut}`;
  });
  let digest = lines.join('\n\n');
  if (digest.length > 16000) digest = `…${digest.slice(-16000)}`;
  return `Earlier in this chat, other agents answered these turns (you were not part of them):\n\n${digest}\n\n---\nNow the user says:\n`;
}

/** One CLI run (`claude -p`, `codex exec`) in `session`, with a fresh live reply. Images go to disk first so the CLI can open them. */
function startTurn(session: SessionItem, text: string, images: Attachment[] = [], allow: string[] = [], preface = '') {
  if (images.length || !allow.length) lastPrompt[session.id] = text;
  // The CLI's own id for the conversation. Claude takes one we choose; Codex names its thread on the first turn
  // and reports it, so a new Codex chat starts without one.
  const first = session.source ?? session.agents?.[0] ?? session.agent;
  const prefix = `${session.agent}:`;
  let cliId: string | null;
  let resume: boolean;
  const selectedModel = modelFor(session);
  rememberLastChat(session.agent, selectedModel, (session.stepEffort ?? turnOptions.effort) as Effort);
  if (session.agent === first) {
    // A conversation whose last run never started is not resumed from its id (see handleAgentEvent).
    const fromId = session.agentStarted === false ? undefined : session.id.startsWith(prefix) ? session.id.slice(prefix.length) : undefined;
    const known = session.agentSession ?? fromId;
    cliId = known ?? (session.agent === 'claude' ? crypto.randomUUID() : null);
    const modelChanged = Boolean(session.conversationModel && session.conversationModel !== selectedModel);
    if (modelChanged) {
      cliId = session.agent === 'claude' ? crypto.randomUUID() : null;
      session.agentStarted = false;
    }
    if (cliId) session.agentSession = cliId;
    resume = cliId !== null && !modelChanged && (session.agentStarted ?? !!session.path);
    session.conversationModel = selectedModel ?? undefined;
  } else {
    // Another agent in this chat: its own conversation, started the first time it is used here.
    const own = session.agentIds?.[session.agent];
    cliId = own?.id ?? (session.agent === 'claude' ? crypto.randomUUID() : null);
    if (cliId && !own) (session.agentIds ??= {})[session.agent] = { id: cliId, started: false };
    const modelChanged = Boolean(session.agentModels?.[session.agent] && session.agentModels[session.agent] !== selectedModel);
    if (modelChanged) cliId = session.agent === 'claude' ? crypto.randomUUID() : null;
    resume = cliId !== null && !modelChanged && Boolean(own?.started);
    (session.agentModels ??= {})[session.agent] = selectedModel ?? '';
  }
  // A turn still marked live is replaced by this one: its events are ignored from now on (see `turns`), so it would
  // show "Thinking" for ever. It ends here, with what it wrote.
  const previous = liveReplies[session.id];
  if (previous?.live) {
    previous.live = false;
    if (previous.startedAt) previous.ms = Date.now() - previous.startedAt;
  }
  const turn = nextId();
  turns[session.id] = turn;
  const reply: Message = { id: nextId(), role: 'assistant', time: clock(new Date().toISOString()), at: new Date().toISOString(), blocks: [], live: true, startedAt: Date.now(), agent: session.agent, model: selectedModel ?? session.model };
  const list = messagesOf(session.id);
  const context = preface + missedTurns(session, list);
  const asked = list.at(-1);
  list.push(reply);
  liveReplies[session.id] = list[list.length - 1]; // the stored (reactive) object
  if (asked?.role === 'user') liveAsks[session.id] = asked;
  replies[session.id] = reply.id;
  session.status = 'working';
  const cwd = workspace.projects.find((project) => project.id === session.projectId)?.path ?? null;
  void (async () => {
    const paths = await Promise.all(images.map((image) => saveAttachment(image.mime, image.src.slice(image.src.indexOf(',') + 1))));
    // A message that starts with `/name` is a command: its text goes to the agent, the chat keeps what was typed.
    const body = await expandSlash(text, session.agent, cwd);
    if (session.agent === 'custom') {
      // A custom provider has no CLI to keep the conversation: the whole chat goes along with every turn.
      const target = parseCustomModel(selectedModel);
      const provider = connectors.custom.find((item) => item.id === target?.providerId);
      if (!target || !provider) throw new Error('This provider was removed or turned off. Pick another model.');
      const spoken = (message: Message) =>
        message.blocks.flatMap((block) => (block.kind === 'response' ? [block.text] : [])).join('\n').trim();
      const history = list
        .slice(0, -2)
        .map((message) => ({ role: message.role, content: spoken(message) }))
        .filter((message) => message.content);
      await sendCustomTurn({
        session: session.id,
        turn,
        kind: provider.kind,
        baseUrl: provider.baseUrl,
        apiKey: provider.apiKey || null,
        model: target.model,
        messages: [...history, { role: 'user', content: body }],
        permissionMode: PERMISSION_MODE[turnOptions.permission],
        allowedTools: [...new Set([...(approved[session.id] ?? []), ...allow])],
        cwd,
        effort: (session.stepEffort ?? turnOptions.effort) as Effort,
      });
      return;
    }
    const hint = OPEN_WORDS.test(body) ? AGENTDECK_HINT : '';
    const prompt =
      hint +
      context +
      (paths.length
        ? `${body || 'Look at the attached image.'}\n\nAttached image${paths.length > 1 ? 's' : ''} (open with the Read tool):\n${paths.map((path) => `- ${path}`).join('\n')}`
        : body);
    await sendAgentTurn({
      session: session.id,
      turn,
      agent: session.agent,
      prompt,
      agentSession: cliId,
      resume,
      // A model of another agent (left over from a switch) is never sent: the agent's own first model goes instead.
      model: selectedModel,
      permissionMode: PERMISSION_MODE[turnOptions.permission],
      effort: (session.stepEffort ?? turnOptions.effort) as Effort,
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
/** The message that started each running turn. */
const liveAsks: Record<string, Message> = {};

const askText = (message: Message) =>
  message.blocks
    .filter((block) => block.kind === 'response')
    .map((block) => block.text)
    .join('\n')
    .trim();

/**
 * A chat opened again while its reply is still being written: what was loaded (a saved copy, or the file as the CLI
 * has written it so far) does not hold the running turn, or holds only part of it. The live question and reply go
 * on the end, in place of any copy of that question, so the text keeps growing where the reader left it.
 */
function withLive(sessionId: string, list: Message[]): Message[] {
  const reply = liveReplies[sessionId];
  const asked = liveAsks[sessionId];
  if (!reply || !asked || list.includes(reply)) return list;
  const want = askText(asked);
  let cut = list.length;
  // Only the last few can be this turn: an older message with the same words is another turn.
  for (let index = list.length - 1; index >= Math.max(0, list.length - 3); index--) {
    if (list[index].role === 'user' && askText(list[index]) === want) {
      cut = index;
      break;
    }
  }
  return [...list.slice(0, cut), asked, reply];
}

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
  // Codex writes a thread's rollout, and Claude its session file, only once a run has started. A run stopped before
  // that leaves an id with nothing to resume. The next message then starts a new conversation, instead of resuming
  // that id again and failing the same way.
  if (event.kind === 'error' && /no rollout found|no conversation found/i.test(event.message)) {
    session.agentStarted = false;
    session.agentSession = undefined;
  }
  trackActivity(sessionId, session.title, workspace.projects.find((project) => project.id === session.projectId)?.name ?? '', event);
  // A line of the CLI the chat does not show: it is only kept in the Audit log.
  if (event.kind === 'raw') return;
  if (!reply) {
    if (event.kind === 'done' || event.kind === 'error') {
      delete replies[sessionId];
      session.status = 'idle';
    }
    return;
  }
  if (event.kind === 'usage') {
    reply.model ??= session.model;
    reply.tokens ??= { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 };
    const tokens = reply.tokens;
    if (event.new_call) {
      tokens.input += event.input_tokens;
      tokens.cacheRead += event.cache_read_tokens;
      tokens.cacheWrite += event.cache_write_tokens;
      reply.callOutput = 0;
    }
    tokens.output += Math.max(0, event.output_tokens - (reply.callOutput ?? 0));
    reply.callOutput = event.output_tokens;
    return;
  }
  if (event.kind === 'done' && event.cost_usd != null) reply.cost = event.cost_usd;
  const update = applyAgentEvent(reply.blocks, event);
  reply.blocks = update.blocks;
  // The CLI has the conversation once it answers; only then is `--resume` valid for the next turn.
  const first = session.source ?? session.agents?.[0] ?? session.agent;
  const agent = reply.agent ?? session.agent;
  if (agent === first) {
    if (event.kind !== 'error') session.agentStarted = true;
    if (update.agentSession) session.agentSession = update.agentSession;
  } else if (event.kind !== 'error' && (update.agentSession || session.agentIds?.[agent])) {
    const id = update.agentSession ?? session.agentIds?.[agent]?.id;
    if (id) (session.agentIds ??= {})[agent] = { id, started: true };
  }
  if (update.finished) {
    reply.live = false;
    if (session.path && session.source && reply.agent && reply.agent !== session.source) {
      keepOtherTurn(session, reply);
      rememberMixed(session);
    }
    if (reply.startedAt) reply.ms = Date.now() - reply.startedAt;
    if (reply.tokens) {
      const estimate = estimateCost(reply.tokens, reply.model);
      recordUsage({
        at: new Date().toISOString(),
        sessionId,
        agent: session.agent,
        model: reply.model,
        projectId: session.projectId,
        ms: reply.ms,
        tokens: { ...reply.tokens },
        cost: reply.cost ?? estimate,
        estimated: reply.cost == null,
      });
    }
    openServedPage(sessionId, reply);
    onDelegateDone(sessionId, reply, event.kind === 'error' ? event.message : null);
    onHandbackDone(sessionId, reply, event.kind === 'error');
    // Our own copy of the turn (agent, model, time), apart from what the CLIs store: the base for syncing later.
    archiveTurn(
      { id: sessionId, title: session.title, agent: session.agent, model: reply.model ?? session.model, projectPath: workspace.projects.find((project) => project.id === session.projectId)?.path ?? null },
      [liveAsks[sessionId], reply].filter((message): message is Message => Boolean(message)),
    );
    delete replies[sessionId];
    delete liveReplies[sessionId];
    delete liveAsks[sessionId];
    session.status = 'idle';
    persistLocal();
    // The CLI wrote the conversation to disk: pick it up so the row gets its path, in place (same id).
    // The cached transcript of this chat is now behind the file: a new time makes the next open read it again.
    session.updatedAt = new Date().toISOString();
    transcripts.delete(sessionId);
    if (!session.path) void importHistory({ refresh: true });
    // Messages typed while it worked go next, one at a time.
    const next = dequeue(queued, sessionId);
    if (next) setTimeout(() => send(sessionId, next.text, next.images, next.elements, next.mentions), 0);
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

// ---------- delegating to another agent ----------
// `@claude do this` in a chat does not switch the chat to Claude. The task runs in a chat of its own under this one
// (a child: it shows in the sidebar and can be opened), with a summary of this chat as its background. When it has
// answered, the answer goes back into this chat and the chat's own agent carries on with it. With several `@`s the
// children run side by side and the agent is called once, when the last one has answered.

type DelegateBatch = { main: string; task: string; /** The message whose cards show the work. */ asked: Message; pending: Set<string>; results: { label: string; text: string }[]; run: DelegateRun; /** A workflow preset's note to the agent that writes the final answer. */ orchestrator?: string; /** More for that agent: the findings the steps shared, where their notes are. */ memo?: string };
type DelegateCard = Extract<Block, { kind: 'delegate' }>;
const delegations = new Map<string, { batch: DelegateBatch; label: string; card: DelegateCard; node: RunNode }>();

// What the Workflow tab draws: one run per delegated message (main chat -> a child per agent -> handback -> main agent).
export type RunStats = { ms: number; tokens?: import('./usage').Tokens; cost: number | null; tools: number; last: string };
/** One question a step put to an earlier step (its chat answers it), kept for the Workflow tab and the Audit log. */
export type Talk = { to: string; question: string; answer?: string; at: number };
export type RunNode = { childId: string; talks?: Talk[]; /** The preset step it runs, when the run comes from a workflow. */ stepId?: string; /** What it was handed (earlier answers) and what it answered, for the audit. */ input?: string; output?: string; agent: Agent; model: string; label: string; startedAt: number; endedAt?: number; state: 'running' | 'done' | 'failed'; note?: string; stats?: RunStats };
export type DelegateRun = {
  id: string;
  main: string;
  task: string;
  /** Name of the workflow preset that planned this run; none for an `@agent` delegation. */
  preset?: string;
  /** The user stopped the chat: its agents are not called back and the steps still waiting do not start. */
  cancelled?: boolean;
  /** The steps of the preset as they were when the run started (drawn as the canvas). */
  plan?: import('./workflows.svelte').WorkflowStep[];
  /** Steps that did not run because their condition failed. */
  skipped?: string[];
  /** The folder of the run's shared notes (BOARD.md, agents/, handoff/), when it could be made. */
  folder?: string;
  startedAt: number;
  nodes: RunNode[];
  /** waiting: children still working; running: the main agent is answering with their answers. */
  handback: 'waiting' | 'running' | 'done' | 'failed';
  handbackAt?: number;
  endedAt?: number;
  handbackStats?: RunStats;
};
const RUN_LIMIT = 100;
export const delegateRuns = $state<DelegateRun[]>([]);

// Finished runs are kept (IndexedDB), so the Workflow tab has a history to judge the flows by after a restart. What a
// step was handed and answered is cut short: the numbers and the outcome are what the history is for.
const RUNS_KEY = 'workflow.runs';
const clip = (text: string | undefined, max = 1500) => (text && text.length > max ? `${text.slice(0, max)}…` : text);
void getPref<DelegateRun[]>(RUNS_KEY).then((saved) => {
  if (!Array.isArray(saved)) return;
  const known = new Set(delegateRuns.map((run) => run.id));
  delegateRuns.unshift(...saved.filter((run) => !known.has(run.id)));
});
$effect.root(() => {
  $effect(() => {
    const ended = delegateRuns.filter((run) => run.endedAt);
    const snapshot = JSON.parse(JSON.stringify(ended)) as DelegateRun[];
    for (const run of snapshot) for (const node of run.nodes) Object.assign(node, { input: clip(node.input), output: clip(node.output) });
    setPref(RUNS_KEY, snapshot.slice(-RUN_LIMIT));
  });
});

function statsOf(reply: Message | undefined, startedAt: number): RunStats | undefined {
  if (!reply) return undefined;
  const text = reply.blocks.flatMap((block) => (block.kind === 'response' ? [block.text] : []));
  const last = text.join("\n").split('\n').reverse().find((line) => line.trim()) ?? '';
  const commands = reply.blocks.filter((block): block is Extract<Block, { kind: 'command' }> => block.kind === 'command');
  const cost = reply.cost ?? (reply.tokens ? estimateCost(reply.tokens, reply.model) : null);
  return {
    ms: reply.ms ?? Date.now() - (reply.startedAt ?? startedAt),
    tokens: reply.tokens ? { ...reply.tokens } : undefined,
    cost,
    tools: commands.length,
    last: last.trim() || (commands.length ? commands[commands.length - 1].command : ''),
  };
}

/** The numbers of a turn still being written in `sessionId` (read it again each second: the reply is not reactive here). */
export const liveStats = (sessionId: string, startedAt: number) => statsOf(liveReplies[sessionId], startedAt);

/** Whether the chat `childId` is still working on something it was handed. */
export const isDelegating = (childId: string) => delegations.has(childId) || stepWaiters.has(childId);
/** Whether a run that started some of these chats is still going (a step waiting in its queue is waiting for it). */
export const isRunActive = (childIds: string[]) => delegateRuns.some((run) => !run.endedAt && run.nodes.some((node) => childIds.includes(node.childId)));

const firstLineOf = (text: string, max: number) => {
  const line = text.split('\n')[0].trim();
  return line.length > max ? `${line.slice(0, max - 1)}…` : line;
};

const plainText = (message: Message) =>
  message.blocks
    .flatMap((block) => (block.kind === 'response' ? [block.text] : block.kind === 'command' ? [`(ran: ${block.command.slice(0, 160)})`] : []))
    .join('\n')
    .trim();

/** The last messages of a chat as plain text, newest kept when it is long. */
function digestOf(messages: Message[], limit = 6000): string {
  const lines = messages.slice(-8).map((message) => {
    const text = plainText(message);
    return `${message.role === 'user' ? 'User' : agentLabel(message.agent, message.model)}: ${text.length > 1500 ? `${text.slice(0, 1500)}…` : text}`;
  });
  const joined = lines.join('\n\n');
  return joined.length > limit ? `…${joined.slice(-limit)}` : joined;
}

/** What the `@chat` and `@project` mentions of a message add in front of the prompt. */
function mentionContext(mentions: Mention[]): string {
  const parts: string[] = [];
  for (const mention of mentions) {
    if (mention.kind === 'session') {
      const messages = transcripts.get(mention.id)?.messages ?? localMessages[mention.id] ?? [];
      parts.push(messages.length ? `Chat "${mention.label}", its latest messages:\n${digestOf(messages, 4000)}` : `Chat "${mention.label}" (not opened in this app yet, so no text of it is known here)`);
    } else if (mention.kind === 'project') {
      const project = workspace.projects.find((item) => item.id === mention.id);
      if (project) parts.push(`Project "${project.name}" is at ${project.path}`);
    }
  }
  return parts.length ? `The user attached this context:\n\n${parts.join('\n\n')}\n\n---\n` : '';
}

function delegate(main: SessionItem, asked: Message, task: string, targets: Mention[], preface: string, images: Attachment[], elements: PageElement[]) {
  delegateRuns.push({ id: crypto.randomUUID(), main: main.id, task, startedAt: Date.now(), nodes: [], handback: 'waiting' });
  if (delegateRuns.length > RUN_LIMIT) delegateRuns.splice(0, delegateRuns.length - RUN_LIMIT);
  const run = delegateRuns[delegateRuns.length - 1];
  const batch: DelegateBatch = { main: main.id, task, asked, pending: new Set(), results: [], run };
  const history = [...messagesOf(main.id)];
  main.status = 'working';
  for (const target of targets) {
    const [agent, model] = target.id.split('/') as [Agent, string];
    const uuid = crypto.randomUUID();
    const id = `claude:${uuid}`;
    workspace.sessions.push({
      id,
      projectId: main.projectId,
      title: `@${agentLabel(agent)}: ${firstLineOf(task, 50)}`,
      agent,
      agentSession: uuid,
      model,
      gitBranch: main.gitBranch,
      status: 'idle',
      parentId: main.id,
      draft: false,
    });
    const child = workspace.sessions.find((item) => item.id === id);
    if (!child) continue;
    asked.blocks.push({ kind: 'delegate', childId: id, agent, model, state: 'running' });
    const card = asked.blocks[asked.blocks.length - 1] as DelegateCard;
    const label = `${agentLabel(agent)} · ${modelLabel(model)}`;
    run.nodes.push({ childId: id, agent, model, label, startedAt: Date.now(), state: 'running' });
    const node = run.nodes[run.nodes.length - 1];
    delegations.set(id, { batch, label, card, node });
    batch.pending.add(id);
    // The child has not spoken in this chat, so every earlier turn of it is background for the child.
    const background = missedTurns({ ...child, agent: '' as Agent }, history);
    const brief = `${preface}${background}You were asked from another chat, whose own agent is ${agentLabel(main.agent)}, to do this part of the work: ${task}\n\nFinish with a complete answer that agent can use without seeing your work. ${LANGUAGE_RULE}`;
    send(id, task, images, elements, [], { preface: brief });
  }
  if (batch.pending.size === 0) {
    main.status = 'idle';
    delegateRuns.splice(delegateRuns.indexOf(run), 1);
  }
  keepHandout(main.id, asked);
  persistLocal();
}

/** A child chat finished (or stopped, or failed): its card is settled, and the last one of a batch calls the agent back. */
function onDelegateDone(childId: string, reply: Message, error: string | null) {
  const waiter = stepWaiters.get(childId);
  if (waiter) {
    stepWaiters.delete(childId);
    waiter(plainText({ ...reply, blocks: reply.blocks.filter((block) => block.kind === 'response') }), error, reply);
    return;
  }
  const entry = delegations.get(childId);
  if (!entry) return;
  delegations.delete(childId);
  const text = plainText({ ...reply, blocks: reply.blocks.filter((block) => block.kind === 'response') });
  const ok = !error && text.length > 0;
  entry.card.state = ok ? 'done' : 'failed';
  entry.card.note = ok ? undefined : (error ?? 'no answer');
  entry.node.state = ok ? 'done' : 'failed';
  entry.node.note = ok ? undefined : (error ?? 'no answer');
  entry.node.endedAt = Date.now();
  entry.node.stats = statsOf(reply, entry.node.startedAt);
  entry.batch.results.push({ label: entry.label, text: ok ? text : `(it did not answer: ${error ?? 'no text'})` });
  entry.batch.pending.delete(childId);
  keepHandout(entry.batch.main, entry.batch.asked);
  if (entry.batch.pending.size === 0) {
    if (entry.batch.run.cancelled) endCancelled(entry.batch.main, entry.batch.run);
    else handBack(entry.batch);
  }
}

/** A run the user stopped: nothing is called back, the chat is free again. */
function endCancelled(main: string, run: DelegateRun) {
  const session = workspace.sessions.find((item) => item.id === main);
  if (session && !replies[main]) session.status = 'idle';
  run.handback = 'failed';
  run.endedAt = Date.now();
}

// ---------- workflow presets: the chat's agent as orchestrator ----------
// A chat with a flow does not answer a message itself. Its steps run as child chats (sub sessions under it), each
// stage after the one that feeds it; steps of one stage run side by side. To keep tokens down every step gets only
// its own job, the task and the short handoffs of the steps that feed it, never their whole transcripts. The run has
// a folder of shared notes (`.agentdeck/runs/<run>/`): BOARD.md, written by the app only (goal, plan, handoffs),
// `agents/<name>.md`, one per step and written by that step only (findings, questions to the others and answers),
// and `handoff/<n>-<name>.md`, the summary a step leaves for the next. One writer per file, so no locking is needed.
// When the last stage is done the chat's own agent is called once with the handoffs and the findings.

/** A step's question to an earlier step: its last line `ASK @<step>: <question>`. */
const MAX_ASKS = 2;
const askOf = (text: string) => {
  const found = /^\s*ASK @([^:\n]{1,60}):\s*(.+)$/m.exec(text);
  return found ? { to: found[1].trim(), question: found[2].trim() } : null;
};
const shorten = (text: string, max: number) => (text.length > max ? `${text.slice(0, max)}…` : text);

/** The line a step ends with to send its input back to the step(s) before it. */
const REPLAN_MARK = 'REPLAN:';
const MAX_REPLANS = 1;
const replanOf = (text: string) => {
  const found = /^\s*REPLAN:\s*(.+)$/m.exec(text);
  if (found) return found[1].trim();
  // The handoff can say it too: STATUS: needs-plan-change, with what is missing in OPEN.
  if (/^\s*STATUS:\s*needs-plan-change/im.test(text)) return /^\s*OPEN:\s*(.+)$/im.exec(text)?.[1].trim() || 'the plan needs changes';
  return null;
};

/** The section a step ends with: short labelled lines the next agent reads at a glance, never prose. */
const HANDOFF_RULE =
  'End with "## Handoff" (<=120 words, only this is passed on; "-" if empty): STATUS: done|blocked|needs-plan-change / RESULT: <1 line> / FILES: <paths> / FACTS: <<=5, "; "-separated, path:line> / OPEN: <<=3> / VERIFY: <cmd> -> pass|fail|not run / NEXT: <first thing the next step does>. Telegraphic: no filler, no restating the task.';

/** How the prompt that hands the answers back starts; read back from the chat's file it is shown short. */
const HANDBACK_HEAD = 'The agents you handed work to have answered.';

/** Told to every agent of a run: the user's language, not the language of these instructions. */
const LANGUAGE_RULE = "Reply in the user's language (these notes are English); code, paths, commands unchanged.";

const stepWaiters = new Map<string, (text: string, error: string | null, reply: Message) => void>();
/** Flow steps waiting for the user's answers, by their chat. */
const askingCards = new Map<string, DelegateCard>();

/** Sends the answers to questions an agent asked (in its own chat, or a flow step's from the Flow steps card). */
export function answerQuestions(sessionId: string, text: string) {
  if (!text.trim() || replies[sessionId]) return;
  const card = askingCards.get(sessionId);
  if (card) {
    card.state = 'running';
    askingCards.delete(sessionId);
  }
  send(sessionId, text, [], []);
}

const passes = (step: WorkflowStep, previous: string) => {
  const { mode, text } = step.condition;
  if (mode === 'always' || !text.trim()) return true;
  const found = previous.toLowerCase().includes(text.trim().toLowerCase());
  return mode === 'contains' ? found : !found;
};

/** Reads (no `content`) or writes one file of a run's notes; '' when it cannot. */
async function runFile(root: string | null, run: string, name: string, content?: string): Promise<string> {
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    const args = { root, run, name, ...(content === undefined ? {} : { content }) };
    const out = await invoke(content === undefined ? 'run_file_read' : 'run_file_write', args);
    return typeof out === 'string' ? out : '';
  } catch {
    return '';
  }
}

const slugOf = (text: string) => text.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '').slice(0, 32) || 'step';

/** The part of an answer meant for the next step: its "## Handoff" section, else its end. */
function handoffOf(text: string): string {
  const at = text.search(/^#{1,3}\s*handoff\b/im);
  const part = (at >= 0 ? text.slice(at).replace(/^#{1,3}\s*handoff[^\n]*\n?/i, '') : text.slice(-1200)).trim();
  return part.length > 1600 ? `${part.slice(0, 1600)}…` : part;
}

/** The findings (`[F]` lines) the steps wrote in their notes, without repeats, at most `limit` lines. */
function findingsOf(notes: string[], limit = 30): string[] {
  const seen = new Set<string>();
  for (const text of notes) {
    for (const line of text.split('\n')) {
      const item = line.trim().replace(/^[-*]\s*/, '');
      if (item.startsWith('[F]') && !seen.has(item)) seen.add(item);
      if (seen.size >= limit) return [...seen];
    }
  }
  return [...seen];
}

type FlowRunContext = { root: string | null; folder: string; board: string[]; slugs: Map<string, string>; handoffs: Map<string, string> };

function boardText(task: string, preset: WorkflowPreset, ctx: FlowRunContext, groups: WorkflowStep[][]): string {
  const plan = groups.map((group, stage) => `${stage + 1}. ${group.map((step) => `${step.name} (${agentLabel(step.agent, step.model)}) — notes: agents/${ctx.slugs.get(step.id)}.md`).join(' | ')}`).join('\n');
  return [`# Run board — ${preset.name}`, '', `Goal: ${firstLineOf(task, 300)}`, '', '## Plan (one line per stage; steps of a stage run at the same time)', plan, '', '## Handoffs', ...(ctx.board.length ? ctx.board : ['(none yet)']), ''].join('\n');
}

function runStep(main: SessionItem, asked: Message, run: DelegateRun, step: WorkflowStep, brief: string, task: string, previous: string, images: Attachment[], elements: PageElement[], queued?: { uuid: string; card: DelegateCard }) {
  const agent = step.agent;
  const model = step.model || MODELS[agent]?.[0]?.value || '';
  const uuid = queued?.uuid ?? crypto.randomUUID();
  const id = `claude:${uuid}`;
  workspace.sessions.push({
    id,
    projectId: main.projectId,
    title: `${step.name}: ${firstLineOf(task, 50)}`,
    agent,
    agentSession: uuid,
    model,
    gitBranch: main.gitBranch,
    status: 'idle',
    parentId: main.id,
    stepEffort: step.effort || undefined,
    draft: false,
  });
  // Its card was made when the run began, as "queued": now it is working.
  let card: DelegateCard;
  if (queued) {
    card = queued.card;
    card.state = 'running';
    // A step run again (its plan was sent back) gets a new chat: the card follows it.
    card.childId = id;
    card.note = undefined;
  } else {
    asked.blocks.push({ kind: 'delegate', childId: id, agent, model, state: 'running', step: step.name });
    card = asked.blocks[asked.blocks.length - 1] as DelegateCard;
  }
  const label = `${step.name} · ${agentLabel(agent, model)}`;
  run.nodes.push({ childId: id, stepId: step.id, agent, model, label, startedAt: Date.now(), state: 'running', input: previous });
  const node = run.nodes[run.nodes.length - 1];
  const asks = new Map<string, number>();
  return new Promise<{ step: WorkflowStep; label: string; text: string; ok: boolean }>((resolve) => {
    const settle = (text: string, error: string | null, reply: Message) => {
      // The step asked the user first: the run waits here until the answers come back as the step's next turn.
      const questions = error ? [] : parseQuestions(text).questions;
      if (questions.length) {
        card.state = 'asking';
        card.questions = questions;
        askingCards.set(id, card);
        stepWaiters.set(id, settle);
        keepHandout(main.id, asked);
        return;
      }
      // The step needs a fact only an earlier step knows: that step's own chat (it has the context) is asked, its short
      // answer goes back to this step as its next turn. Capped, so two steps cannot talk for ever.
      const ask = error ? null : askOf(text);
      const target = ask && (asks.get(id) ?? 0) < MAX_ASKS ? run.nodes.findLast((item) => item.state === 'done' && item.childId !== id && item.label.split(' · ')[0].toLowerCase() === ask.to.toLowerCase()) : undefined;
      if (ask && target) {
        asks.set(id, (asks.get(id) ?? 0) + 1);
        (node.talks ??= []).push({ to: ask.to, question: ask.question, at: Date.now() });
        const talk = node.talks[node.talks.length - 1];
        card.talking = ask.to;
        card.talkText = ask.question;
        keepHandout(main.id, asked);
        // Registered at once: a Stop while it waits ends this step (it has no reply of its own to stop).
        stepWaiters.set(id, settle);
        stepWaiters.set(target.childId, (answer, failed) => {
          talk.answer = failed ? '(no answer)' : shorten(answer, 700);
          card.talking = undefined;
          card.talkText = undefined;
          logTalk(main.id, main.title, workspace.projects.find((project) => project.id === main.projectId)?.name ?? '', step.name, ask.to, ask.question, talk.answer);
          keepHandout(main.id, asked);
          send(id, `A from ${ask.to}: ${talk.answer}`, [], []);
        });
        send(target.childId, `Q from ${step.name}: ${ask.question}\nAnswer in at most 60 words, only the answer.`, [], []);
        return;
      }
      card.questions = undefined;
      const ok = !error && text.length > 0;
      card.state = ok ? 'done' : 'failed';
      card.note = ok ? undefined : (error ?? 'no answer');
      node.state = ok ? 'done' : 'failed';
      node.note = card.note;
      node.endedAt = Date.now();
      node.stats = statsOf(reply, node.startedAt);
      node.output = text;
      keepHandout(main.id, asked);
      resolve({ step, label, text: ok ? text : `(it did not answer: ${error ?? 'no text'})`, ok });
    };
    stepWaiters.set(id, settle);
    send(id, task, images, elements, [], { preface: brief });
  });
}

async function runWorkflow(main: SessionItem, asked: Message, preset: WorkflowPreset, task: string, preface: string, images: Attachment[], elements: PageElement[]) {
  const runId = crypto.randomUUID();
  const graph = normalizeGraph(preset.steps);
  const links = inputsOf(graph);
  const groups = stepGroups(graph);
  delegateRuns.push({ id: runId, main: main.id, task, preset: preset.name, plan: JSON.parse(JSON.stringify(graph)), skipped: [], startedAt: Date.now(), nodes: [], handback: 'waiting' });
  if (delegateRuns.length > RUN_LIMIT) delegateRuns.splice(0, delegateRuns.length - RUN_LIMIT);
  const run = delegateRuns[delegateRuns.length - 1];
  const batch: DelegateBatch = { main: main.id, task, asked, pending: new Set(), results: [], run, orchestrator: preset.orchestrator.trim() || undefined };
  main.status = 'working';

  const root = workspace.projects.find((project) => project.id === main.projectId)?.path ?? null;
  const slugs = new Map<string, string>();
  graph.forEach((step, index) => slugs.set(step.id, `${index + 1}-${slugOf(step.name)}`));
  const ctx: FlowRunContext = { root, folder: '', board: [], slugs, handoffs: new Map() };
  const boardPath = await runFile(root, runId, 'BOARD.md', boardText(task, preset, ctx, groups));
  ctx.folder = boardPath ? boardPath.replace(/[\\/]BOARD\.md$/, '') : '';
  run.folder = ctx.folder || undefined;

  // Every step has its card in the message from the start: the ones that wait say "Queued", the ones of a stage that
  // runs together all say "Working" once it starts.
  const cards = new Map<string, { uuid: string; card: DelegateCard }>();
  for (const step of graph) {
    const uuid = crypto.randomUUID();
    asked.blocks.push({ kind: 'delegate', childId: `claude:${uuid}`, agent: step.agent, model: step.model || MODELS[step.agent]?.[0]?.value || '', state: 'queued', step: step.name });
    cards.set(step.id, { uuid, card: asked.blocks[asked.blocks.length - 1] as DelegateCard });
  }
  keepHandout(main.id, asked);

  let first = true;
  // A step can send its input back (REPLAN): the steps that fed it run again with the reason, at most MAX_REPLANS times.
  const replanNotes = new Map<string, string>();
  let replans = 0;
  for (let stage = 0; stage < groups.length; stage++) {
    const group = groups[stage];
    const inputText = (step: WorkflowStep) =>
      (links[step.id] ?? [])
        .filter((from) => from !== 'start' && ctx.handoffs.has(from))
        .map((from) => `### ${graph.find((item) => item.id === from)?.name ?? from}\n${ctx.handoffs.get(from)}`)
        .join('\n\n');
    const active = group.filter((step) => passes(step, inputText(step)));
    for (const step of group) {
      if (active.includes(step)) continue;
      run.skipped?.push(step.id);
      const skipped = cards.get(step.id)?.card;
      if (skipped) {
        skipped.state = 'skipped';
        skipped.note = 'Condition not met';
      }
    }
    if (active.length === 0) continue;
    const board = boardText(task, preset, ctx, groups);
    const done = await Promise.all(
      active.map((step) => {
        const slug = slugs.get(step.id) ?? slugOf(step.name);
        const previous = inputText(step);
        const others = active.filter((item) => item !== step).map((item) => `"${item.name}" (notes: agents/${slugs.get(item.id)}.md)`);
        const where = ctx.folder || '.agentdeck/runs/' + runId;
        const fed = (links[step.id] ?? []).some((from) => from !== 'start');
        // Written for the agent, not the user: short labelled lines. What the user reads is the chat itself.
        const brief = [
          preface,
          `Step "${step.name}" of an AgentDeck run (orchestrator: ${agentLabel(main.agent)}). Do only this job:`,
          step.prompt.trim() || 'Do your part of the task.',
          `Run folder: ${where}. BOARD.md = goal, plan, handoffs (app-written, read-only${ctx.folder ? '; read it first' : '; below'}). Notes: agents/${slug}.md, yours only, append one line each: [F] fact (path:line) | [Q @name] question | [A @name] answer. Never edit BOARD.md or others' notes.`,
          ctx.folder ? '' : board,
          previous ? `IN (from the step before; read its notes only if needed):\n${previous}` : '',
          others.length ? `Parallel with you: ${others.join(', ')}. Read their notes at start and after each big step, answer [Q @${step.name}] with [A], do not edit files they claim.` : '',
          LANGUAGE_RULE,
          QUESTION_RULE,
          fed ? `Need one fact only an earlier step knows: end with "ASK @<step name>: <question>" and stop (max ${MAX_ASKS}). Input too unclear or wrong to work from: do not guess, end with "${REPLAN_MARK} <what to fix>" and stop.` : '',
          replanNotes.get(step.id) ? `Sent back by the next step: "${replanNotes.get(step.id)}". Fix that.` : '',
          REVIEW_STEP.test(`${step.name} ${step.prompt}`) ? REVIEW_RULE : '',
          HANDOFF_RULE,
        ]
          .filter((line, index, all) => line !== '' || all[index - 1] !== '')
          .join('\n');
        return runStep(main, asked, run, step, brief, task, previous, first ? images : [], first ? elements : [], cards.get(step.id));
      }),
    );
    first = false;
    for (const item of done) {
      const handoff = item.ok ? handoffOf(item.text) : item.text;
      ctx.handoffs.set(item.step.id, handoff);
      const name = `handoff/${slugs.get(item.step.id)}.md`;
      void runFile(root, runId, name, `# ${item.step.name}\n\n${handoff}\n`);
      ctx.board.push(`- ${item.step.name}: ${name}${item.ok ? '' : ' (failed)'}`);
      batch.results.push({ label: item.label, text: handoff });
    }
    void runFile(root, runId, 'BOARD.md', boardText(task, preset, ctx, groups));
    // A step that failed ends the chain: the agent is told what happened so far.
    if (run.cancelled || done.some((item) => !item.ok)) break;
    for (const step of active) replanNotes.delete(step.id);
    // A step sent its input back: the steps that fed it run again (and every stage after them), with its reason.
    const back = done.find((item) => item.ok && replanOf(item.text) !== null);
    if (back && replans < MAX_REPLANS) {
      const feeders = (links[back.step.id] ?? []).filter((from) => from !== 'start');
      const from = Math.min(...feeders.map((id) => groups.findIndex((list) => list.some((item) => item.id === id))).filter((index) => index >= 0));
      if (Number.isFinite(from)) {
        replans++;
        const reason = replanOf(back.text) ?? '';
        for (const id of feeders) replanNotes.set(id, reason);
        const again = groups.slice(from, stage + 1).flat();
        // Their cards wait again; each gets a new chat when it starts.
        for (const item of again) {
          const entry = cards.get(item.id);
          if (!entry) continue;
          entry.card.state = 'queued';
          entry.card.note = feeders.includes(item.id) ? 'Sent back' : undefined;
          cards.set(item.id, { uuid: crypto.randomUUID(), card: entry.card });
          ctx.handoffs.delete(item.id);
        }
        batch.results = batch.results.filter((result) => !again.some((item) => result.label.startsWith(`${item.name} ·`)));
        ctx.board.push(`- ${back.step.name} sent its input back: ${reason}`);
        keepHandout(main.id, asked);
        stage = from - 1;
      }
    }
  }
  // A step that failed ends the chain: those that never started say so, instead of waiting for ever.
  for (const { card } of cards.values()) {
    if (card.state === 'queued') {
      card.state = 'skipped';
      card.note = 'Not run';
    }
  }
  keepHandout(main.id, asked);
  if (run.cancelled) {
    endCancelled(main.id, run);
    return;
  }
  if (batch.results.length === 0) {
    main.status = 'idle';
    run.handback = 'failed';
    run.endedAt = Date.now();
    return;
  }
  const notes = await Promise.all(graph.map((step) => runFile(root, runId, `agents/${slugs.get(step.id)}.md`)));
  const facts = findingsOf(notes);
  batch.memo = [facts.length ? `Findings the steps shared:\n${facts.map((fact) => `- ${fact}`).join('\n')}` : '', ctx.folder ? `The run's notes are in ${ctx.folder} (BOARD.md, agents/, handoff/): open them only if you need more detail.` : '']
    .filter(Boolean)
    .join('\n\n');
  handBack(batch);
}

/** The main agent finished the turn that got the answers: the run is over. */
function onHandbackDone(sessionId: string, reply: Message, failed: boolean) {
  const run = delegateRuns.find((item) => item.main === sessionId && item.handback === 'running');
  if (!run) return;
  run.handback = failed ? 'failed' : 'done';
  run.endedAt = Date.now();
  run.handbackStats = statsOf(reply, run.handbackAt ?? run.startedAt);
}

/** Puts the answers into the chat that asked, and runs its agent on them as soon as it is free. */
/** A step that reviews or verifies: it looks at the result itself when it can, and ends with a verdict the owner reads. */
const REVIEW_STEP = /\b(review|verify|check|test|qa)\b/i;
const REVIEW_RULE =
  'Verify, do not only read: if the change shows on a page, open it with agentdeck browser_open (or browser_serve), use browser_screenshot and browser_console, and say what you saw. End with one line "VERDICT: PASS" or "VERDICT: FAIL - <blocking problems>". Risks and nice-to-haves go under "Suggestions", they are not a FAIL.';

function handBack(batch: DelegateBatch) {
  const answers = batch.results.map((result) => `### ${result.label}\n${result.text}`).join('\n\n');
  const memo = batch.memo ? `\n\n${batch.memo}` : '';
  const finish = batch.orchestrator ? `${batch.orchestrator}\n\nThe user's request: ${batch.task}` : `Using their answers, finish the user's request: ${batch.task}`;
  // A step that reviews can find the work wrong. The agent that gets the answers then owns the result: it fixes what is
  // wrong and cleans up after the steps, instead of only reporting it.
  const owner =
    "You own the result. Fix only what blocks the request: a step's VERDICT: FAIL, a failed check, or work that is wrong/unfinished. Then do not just report: see all changes (handoff FILES; git status, git diff), undo wrong ones, delete leftovers (debug output, stray files, notes), redo correctly, run the project's checks, end with what you fixed/removed. Suggestions and risks a step lists are not failures: do not edit for them, list them for the user in one short line each. If all is right, say so in one line.";
  const prompt = `${HANDBACK_HEAD}\n\n${answers}${memo}\n\n${finish}\n\n${owner}\n\n${LANGUAGE_RULE}`;
  const shown = `↩ ${batch.results.map((result) => result.label.split(' · ')[0]).join(', ')} answered`;
  const go = (tries: number) => {
    if (!replies[batch.main]) {
      batch.run.handback = 'running';
      batch.run.handbackAt = Date.now();
      send(batch.main, shown, [], [], [], { prompt });
    }
    else if (tries < 900) setTimeout(() => go(tries + 1), 1000);
  };
  go(0);
}

/** Send from the message box. While the agent is busy: wait in line (Enter), or stop it and go first (Ctrl+Enter). */
export function sendMessage(text: string, images: Attachment[] = [], elements: PageElement[] = [], mentions: Mention[] = [], whileRunning: 'queue' | 'interrupt' = 'queue') {
  const session = activeSession();
  if (!session || (!text.trim() && images.length === 0 && elements.length === 0)) return;
  // A task for another agent needs words to say what the task is.
  if (mentions.some((mention) => mention.kind === 'ai') && !text.trim()) return;
  if (replies[session.id]) {
    if (whileRunning === 'queue') {
      enqueue(queued, session.id, { text, images, elements, mentions });
      return;
    }
    stopTurn(session.id);
  }
  send(session.id, text, images, elements, mentions);
}

/** Sends a queued message now: the reply running in the chat is stopped (what it wrote stays), and this one goes out. */
export function steerQueued(sessionId: string, item: Queued) {
  const rest = (queued[sessionId] ?? []).filter((entry) => entry !== item);
  if (rest.length) queued[sessionId] = rest;
  else delete queued[sessionId];
  if (replies[sessionId]) stopTurn(sessionId);
  send(sessionId, item.text, item.images, item.elements, item.mentions);
}

/** A message as it is recognised again in the chat's file (where it has another id than the one it got here). */
const signature = (message: Message) =>
  `${message.role}|${message.blocks.map((block) => (block.kind === 'response' ? block.text.trim() : '')).join('').slice(0, 300)}`;

/** Messages taken out of a saved chat by an edit: its file still has them, so they are hidden when it is read. */
const hidden: Record<string, string[]> = {};
void getPref<Record<string, string[]>>('chat.hidden').then((saved) => {
  if (saved) Object.assign(hidden, saved);
});

/**
 * Edits one of your messages, as in ChatGPT: it and everything after it go, and the new text is sent in its
 * place (with the same pictures). A reply still running is stopped first.
 */
export function editMessage(messageId: string, text: string) {
  const session = activeSession();
  if (!session || !text.trim()) return;
  const list = messagesOf(session.id);
  const index = list.findIndex((message) => message.id === messageId && message.role === 'user');
  if (index === -1) return;
  if (replies[session.id]) stopTurn(session.id);
  delete queued[session.id];
  const images: Attachment[] = list[index].blocks.flatMap((block) =>
    block.kind === 'image' ? [{ id: nextId(), name: block.name, mime: block.src.slice(5, block.src.indexOf(';')) || 'image/png', src: block.src }] : [],
  );
  // The page elements sent with it go again too: an edit changes the words, not what was attached.
  const elements: PageElement[] = list[index].blocks.flatMap((block) =>
    block.kind === 'element'
      ? [{ id: nextId(), tag: block.tag, text: block.text, selector: block.selector, html: block.html ?? '', around: block.around, box: { x: block.box?.x ?? 0, y: block.box?.y ?? 0, w: block.box?.w ?? 0, h: block.box?.h ?? 0 }, style: block.style ?? {}, url: block.url ?? '', title: block.title ?? '', note: block.note }]
      : [],
  );
  const removed = list.splice(index);
  if (session.path) {
    hidden[session.id] = [...new Set([...(hidden[session.id] ?? []), ...removed.flatMap((message) => [message.id, signature(message)])])];
    setPref('chat.hidden', hidden);
    const others = otherTurns[session.id];
    if (others) otherTurns[session.id] = others.filter((message) => !removed.includes(message));
    transcripts.delete(session.id);
  } else {
    localMessages[session.id] = list;
    persistLocal();
  }
  send(session.id, text, images, elements);
}

/**
 * `extra.prompt` is what the agent is sent when it differs from the text shown in the chat, and `extra.preface` goes in
 * front of the prompt without being shown (context of a delegated task).
 */
function send(sessionId: string, text: string, images: Attachment[], elements: PageElement[], mentions: Mention[] = [], extra: { prompt?: string; preface?: string } = {}) {
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
      ...elements.map((item) => ({ kind: 'element' as const, tag: item.tag, text: item.text, selector: item.selector, url: item.url, title: item.title, note: item.note, html: item.html, around: item.around, box: { ...item.box }, style: { ...item.style } })),
      ...(body ? [{ kind: 'response' as const, text: body }] : []),
    ],
    at: new Date().toISOString(),
  });
  const preface = (extra.preface ?? '') + mentionContext(mentions);
  const targets = mentions.filter((mention) => mention.kind === 'ai');
  const preset = presetOf(sessionId);
  if (inTauri() && targets.length > 0 && body) delegate(session, messages[messages.length - 1], body, targets, preface, images, elements);
  else if (inTauri() && preset && preset.steps.length > 0 && body && !extra.prompt && !extra.preface) void runWorkflow(session, messages[messages.length - 1], preset, body, preface, images, elements);
  else if (inTauri() && (session.agent === 'claude' || session.agent === 'codex' || session.agent === 'antigravity' || session.agent === 'custom')) startTurn(session, withElements(extra.prompt ?? body, elements), images, [], preface);
  else if (inTauri()) {
    messages.push({
      id: nextId(),
      role: 'assistant',
      time: clock(new Date().toISOString()),
      blocks: [{ kind: 'response', text: `⚠ ${session.agent} chats are not connected yet. Claude and Codex run from here for now.` }],
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
export type TreeData = {
  /**
   * `section` is a heading row (Recents, This computer, Projects, or a date); a `session` with `flat` sits right under
   * one; `more` is the "Show more / Show less" row at the end of a long list.
   */
  kind: 'project' | 'session' | 'section' | 'more';
  session?: SessionItem;
  path?: string;
  /** The last fork under its chat: its branch line ends there. */
  last?: boolean;
  flat?: boolean;
  /** Section: how many rows it holds (no fold arrow when empty). More: the list it unfolds, and whether it is open. */
  count?: number;
  group?: string;
  open?: boolean;
};

/** How the sidebar lists chats: by folder (Recents, This computer, Projects), by date, or one plain list. */
export type SidebarView = { groupBy: 'folder' | 'date' | 'none'; sortBy: 'updated' | 'name'; order: 'desc' | 'asc' };
export const DEFAULT_VIEW: SidebarView = { groupBy: 'folder', sortBy: 'updated', order: 'desc' };
export type TreeOptions = {
  view: SidebarView;
  /** Heading keys that are folded. */
  collapsed: string[];
  /** Lists showing every row instead of the first few ("Show more" pressed). */
  showAll: string[];
};
/** Rows a list shows before its "Show more". */
const LIST_LIMIT = 5;
const PROJECT_LIMIT = 10;

/** The project of the open chat while it is still an empty new session: its row shows where the chat will go. */
export function pendingProject(): string | undefined {
  const active = activeSession();
  return active && isEmptyDraft(active) ? active.projectId : undefined;
}

export function buildTree({ view, collapsed, showAll }: TreeOptions = { view: DEFAULT_VIEW, collapsed: [], showAll: [] }): ZTreeNode<TreeData>[] {
  // A session with no time yet is new: it goes first.
  const newest = (a: SessionItem, b: SessionItem) => {
    if (!a.updatedAt || !b.updatedAt) return Number(!b.updatedAt) - Number(!a.updatedAt);
    return b.updatedAt.localeCompare(a.updatedAt);
  };
  // Two levels, as in Claude: every fork, a fork of a fork too, sits right under the chat it all started from.
  const rootOf = (item: SessionItem): string => {
    let cursor = item;
    for (let depth = 0; cursor.parentId && depth < 50; depth++) {
      const parent = workspace.sessions.find((other) => other.id === cursor.parentId);
      if (!parent) break;
      cursor = parent;
    }
    return cursor.id;
  };
  const forksOf = (root: SessionItem): ZTreeNode<TreeData>[] =>
    workspace.sessions
      .filter((item) => item.parentId && item.id !== root.id && rootOf(item) === root.id)
      .sort(newest)
      .map((item, index, list) => sessionNode(item, index === list.length - 1));

  const sessionNode = (session: SessionItem, last = false): ZTreeNode<TreeData> => ({
    id: `session:${session.id}`,
    label: session.title,
    icon: session.parentId ? 'lucideGitFork' : 'lucideMessageSquare',
    children: session.parentId ? [] : forksOf(session),
    data: { kind: 'session', session, last },
  });

  // A new chat joins the list only after its first message; an empty one stays out even while it is open.
  const shown = (item: SessionItem) => !isEmptyDraft(item);
  // The chosen order; a new chat (no time yet) stays first when sorting by time.
  const ordered = (list: SessionItem[]) => {
    const sorted = [...list].sort(view.sortBy === 'name' ? (x, y) => x.title.localeCompare(y.title) : newest);
    return view.order === 'asc' ? sorted.reverse() : sorted;
  };
  // The first rows of a long list, then "Show more" (or every row and "Show less").
  const limited = <T>(key: string, list: T[], limit: number, row: (item: T) => ZTreeNode<TreeData>) => {
    if (list.length <= limit) return list.map(row);
    const open = showAll.includes(key);
    const more: ZTreeNode<TreeData> = {
      id: `more:${key}`,
      label: open ? 'Show less' : `Show ${list.length - limit} more`,
      isLeaf: true,
      children: [],
      data: { kind: 'more', group: key, open },
    };
    return [...(open ? list : list.slice(0, limit)).map(row), more];
  };

  const projectNode = (project: Project): ZTreeNode<TreeData> => ({
    id: `project:${project.id}`,
    label: project.name,
    icon: 'lucideFolder',
    data: { kind: 'project', path: project.path },
    children: limited(
      `project:${project.id}`,
      ordered(workspace.sessions.filter((item) => item.projectId === project.id && !item.parentId && shown(item))),
      LIST_LIMIT,
      (item) => sessionNode(item),
    ),
  });
  // A chat listed right under a heading: one line, no branch lines. `key` keeps ids unique when a chat is listed twice.
  const flatNode = (session: SessionItem, key = 'session'): ZTreeNode<TreeData> => ({
    id: `${key}:${session.id}`,
    label: session.title,
    isLeaf: true,
    children: [],
    data: { kind: 'session', session, flat: true },
  });
  // A heading and, unless it is folded, its rows. An empty heading still shows (its + button), without a fold arrow.
  const section = (key: string, label: string, rows: ZTreeNode<TreeData>[], always = false): ZTreeNode<TreeData>[] => {
    if (!rows.length && !always) return [];
    const head: ZTreeNode<TreeData> = { id: `section:${key}`, label, isLeaf: true, children: [], data: { kind: 'section', count: rows.length } };
    return [head, ...(collapsed.includes(key) ? [] : rows)];
  };

  // Pinned: chats and project folders kept at the top, in the order they were pinned. A pinned folder moves there
  // from Projects; a pinned chat leaves Recents and the date lists but stays in its project.
  const pinnedChats = pinned.sessions.map((id) => workspace.sessions.find((item) => item.id === id)).filter((item): item is SessionItem => Boolean(item && shown(item)));
  const visibleProjects = workspace.projects.filter((project) => project.id !== NO_PROJECT && !hiddenProjects.some((path) => samePath(path, project.path)));
  const pinnedProjects = pinned.projects.map((path) => visibleProjects.find((project) => samePath(project.path, path))).filter((item): item is Project => Boolean(item));
  const pins = section('pinned', 'Pinned', [...pinnedProjects.map(projectNode), ...pinnedChats.map((item) => flatNode(item, 'pin'))]);

  const chats = workspace.sessions.filter((item) => shown(item) && (view.groupBy === 'folder' || !item.parentId));
  const unpinned = chats.filter((item) => !isPinnedSession(item.id));
  if (view.groupBy === 'date') {
    // Today, Yesterday, Previous 7 days, Older: like the chat lists of Claude and ChatGPT.
    const day = 86_400_000;
    const midnight = new Date().setHours(0, 0, 0, 0);
    const bucket = (item: SessionItem) => {
      const time = item.updatedAt ? Date.parse(item.updatedAt) : Date.now();
      return time >= midnight ? 'today' : time >= midnight - day ? 'yesterday' : time >= midnight - 7 * day ? 'week' : 'older';
    };
    const labels = { today: 'Today', yesterday: 'Yesterday', week: 'Previous 7 days', older: 'Older' } as const;
    return [
      ...pins,
      ...(Object.keys(labels) as (keyof typeof labels)[]).flatMap((key) =>
        section(key, labels[key], limited(key, ordered(unpinned.filter((item) => bucket(item) === key)), LIST_LIMIT, (item) => flatNode(item, 'list'))),
      ),
    ];
  }
  if (view.groupBy === 'none') return [...pins, ...section('chats', 'Chats', limited('chats', ordered(unpinned), LIST_LIMIT * 3, (item) => flatNode(item, 'list')), true)];

  // By folder: Recents (newest chats of every project), This computer (chats outside any project), Projects.
  const latest = unpinned.filter((item) => !item.parentId).sort(newest);
  const computer = chats.filter((item) => item.projectId === NO_PROJECT && !item.parentId);
  const projects = visibleProjects.filter((project) => !pinnedProjects.includes(project));
  // Projects come most recently used first; sorting by name sorts them too.
  if (view.sortBy === 'name') projects.sort((x, y) => x.name.localeCompare(y.name));
  if (view.order === 'asc') projects.reverse();
  return [
    ...pins,
    ...section('recents', 'Recents', limited('recents', latest.slice(0, 15), LIST_LIMIT, (item) => flatNode(item, 'recent'))),
    ...section('computer', 'This computer', limited('computer', ordered(computer), LIST_LIMIT, (item) => flatNode(item)), true),
    ...section('projects', 'Projects', limited('projects', projects, PROJECT_LIMIT, projectNode), true),
  ];
}

// ---------- real history (Claude Code and Codex, read-only) ----------
export const chat = $state({ messages: mockMessages as Message[], loading: false, error: '', older: 0, loadingOlder: false });
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
  await Promise.all([restored, addedReady]);
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

/** The parent remembered for a sub chat, when that parent still exists (in the history, or as a chat of this app). */
function knownParent(id: string, ids: Set<string>): string | undefined {
  const parent = parents[id];
  if (!parent || parent === id) return undefined;
  return ids.has(parent) || workspace.sessions.some((item) => item.id === parent) ? parent : undefined;
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
    if (!projectIds.has(item.cwd)) {
      const id = `dir:${item.cwd}`;
      projectIds.set(item.cwd, id);
      projects.push({ id, name: item.cwd ? folderName(item.cwd) : 'Unknown', path: item.cwd });
    }
  }
  const sessions: SessionItem[] = found.map((item) => ({
    // A saved chat continued with another agent: which agents it has had, and their own conversation ids.
    ...(mixed[item.id] ?? {}),
    id: item.id,
    projectId: projectIds.get(item.cwd)!,
    title: item.title,
    agent: mixed[item.id]?.agents?.at(-1) ?? item.source,
    // Sonnet unless the chat was changed: the first model of the list is the biggest one.
    model: item.model ?? (item.source === 'claude' ? 'claude-sonnet-5-5' : MODELS[item.source][0].value),
    conversationModel: item.model ?? undefined,
    status: 'idle',
    parentId: item.parent_id && ids.has(item.parent_id) ? item.parent_id : knownParent(item.id, ids),
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
  // The open empty draft keeps its project too, so its row stays highlighted.
  const draft = workspace.sessions.find((item) => item.id === workspace.activeId && isEmptyDraft(item));
  if (draft) local.push(draft);
  // The open chat stays open: after F5 it is an imported session, after a send it may still be a local one.
  const keepActive = ids.has(workspace.activeId) || local.some((item) => item.id === workspace.activeId) ? workspace.activeId : undefined;
  // Folders added by hand stay even with no chat in them yet, right after "This computer".
  const added = addedProjects
    .filter((path) => !projects.some((project) => samePath(project.path, path)))
    .map((path) => ({ id: `dir:${path}`, name: folderName(path), path }));
  projects.unshift(noProjectRow(), ...added);
  sessions.sort((a, b) => (b.updatedAt ?? '').localeCompare(a.updatedAt ?? ''));

  workspace.projects = projects;
  workspace.sessions = [...sessions, ...local.filter((item) => projects.some((project) => project.id === item.projectId))];
  workspace.activeId = keepActive ?? sessions[0].id;
}

let transcriptToken = 0;

// ---------- transcripts: memory + IndexedDB cache, so opening a chat is instant ----------
// A transcript is valid while the session's index entry (last update + message count) is unchanged.
// `start` is where the first loaded message sits in the chat's file: above 0, older messages are still on disk.
type Transcript = { stamp: string; messages: Message[]; start: number };
const transcripts = new Map<string, Transcript>();
// The format version leads the stamp: a cached transcript from an older app version is read again (v2: token usage,
// v3: the file each tool call changed, so a file's +/- counts survive a reload; v4: Codex tokens and model).
const stampOf = (session: SessionItem) => `v4|${session.updatedAt ?? ''}|${session.messageCount ?? 0}`;
const MAX_SAVED = 12;
/** How many of the newest messages of a chat survive a reload in the saved copy. */
const SAVED_TAIL = 200;

/** Agents and per-agent ids of saved chats that switched agent (the chat files cannot hold them). */
const mixed: Record<string, Pick<SessionItem, 'agents' | 'agentIds'>> = {};
void getPref<typeof mixed>('chat.mixed').then((saved) => {
  if (saved) Object.assign(mixed, saved);
});
export function rememberMixed(session: SessionItem) {
  if (!session.path || !session.agents || session.agents.length < 2) return;
  mixed[session.id] = $state.snapshot({ agents: session.agents, agentIds: session.agentIds });
  setPref('chat.mixed', mixed);
}

/**
 * Turns of another agent in a saved chat (a Claude chat continued with Codex, say). The chat's file only holds
 * its own agent's turns, so these are kept here and merged back in by time when the chat is read again.
 */
const otherTurns: Record<string, Message[]> = {};
void getPref<Record<string, Message[]>>('chat.otherTurns').then((saved) => {
  if (saved) Object.assign(otherTurns, saved);
});
function keepOtherTurn(session: SessionItem, reply: Message) {
  const list = messagesOf(session.id);
  const index = list.indexOf(reply);
  const asked = index > 0 && list[index - 1].role === 'user' ? [list[index - 1]] : [];
  const kept = (otherTurns[session.id] ??= []);
  for (const message of [...asked, reply]) {
    if (!kept.some((item) => item.id === message.id)) kept.push($state.snapshot(message) as Message);
  }
  setPref('chat.otherTurns', $state.snapshot(otherTurns));
}

/**
 * The message that handed work out (a flow's steps, `@agent`) is never in the chat's file: the chat's agent only
 * speaks at the handback. Kept with the other turns, and saved again as its cards change, so the chat still shows
 * its Flow steps card once it is read back from the file.
 */
function keepHandout(mainId: string, asked: Message) {
  const kept = (otherTurns[mainId] ??= []);
  const copy = $state.snapshot(asked) as Message;
  const index = kept.findIndex((item) => item.id === asked.id);
  if (index >= 0) kept[index] = copy;
  else kept.push(copy);
  setPref('chat.otherTurns', $state.snapshot(otherTurns));
}

/** Messages shown per read of the chat's file: the newest ones on open, the ones before them when scrolling up. */
const PAGE_LIMIT = 80;

function toMessages(session: SessionItem, loaded: HistoryMessage[]): Message[] {
  return loaded.map((message) => ({
    ...(message.role === 'assistant' ? { agent: session.source } : {}),
    id: message.id,
    role: message.role,
    time: clock(message.time),
    at: message.time || undefined,
    // A prompt sent with the turns this agent missed shows only what the user typed.
    blocks: toBlocks(message.blocks).map((block) =>
      message.role === 'user' && block.kind === 'response' && block.text.startsWith('Earlier in this chat, other agents answered')
        ? { ...block, text: block.text.slice(block.text.indexOf('Now the user says:\n') + 'Now the user says:\n'.length) }
        : message.role === 'user' && block.kind === 'response' && block.text.startsWith(HANDBACK_HEAD)
          ? { ...block, text: '↩ The steps answered' }
          : block,
    ),
    // Tokens come from the transcript itself, so every reply shows them again when the chat is reopened.
    ...(message.usage
      ? { tokens: { input: message.usage.input, output: message.usage.output, cacheRead: message.usage.cache_read, cacheWrite: message.usage.cache_write }, model: message.model }
      : {}),
  }));
}

/** The file's messages minus the deleted ones, with the other agents' turns that fall in the time range `[lower, upper)`. */
function assemble(session: SessionItem, own: Message[], lower?: string, upper?: string): Message[] {
  const others = (otherTurns[session.id] ?? []).filter((message) => (!lower || (message.at ?? '') >= lower) && (!upper || (message.at ?? '') < upper));
  const gone = new Set(hidden[session.id] ?? []);
  const out = (message: Message) => gone.has(message.id) || gone.has(signature(message));
  const shown = gone.size ? own.filter((message) => !out(message)) : own;
  if (!others.length) return shown;
  return [...shown, ...others.filter((message) => !out(message))].sort((a, b) => (a.at ?? '').localeCompare(b.at ?? ''));
}

async function fetchTranscript(session: SessionItem): Promise<{ messages: Message[]; start: number }> {
  const page = await loadHistoryPage(session.source!, session.path!, null, PAGE_LIMIT);
  const own = toMessages(session, page.messages);
  return { messages: assemble(session, own, page.start > 0 ? own[0]?.at : undefined), start: page.start };
}

/** Reads the page of messages before the first one on screen and puts it in front. Returns how many it added. */
export async function loadOlder(session: SessionItem | undefined): Promise<number> {
  if (!session?.path || !session.source || chat.older <= 0 || chat.loadingOlder) return 0;
  chat.loadingOlder = true;
  try {
    const page = await loadHistoryPage(session.source, session.path, chat.older, PAGE_LIMIT);
    if (workspace.activeId !== session.id) return 0;
    const own = toMessages(session, page.messages);
    const added = assemble(session, own, page.start > 0 ? own[0]?.at : undefined, chat.messages[0]?.at);
    chat.messages = [...added, ...chat.messages];
    chat.older = page.start;
    const cached = transcripts.get(session.id);
    if (cached) transcripts.set(session.id, { ...cached, messages: [...added, ...cached.messages], start: page.start });
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
  // Cloning a long chat is slow, so it waits for an idle moment and keeps only the newest messages: F5 then shows
  // the end at once, and the full file is read again right after.
  const persist = () =>
    void getPref<string[]>('transcripts.keys').then((keys = []) => {
      const next = [session.id, ...keys.filter((key) => key !== session.id)];
      for (const old of next.slice(MAX_SAVED)) setPref(`transcript.${old}`, null);
      setPref('transcripts.keys', next.slice(0, MAX_SAVED));
      const tail = { stamp: '', messages: messages.slice(-SAVED_TAIL), start: start + Math.max(0, messages.length - SAVED_TAIL) };
      setPref(`transcript.${session.id}`, JSON.parse(JSON.stringify(tail)));
    });
  if (typeof requestIdleCallback === 'function') requestIdleCallback(persist, { timeout: 3000 });
  else setTimeout(persist, 500);
}

/** Warms the cache for a chat the user is likely to open next (row hover, most recent ones). */
const prefetching = new Set<string>();
export async function prefetchTranscript(session: SessionItem | undefined) {
  if (!session?.path || !session.source || transcripts.get(session.id)?.stamp === stampOf(session) || prefetching.has(session.id)) return;
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

/** Fills the chat with the transcript of a session: from cache when it is current, from disk otherwise. */
export async function openTranscript(session: SessionItem | undefined) {
  const token = ++transcriptToken;
  chat.error = '';
  chat.older = 0;
  if (!session) {
    chat.messages = [];
    return;
  }
  if (!session.path || !session.source) {
    chat.messages = localMessages[session.id] ?? (mockIds.has(session.id) ? mockMessages : []);
    return;
  }
  // Show the empty chat with its skeleton first and let it paint: reading the saved copy and mounting a long chat
  // takes a moment on the main thread, and the window must not look frozen meanwhile.
  chat.messages = [];
  chat.loading = true;
  await new Promise<void>((resolve) => requestAnimationFrame(() => setTimeout(resolve, 0)));
  if (token !== transcriptToken) return;
  let cached = transcripts.get(session.id);
  if (!cached) {
    const saved = await getPref<Transcript>(`transcript.${session.id}`);
    if (token !== transcriptToken) return;
    if (saved?.messages) transcripts.set(session.id, (cached = saved));
  }
  if (cached) {
    // Show what we have right away; a stale copy is replaced as soon as the fresh one arrives.
    chat.messages = withLive(session.id, cached.messages);
    chat.older = cached.start ?? 0;
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
