import { BUILT_IN_AGENTS, agentDisplayName } from '../../agents/provider-registry';
import type { ZIconName } from '@shival99/z-ui-svelte';
import { modelCatalog } from '../../agents/models.svelte';
import { inTauri, isPreviewCopy } from '../../app/platform/history';
import { type RunState, IDLE, type RunEvent, step } from '../runs/run-state';
import { type Message } from '../mock';
import { getPref, setPref } from '../../app/platform/prefs';
import { applyAgentEvent } from '../agent-reply';
import { answerCopilotPermission } from '../agent';
import { samePath } from '../../files/paths';
import { listen } from '../../app/platform/tauri-event';
import { lastChat } from './turns.svelte';
import { prewarmChat } from './run.svelte';
import { hidden } from './sending.svelte';
import { folderName } from './history.svelte';

// Mock workspace. Real data arrives from agentd (M2); the shapes follow crates/protocol.
export type Agent = 'claude' | 'codex' | 'antigravity' | 'opencode' | 'copilot' | 'custom';
export type Status = 'working' | 'idle' | 'waiting';
/** Reasoning effort and the approval modes shared by the agent CLIs. */
export type Effort =
  'off' | 'low' | 'medium' | 'high' | 'xhigh' | 'max' | 'ultra';
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
  /** A fork starts a conversation of its own: what was said before it, put before its first prompt, once. */
  forkContext?: string;
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
  /** How many times this agent's account had been switched when the conversation began (see account-epoch.svelte.ts). */
  agentEpoch?: number;
  /** Model used by the current CLI conversation; changing it starts a fresh conversation. */
  conversationModel?: string;
  /** Model used by each additional agent conversation in a mixed chat. */
  agentModels?: Partial<Record<Agent, string>>;
  /** Per agent: its own conversation id and whether it has one yet. `agentSession` / `agentStarted` belong to
   *  the chat's first agent; an agent the chat moved to later keeps its ids here. */
  agentIds?: Partial<Record<Agent, { id: string; started: boolean; epoch?: number }>>;
};

/**
 * Brand colour per agent. The OpenAI logo's path has a hard-coded `fill="#fff"`, so text colour alone
 * does not reach it: `[&_path]:fill-current` makes the path take the colour instead.
 */
export const AGENT_TEXT: Record<Agent, string> = {
  claude: 'text-orange-500',
  codex: 'text-emerald-500 [&_path]:fill-current',
  antigravity: 'text-sky-500',
  opencode:
    BUILT_IN_AGENTS.find((item) => item.id === 'opencode')?.tone ??
    'text-foreground',
  copilot: BUILT_IN_AGENTS.find((item) => item.id === 'copilot')?.tone ?? 'text-foreground',
  custom: 'text-violet-500',
};

/** Brand logo per agent (z-icon brand set). */
export const AGENT_ICON: Record<Agent, ZIconName> = {
  claude: 'brandClaude' as ZIconName,
  codex: 'brandOpenai' as ZIconName,
  antigravity: 'brandGemini' as ZIconName,
  opencode:
    BUILT_IN_AGENTS.find((item) => item.id === 'opencode')?.icon ??
    ('lucideTerminal' as ZIconName),
  copilot: BUILT_IN_AGENTS.find((item) => item.id === 'copilot')?.icon ?? ('lucideBot' as ZIconName),
  custom: 'lucideServer' as ZIconName,
};

export const AGENTS: { value: Agent; label: string }[] = [
  ...BUILT_IN_AGENTS.map(({ id, label }) => ({ value: id as Agent, label })),
];

/** Models per agent, from the catalogue in models.ts. */
export const MODELS = modelCatalog();

const mockWorkspace = {
  activeId: 's1',
  projects: [
    { id: 'p1', name: 'agentdeck', path: 'E:\\WORK\\PROJECTS\\agentdeck' },
    { id: 'p2', name: 'z-ui', path: 'E:\\WORK\\PROJECTS\\z-ui' },
    {
      id: 'p3',
      name: 'msg-new-template',
      path: 'E:\\WORK\\PROJECTS\\msg-new-template',
    },
  ] as Project[],
  sessions: [
    {
      id: 's1',
      projectId: 'p1',
      title: 'Build the settings panel',
      agent: 'claude',
      model: 'claude-sonnet-5-5',
      gitBranch: 'main',
      status: 'working',
    },
    {
      id: 's2',
      projectId: 'p1',
      title: 'Try it with Codex',
      agent: 'codex',
      agents: ['claude', 'codex'],
      model: 'gpt-6-luna',
      gitBranch: 'main',
      status: 'waiting',
      parentId: 's1',
    },
    {
      id: 's3',
      projectId: 'p1',
      title: 'Codex, second attempt',
      agent: 'codex',
      model: 'gpt-5.6-luna',
      gitBranch: 'main',
      status: 'idle',
      parentId: 's2',
    },
    {
      id: 's4',
      projectId: 'p1',
      title: 'Virtual chat list',
      agent: 'claude',
      agents: ['claude', 'codex', 'antigravity'],
      model: 'claude-opus-5-5',
      gitBranch: 'feat/chat-virtual',
      status: 'idle',
    },
    {
      id: 's5',
      projectId: 'p2',
      title: 'Svelte 5 z-tabs styling',
      agent: 'claude',
      model: 'claude-sonnet-5-5',
      gitBranch: 'main',
      status: 'idle',
    },
    {
      id: 's6',
      projectId: 'p2',
      title: 'Review the carousel API',
      agent: 'antigravity',
      model: 'gemini-4-argon',
      gitBranch: 'feat/carousel',
      status: 'idle',
    },
    {
      id: 's7',
      projectId: 'p3',
      title: 'Zoom check on mobile table',
      agent: 'claude',
      model: 'claude-haiku-5-5',
      gitBranch: 'fix/mobile-zoom',
      status: 'waiting',
    },
  ] as SessionItem[],
};

// In the desktop app start empty and show the real history; the mock only serves plain-browser dev.
export const workspace = $state(
  inTauri()
    ? {
        activeId: '',
        projects: [] as Project[],
        sessions: [] as SessionItem[],
      }
    : mockWorkspace,
);

// Mirror the open chat synchronously: IndexedDB writes are async, so a quick F5 after switching chats can otherwise
// restore the previous id. The Preview shares this origin and must not change the desktop window's selection.
const ACTIVE_SESSION_KEY = 'agentdeck.activeSessionId';
if (inTauri() && !isPreviewCopy()) {
  try {
    const saved = localStorage.getItem(ACTIVE_SESSION_KEY);
    if (saved) workspace.activeId = saved;
  } catch {
    // IndexedDB remains the fallback when local storage is unavailable.
  }
}

export const activeSession = () =>
  workspace.sessions.find((item) => item.id === workspace.activeId);
export const projectOf = (session: SessionItem) =>
  workspace.projects.find((item) => item.id === session.projectId);

export type Mention = {
  kind: 'session' | 'project' | 'ai';
  id: string;
  label: string;
};

// The run of each chat as the pure reducer (run-state.ts) sees it: every turn start, agent event, stop and permission answer
// goes through `advanceRun`. The reducer decides what counts as finished, stopped, crashed or waiting; `runOf` reads it.
export const runStates = $state<Record<string, RunState>>({});
export const runEpochs: Record<string, number> = {};
export const runOf = (sessionId: string): RunState => runStates[sessionId] ?? IDLE;
export function advanceRun(sessionId: string, event: RunEvent) {
  runStates[sessionId] = step(runOf(sessionId), event).state;
}

/** What was typed but not sent, per session: switch away and back and it is still there. */
export const drafts = $state<Record<string, string>>({});
export const draftMentions = $state<Record<string, Mention[]>>({});

/** An image waiting to be sent: a data URL, already scaled down. */
export type Attachment = {
  id: string;
  name: string;
  mime: string;
  src: string;
};
export const draftImages = $state<Record<string, Attachment[]>>({});

/** A document waiting to be sent (docx, pdf, xlsx, text, code...): it lives on disk and the agent is given its path. */
export type FileAttachment = {
  id: string;
  name: string;
  path: string;
  size: number;
  /** Where it came from, told to the agent beside the path (a selection of a file: which file). */
  note?: string;
};
export const draftFiles = $state<Record<string, FileAttachment[]>>({});

/** A part of a web page picked in the AgentDeck browser, waiting to go with the next message. */
export type PageElement = {
  id: string;
  tag: string;
  text: string;
  selector: string;
  html: string;
  /** Its place among its neighbours. */ around?: string;
  box: { x: number; y: number; w: number; h: number };
  style: Record<string, string>;
  url: string;
  title: string;
  /** What was written about it while marking up the page. */ note?: string;
  /** A PNG of the element alone (a data URL), taken when it was picked. */ picture?: string;
};
export const draftElements = $state<Record<string, PageElement[]>>({});
/** Messages of sessions that live only in this app (new chats), by session id. */
export const localMessages: Record<string, Message[]> = {};

/**
 * Which chat each sub chat belongs to (a flow step, a delegated task, a fork), by the sub chat's id. The chat files
 * do not hold it, so when the history is read again (a chat finishing, a reload) a sub chat would come back as a
 * chat of its own; this map puts it back under its parent.
 */
export const parents: Record<string, string> = {};
void getPref<Record<string, string>>('chat.parents').then((saved) => {
  if (saved) Object.assign(parents, saved, { ...parents });
});
export function rememberParents() {
  let changed = false;
  for (const item of workspace.sessions) {
    if (
      item.parentId &&
      item.parentId !== item.id &&
      parents[item.id] !== item.parentId
    ) {
      parents[item.id] = item.parentId;
      changed = true;
    }
  }
  // A chat that delegated work or ran a flow holds a card for each sub chat it started: that names the parent of
  // sub chats made before this map existed (their own files, once read back, do not say).
  for (const [parentId, list] of Object.entries(localMessages)) {
    for (const message of list) {
      for (const block of message.blocks ?? []) {
        if (
          block.kind === 'delegate' &&
          block.childId &&
          block.childId !== parentId &&
          parents[block.childId] === undefined
        ) {
          parents[block.childId] = parentId;
          changed = true;
        }
      }
    }
  }
  if (changed) setPref('chat.parents', { ...parents });
}

/** Sessions that only exist in the app (drafts, new chats, live agent chats) and their messages. */
export function localSessions(): SessionItem[] {
  return workspace.sessions.filter(
    (item) =>
      !item.path &&
      (item.draft || localMessages[item.id]) &&
      !isEmptyDraft(item),
  );
}

/** Save the local sessions, their messages and the open session to IndexedDB. Call after any change. */
let persistTimer: ReturnType<typeof setTimeout> | undefined;
/** Saves the local chats. Copying every chat is heavy, so calls close together become one save a moment later. */
export function persistLocal() {
  if (persistTimer !== undefined) return;
  persistTimer = setTimeout(persistLocalNow, 600);
}
if (typeof window !== 'undefined') window.addEventListener('pagehide', () => persistTimer !== undefined && persistLocalNow());

function persistLocalNow() {
  clearTimeout(persistTimer);
  persistTimer = undefined;
  rememberParents();
  const sessions = localSessions();
  const messages = Object.fromEntries(
    sessions.map((item) => [item.id, localMessages[item.id] ?? []]),
  );
  // Plain copies: the store holds proxies that IndexedDB cannot store.
  setPref(
    'local.state',
    JSON.parse(
      JSON.stringify({
        sessions,
        messages,
        activeId: workspace.activeId,
      }),
    ),
  );
}

/** The Gemini placeholder became Antigravity: chats saved under the old name move to the new one. */
const renamedAgent = <T extends string | undefined>(agent: T): T =>
  (agent as string | undefined) === 'gemini' ? ('antigravity' as T) : agent;
function renamedSession(item: SessionItem): SessionItem {
  const moved = (map?: Record<string, unknown>) =>
    map && 'gemini' in map
      ? Object.fromEntries(
          Object.entries(map).map(([key, value]) => [
            key === 'gemini' ? 'antigravity' : key,
            value,
          ]),
        )
      : map;
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
const localReady = getPref<{
  sessions: SessionItem[];
  messages: Record<string, Message[]>;
  activeId?: string;
}>('local.state').then(
  (saved) => {
    if (!saved) return;
    // A reply that was still running when the app closed is not running any more: its open blocks are closed (as a
    // stopped turn closes them), and its chat is idle, or the send button would stay a stop button after a reload.
    for (const [id, list] of Object.entries(saved.messages ?? {}))
      localMessages[id] = list.map((message) => ({
        ...message,
        agent: renamedAgent(message.agent),
        live: false,
        blocks: [
          ...applyAgentEvent(message.blocks, { kind: 'error', message: 'stopped' }).blocks,
          // Said once, on the reply that was cut: the work may be half done, and "continue" picks it up.
          ...(message.live && message.role === 'assistant'
            ? [{ kind: 'response' as const, text: '> ↻ The app closed while this reply was being written. Send "continue" to pick it up.' }]
            : []),
        ].map((block) => {
          // A card still waiting when the app reloaded has nobody to answer it: it is closed, not Allow-able again.
          if (block.kind !== 'permission' || block.state !== 'pending') return block;
          // A Copilot turn holds its request open until it is answered: refuse it so the turn ends.
          if (message.agent === 'copilot') for (const request of block.requests) void answerCopilotPermission(request.id, false).catch(() => {});
          return { ...block, state: 'expired' as const };
        }),
      }));
    for (const savedItem of saved.sessions ?? []) {
      const item = { ...renamedSession(savedItem), status: 'idle' as const };
      // Empty new chats saved by an earlier version are not restored.
      if (isEmptyDraft(item)) continue;
      if (!workspace.sessions.some((existing) => existing.id === item.id))
        workspace.sessions.push(item);
    }
    if (saved.activeId && !workspace.activeId)
      workspace.activeId = saved.activeId;
  },
  () => {
    // Nothing saved, or no IndexedDB: start empty.
  },
);

// The two most recent chats (other than the open one) start their CLI a moment after start-up, so the first message to
// either does not wait for it. Bounded: the idle pool keeps only a few processes (warm.rs), and prewarmChat throttles.
void localReady.then(() =>
  setTimeout(() => {
    const recent = workspace.sessions
      .filter((item) => !item.draft && item.id !== workspace.activeId)
      .sort((a, b) => (b.updatedAt ?? '').localeCompare(a.updatedAt ?? ''))
      .slice(0, 2);
    for (const item of recent) prewarmChat(item.id);
  }, 3000),
);

// The open session is saved on its own, so switching sessions survives a reload even without a send.
const activeReady = getPref<string>('workspace.activeId').then(
  (id) => id ?? '',
  () => '',
);
// Set the open session only after its local messages are back, so the chat opens with them.
export const restored = Promise.all([localReady, activeReady]).then(([, id]) => {
  if (id && !workspace.activeId) workspace.activeId = id;
});
$effect.root(() => {
  $effect(() => {
    const id = workspace.activeId;
    if (!id) return;
    if (!isPreviewCopy()) {
      try {
        localStorage.setItem(ACTIVE_SESSION_KEY, id);
      } catch {
        // IndexedDB remains the fallback when local storage is unavailable.
      }
    }
    setPref('workspace.activeId', id);
  });
});
export const mockIds = new Set(mockWorkspace.sessions.map((item) => item.id));

/** Sessions that belong to no project: a plain chat with the computer. */
export const NO_PROJECT = 'none';

/** Projects removed from the sidebar, by folder path. Their chats stay on disk; the list just leaves them out. */
export const hiddenProjects = $state<string[]>([]);
void getPref<string[]>('sidebar.hiddenProjects').then((saved) => {
  if (Array.isArray(saved))
    hiddenProjects.push(
      ...saved.filter((path) => !hiddenProjects.includes(path)),
    );
});
export function hideProject(id: string) {
  const path = workspace.projects.find((project) => project.id === id)?.path;
  if (!path || hiddenProjects.includes(path)) return;
  hiddenProjects.push(path);
  setPref('sidebar.hiddenProjects', [...hiddenProjects]);
}
export const nextId = () =>
  `local-${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`;

/** The row for chats outside any project; always first in the list. */
export const noProjectRow = (): Project => ({
  id: NO_PROJECT,
  name: 'This computer',
  path: '',
});
function ensureNoProject() {
  if (!workspace.projects.some((item) => item.id === NO_PROJECT))
    workspace.projects.unshift(noProjectRow());
}
ensureNoProject();

export { samePath };

/** Folders added by hand (no chat history yet, or never used by an agent) and the folders the user trusts. */
export const addedProjects = $state<string[]>([]);
export const trustedFolders = $state<string[]>([]);
export const addedReady = Promise.all([
  getPref<string[]>('sidebar.addedProjects').then(
    (saved) => Array.isArray(saved) && addedProjects.push(...saved),
  ),
  getPref<string[]>('sidebar.trustedFolders').then(
    (saved) => Array.isArray(saved) && trustedFolders.push(...saved),
  ),
]).catch(() => undefined);

/** A folder agents already worked in (it has chats) counts as trusted: Claude and Codex asked about it then. */
export const isTrusted = (path: string) =>
  trustedFolders.some((item) => samePath(item, path)) ||
  workspace.sessions.some(
    (item) => item.path && samePath(projectOf(item)?.path ?? '', path),
  );

export function trustFolder(path: string) {
  if (trustedFolders.some((item) => samePath(item, path))) return;
  trustedFolders.push(path);
  setPref('sidebar.trustedFolders', [...trustedFolders]);
}

/** Chats (by id) and project folders (by path) pinned to the top of the sidebar. */
export const pinned = $state<{ sessions: string[]; projects: string[] }>({
  sessions: [],
  projects: [],
});
void getPref<{ sessions: string[]; projects: string[] }>('sidebar.pinned').then(
  (saved) => {
    if (Array.isArray(saved?.sessions)) pinned.sessions = saved.sessions;
    if (Array.isArray(saved?.projects)) pinned.projects = saved.projects;
  },
);
const savePins = () =>
  setPref('sidebar.pinned', {
    sessions: [...pinned.sessions],
    projects: [...pinned.projects],
  });
export const isPinnedSession = (id: string) => pinned.sessions.includes(id);
export const isPinnedProject = (path: string) =>
  Boolean(path) && pinned.projects.some((item) => samePath(item, path));
export function togglePinSession(id: string) {
  pinned.sessions = isPinnedSession(id)
    ? pinned.sessions.filter((item) => item !== id)
    : [...pinned.sessions, id];
  savePins();
}
export function togglePinProject(id: string) {
  const path = workspace.projects.find((project) => project.id === id)?.path;
  if (!path) return;
  pinned.projects = isPinnedProject(path)
    ? pinned.projects.filter((item) => !samePath(item, path))
    : [...pinned.projects, path];
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
  let project = workspace.projects.find(
    (item) => item.path && samePath(item.path, path),
  );
  if (!project) {
    project = { id: `dir:${path}`, name: folderName(path), path };
    // Right under "This computer", where it is seen.
    const at =
      workspace.projects.findIndex((item) => item.id === NO_PROJECT) + 1;
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
export function isEmptyDraft(item: SessionItem) {
  return item.draft && !localMessages[item.id]?.length;
}
/** Told which sessions went when empty drafts are dropped, so a tab left open on one can go too. */
let onDraftsDropped: ((ids: string[]) => void) | undefined;
export function watchDroppedDrafts(listener: (ids: string[]) => void) {
  onDraftsDropped = listener;
}
/** Whether the chat is a new one nobody wrote in yet: only such a chat can still be moved to another project. */
export const isUnstarted = (item: SessionItem | undefined) =>
  Boolean(item && isEmptyDraft(item));

/** Moves a new, unsent chat to another project (or "This computer"). A chat that has started keeps its folder. */
export function moveDraftToProject(sessionId: string, projectId: string) {
  const item = workspace.sessions.find((entry) => entry.id === sessionId);
  if (!item || !isEmptyDraft(item) || item.projectId === projectId) return;
  if (projectId === NO_PROJECT) ensureNoProject();
  // One unsent chat per project: the one already there is dropped in favour of this one (its text is lost only if empty).
  for (let index = workspace.sessions.length - 1; index >= 0; index--) {
    const other = workspace.sessions[index];
    if (
      other.id !== sessionId &&
      other.projectId === projectId &&
      isEmptyDraft(other) &&
      !drafts[other.id]?.trim()
    ) {
      workspace.sessions.splice(index, 1);
      onDraftsDropped?.([other.id]);
    }
  }
  item.projectId = projectId;
  item.gitBranch = undefined;
  persistLocal();
}

// A new chat starts on "This computer" unless a project is named: the project is picked under the message box.
export function newSession(projectId = NO_PROJECT) {
  if (projectId === NO_PROJECT) ensureNoProject();
  // One unsent new session at a time: all new-session entry points return to it, preserving typed text and context.
  const empty = workspace.sessions.find(isEmptyDraft);
  if (empty) {
    moveDraftToProject(empty.id, projectId);
    workspace.activeId = empty.id;
    return;
  }
  const base = workspace.sessions.find((item) => item.projectId === projectId);
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
  const last = lastChat;
  if (
    last.agent &&
    (MODELS[last.agent] ?? []).some((item) => item.value === last.model)
  ) {
    return { agent: last.agent, model: last.model };
  }
  return { agent: 'claude', model: 'claude-sonnet-5-5' };
}

/** A new chat outside any project. */
export function newChat() {
  newSession(NO_PROJECT);
}

// Keep the native tray menu in sync with the chats visible in the desktop app. The menu only carries a small recent
// list; selecting one sends its id back through the tray-action event and this module activates that chat.
let trayMenuUpdates = Promise.resolve();
if (inTauri()) {
  void listen<{ action: string; sessionId?: string }>(
    'tray-action',
    ({ payload }) => {
      if (payload.action === 'new-chat') {
        newSession(NO_PROJECT);
        window.dispatchEvent(new Event('agentdeck:new-chat-requested'));
      } else if (
        payload.action === 'open-session' &&
        payload.sessionId &&
        workspace.sessions.some((item) => item.id === payload.sessionId)
      ) {
        workspace.activeId = payload.sessionId;
      }
    },
  );

  $effect.root(() => {
    $effect(() => {
      const recent = workspace.sessions
        .filter((item) => !item.draft)
        .slice()
        .sort(
          (a, b) =>
            Date.parse(b.updatedAt ?? '') - Date.parse(a.updatedAt ?? ''),
        )
        .slice(0, 6)
        .map(({ id, title, agent }) => ({
          id,
          title,
          agent: agentDisplayName(agent),
        }));
      trayMenuUpdates = trayMenuUpdates
        .then(async () => {
          const { invoke } = await import('@tauri-apps/api/core');
          await invoke('tray_update_recent', { sessions: recent });
        })
        .catch(() => {});
    });
  });
}