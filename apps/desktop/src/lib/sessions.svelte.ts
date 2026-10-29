import { archiveTurn } from './archive';
import { samePath } from './paths';
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
import { logRequest, trackActivity } from './activity.svelte';
import { logTalk } from './activity.svelte';
import { guide } from './guide.svelte';
import { parseQuestions, QUESTION_RULE, type Question } from './questions';
import { demoRuns } from './demo-runs';
import { attachmentBlocks, splitAttachments } from './attachments';
import {
  deleteCommandImages,
  finishCommandImages,
  persistCommandImage,
  restoreCommandImages,
  stripCommandImageData,
} from './command-images';
import {
  delegateReady,
  listenDelegateAborts,
  listenDelegateCalls,
  enterWorkerSlot,
  releaseWorkerSlot,
  listenWorkerSlots,
  listenAgentEvents,
  answerCopilotPermission,
  replyDelegateCall,
  prewarmAgent,
  saveAttachment,
  injectAgentMessage,
  sendAgentTurn,
  sendCustomTurn,
  stopAgentTurn,
  type AgentEvent,
  type DelegateCall,
} from './agent';
import { houseStyleHint } from './house-style';
import { IDLE, step, type RunEvent, type RunState } from './run-state';
import { memoryHint, readProjectMemory } from './project-memory';
import { replyStyle } from './reply-style.svelte';
import {
  agentdeckLead,
  argsOfCall,
  DELEGATE_HINT,
  parseHandoff,
  reportDigest,
  findAgent,
  formatAgents,
  privateContentIn,
  formatTaskResult,
  handoffOf,
  isTaskId,
  kindOfCommand,
  MAX_WORKERS,
  newTaskId,
  parseDelegateArgs,
  pickTask,
  reasonLine,
  REPORT_RULE,
  resolveModel,
  startedText,
  toolKindOf,
  type AgentRow,
  type HandoffReport,
  type TaskRecord,
  type Verdict,
} from './delegate-task';
import { argOf, elapsed, toolOf } from './blocks';
import { addUsage, applyAgentEvent } from './agent-reply';
import { getPref, setPref } from './prefs';
import { estimateCost, recordUsage } from './usage';
import { automation, COMPRESS_AT, contextWindow } from './automation.svelte';
import { logError } from './error-log.svelte';
import { startPageMemoryBridge } from './page-memory-bridge';
import { inboxOf, inboxText, MAX_TEAM_TURNS, parseTeamReply, planFor, teamFinished, teamText, type TeamMember, type TeamMessage, type TeamState } from './team';
import {
  customModelId,
  customProviderName,
  modelCatalog,
  modelLabel,
  parseCustomModel,
} from './models.svelte';
import { agentHasRoom, connectors, limitResetAt, roomLine } from './connectors.svelte';
import { classifyFailure, continuePrompt, planRecovery, recoveryNote, type FailureKind, type Recovery } from './turn-failure';
import { parseResetAt } from './limit-reset';
import { requestSystemNotificationPermission, sendSystemNotification } from './system-notifications';
import { dequeue, dequeueAll, enqueue } from './queue';
import { accountEpoch } from './account-epoch.svelte';
import { needsContext } from './context-need';
import { sessionMark } from './chat-dividers';
import { HANDBACK_HEAD, isHandbackPrompt, stripLead } from './handback';
import { joinContinuations } from './continuations';
import {
  allPresets,
  inputsOf,
  normalizeGraph,
  presetOf,
  stepGroups,
  type WorkflowPreset,
  type WorkflowStep,
} from './workflows.svelte';
import { agentDisplayName, BUILT_IN_AGENTS, customProviderOf } from './provider-registry';
import { listen } from './tauri-event';

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

/** Track turn execution state: running vs completed. */
export interface TurnState {
  turnId?: string;
  running: boolean;
  streamingText: boolean;
  lastEventTime: number;
}

/** Classify message: related (inject/queue into running turn) or independent (new turn). */
export function classifyMessage(
  turnState: TurnState,
  midTurnInjection: boolean,
): 'related' | 'independent' {
  // Turn not running: independent (new turn).
  if (!turnState.running) return 'independent';
  // Provider doesn't support mid-turn injection: independent (will queue or new turn in CLI).
  if (!midTurnInjection) return 'independent';
  // Sent during text streaming or while tool runs: related (inject into current turn).
  if (turnState.streamingText) return 'related';
  // Otherwise (turn running, injection supported, not streaming): related.
  return 'related';
}

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
const runEpochs: Record<string, number> = {};
export const runOf = (sessionId: string): RunState => runStates[sessionId] ?? IDLE;
function advanceRun(sessionId: string, event: RunEvent) {
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
function localSessions(): SessionItem[] {
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
const nextId = () =>
  `local-${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`;

/** The row for chats outside any project; always first in the list. */
const noProjectRow = (): Project => ({
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
const addedReady = Promise.all([
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
function isEmptyDraft(item: SessionItem) {
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
  if (
    lastChat.agent &&
    (MODELS[lastChat.agent] ?? []).some((item) => item.value === lastChat.model)
  ) {
    return { agent: lastChat.agent, model: lastChat.model };
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

/**
 * Said to the agent when a message is about running or opening something. Agents follow the project's own notes
 * (a skill that opens the user's Chrome, a shell command that starts the server), and a server started in the agent's
 * own shell dies when the command ends, which leaves the Preview on a refused connection. These tools do not.
 */
const AGENTDECK_HINT =
  '[AgentDeck] You run inside AgentDeck. To start a dev server or open a page, use the agentdeck tools: browser_serve (starts the server so it keeps running after your turn, and opens it in the Preview) or browser_open for an address that already answers. Then use browser_screenshot, browser_console and browser_network to look at it. Do not start servers in your own shell and do not open the system browser or Chrome: that server would stop when your command ends, and the user is looking at the AgentDeck Preview.';

/**
 * Chats (and the agent in them) already told how to delegate. Kept between runs of the app: a resumed conversation
 * still has the hint in its history, so sending it again after every restart only costs tokens.
 */
/** What each chat was last told about handing work out (auto-orchestrate on or off), this run of the app. */
const orchestrateTold = new Map<string, boolean>();
const NO_DELEGATE_HINT =
  'Auto-orchestration is off in AgentDeck: do the work yourself. Call delegate_task only when the user asks you to hand work to another agent.';
const delegateHinted = new Set<string>();
void getPref<string[]>('chat.delegateHinted').then((saved) => {
  for (const key of saved ?? []) delegateHinted.add(key);
});
function rememberDelegateHint(key: string) {
  delegateHinted.add(key);
  setPref('chat.delegateHinted', [...delegateHinted].slice(-500));
}

/** Agents whose own system prompt already says how to use the Preview (claude.rs, codex.rs, custom_chat.rs). */
const KNOWS_PREVIEW = new Set<Agent>(['claude', 'codex', 'custom']);

/** The last thing asked in each chat: "open it", "run the app" decides whether a served page opens by itself. */
const lastPrompt: Record<string, string> = {};
const OPEN_WORDS =
  /\b(open|opened|preview|browser|run|running|start|started|serve)\b|mở|chạy|xem thử/i;

/** Pick the requested framework's address when a turn starts more than one local dev server. */
function previewUrl(text: string, prompt: string): string | undefined {
  const wantsSvelte = /\bsvelte(?:\s*5)?\b|z-ui-svelte/i.test(
    `${prompt}\n${text}`,
  );
  const urls = [
    ...text.matchAll(
      /https?:\/\/(?:localhost|127\.0\.0\.1|\[::1\])(?::\d+)?[^\s`'")\]>]*/gi,
    ),
  ].map((match) => ({
    url: match[0],
    at: match.index ?? 0,
  }));
  const unique = urls.filter(
    (item, index) =>
      urls.findIndex((other) => other.url === item.url) === index,
  );
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
  // A flow's steps and the turn that writes its final answer only report: an address they mention (an API a search
  // found, a URL in a README) is not a page the user asked to see, and opening it showed a refused connection.
  if (
    isWorkerChat(sessionId) ||
    delegateRuns.some((run) => run.main === sessionId && !run.endedAt)
  )
    return;
  const text = reply.blocks
    .map((block) =>
      block.kind === 'response'
        ? block.text
        : block.kind === 'command'
          ? `${block.command}\n${block.output}`
          : '',
    )
    .join('\n');
  const asked = OPEN_WORDS.test(lastPrompt[sessionId] ?? '');
  // You asked for it, or the answer says the page is open or running.
  if (!asked && !OPEN_WORDS.test(text)) return;
  // Without that ask, only an address a command printed (a server that was started) counts, not one the prose mentions.
  const source = asked
    ? text
    : reply.blocks
        .map((block) =>
          block.kind === 'command' ? `${block.command}\n${block.output}` : '',
        )
        .join('\n');
  const found = previewUrl(source, lastPrompt[sessionId] ?? '');
  // An agent that opened the page itself (browser_open, browser_serve) has put it where it wanted: nothing to add.
  const opened = reply.blocks.some(
    (block) =>
      block.kind === 'command' && /browser_(open|serve)/.test(block.command),
  );
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
  for (const match of item.html.matchAll(
    /\b(?:id|aria-label|title|placeholder|formcontrolname|name|data-[\w-]+|routerlink|ng-reflect-[\w-]+)="([^"]{2,60})"/gi,
  ))
    add(match[1]);
  const generic =
    /^(flex|grid|block|inline|hidden|relative|absolute|items-|justify-|gap-|p[xytblr]?-|m[xytblr]?-|w-|h-|min-|max-|text-|bg-|border|rounded|shadow|font-|size-|z-|overflow|cursor|transition|hover:|focus:|disabled:|ring|outline|opacity|sr-only|truncate|shrink|grow|self-|col-|row-|space-|divide-)/;
  for (const match of item.html.matchAll(/\bclass="([^"]*)"/gi))
    for (const name of match[1].split(/\s+/))
      if (name && !generic.test(name) && !/^\d/.test(name)) add(name, 4);
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
      `Styles: ${Object.entries(item.style)
        .map(([key, value]) => `${key}: ${value}`)
        .join('; ')}`,
      '```html',
      item.html,
      '```',
      ...(item.around
        ? ['Its place on the page:', '```', item.around, '```']
        : []),
      ...(hints.length
        ? [
            `To find its source in the project, search the files for these (they come from this element's text, ids and classes): ${hints.map((hint) => `"${hint}"`).join(', ')}`,
          ]
        : []),
    ].join('\n');
  });
  const guide =
    'These elements are what the request is about. Change the code that renders them, in the place they are rendered: find it by searching the project for the words listed, read the file around the match, and put the change exactly where the element sits (see "Its place on the page"), written like the markup around it. Do not add it anywhere else.';
  return `${text || 'Look at the element(s) below.'}\n\n${guide}\n\n${parts.join('\n\n')}`;
}

/** How the Composer's permission setting maps onto the CLI's `--permission-mode`; "ask" passes none. */
/**
 * The agent, model and effort of the last message the user actually sent. A new chat starts from them instead of a
 * fixed default. Opening a chat to read it never changes this: only sending does.
 */
export const lastChat = $state<
  { agent: Agent; model: string; effort: Effort } | { agent: null }
>({ agent: null });
void getPref<{ agent: Agent; model: string; effort: Effort }>(
  'composer.lastChat',
).then((saved) => {
  if (saved?.agent && saved.model) Object.assign(lastChat, saved);
});
function rememberLastChat(agent: Agent, model: string | null, effort: Effort) {
  if (!model) return;
  const saved = { agent, model, effort };
  Object.assign(lastChat, saved);
  setPref('composer.lastChat', saved);
}

// The app's own names: each agent's adapter maps them onto its CLI (crates/adapters/src/agent/permission.rs).
const PERMISSION_MODE: Record<Permission, string> = {
  ask: 'ask',
  edits: 'edits',
  plan: 'plan',
  auto: 'auto',
  yolo: 'yolo',
};
/** The permission setting of the Composer, read when a turn starts. */
export const turnOptions = $state<{ permission: Permission; effort: Effort }>({
  permission: 'auto',
  effort: 'medium',
});

/** The run of each chat that is current: events of an older (stopped) run are ignored. */
const turns: Record<string, string> = {};

/** Messages typed while the agent was still busy, waiting their turn. */
export type Queued = {
  text: string;
  images: Attachment[];
  elements: PageElement[];
  mentions?: Mention[];
  files?: FileAttachment[];
};
export const queued = $state<Record<string, Queued[]>>({});

/** An agent whose account is being switched: a send to it waits, since the CLI it would start keeps the old sign-in
 *  until the switch is done (see account-epoch.svelte.ts). The sends are kept whole (prompt, preface, silent turn) and
 *  made, in order, when the switch ends. */
const switchingAgent = (agent: string) => Boolean(connectors.busy[`switch:${agent}`]);
const afterSwitch = new Map<string, (() => void)[]>();
let switchingBefore = new Set<string>();
$effect.root(() => {
  $effect(() => {
    const now = new Set(
      Object.keys(connectors.busy)
        .filter((key) => key.startsWith('switch:'))
        .map((key) => key.slice('switch:'.length)),
    );
    for (const agent of switchingBefore) {
      if (now.has(agent)) continue;
      const waiting = afterSwitch.get(agent) ?? [];
      afterSwitch.delete(agent);
      for (const go of waiting) go();
    }
    switchingBefore = now;
  });
});

type SessionWorkBatch = { failed: boolean };
const sessionWorkBatches: Record<string, SessionWorkBatch> = {};

function beginSessionWork(sessionId: string) {
  sessionWorkBatches[sessionId] ??= { failed: false };
}

function failSessionWork(sessionId: string) {
  const batch = sessionWorkBatches[sessionId];
  if (batch) batch.failed = true;
}

function hasOutstandingSessionWork(sessionId: string): boolean {
  if (
    replies[sessionId] ||
    hasSideTurn(sessionId) ||
    renderWaiters[sessionId] ||
    needsUserInput(sessionId)
  )
    return true;
  if (delegateRuns.some((run) => run.main === sessionId && !run.endedAt))
    return true;
  if (
    [...taskRuns.keys()].some(
      (taskId) => delegateTasks[taskId]?.caller === sessionId,
    )
  )
    return true;
  return false;
}

function notifySessionWorkDone(sessionId: string) {
  const batch = sessionWorkBatches[sessionId];
  if (!batch) return;
  delete sessionWorkBatches[sessionId];
  const session = workspace.sessions.find((item) => item.id === sessionId);
  if (!session) return;
  // "Notify when done" on this chat: always. Otherwise only with notifications on, and not for the chat the user is
  // looking at (the answer is in front of them, a notification would only interrupt).
  if (!notifyDone[sessionId]) {
    if (!connectors.alerts.system) return;
    if (typeof document !== 'undefined' && document.hasFocus() && workspace.activeId === sessionId) return;
  }
  const last = [...messagesOf(sessionId)].reverse().find((message) => message.role === 'assistant');
  const answer = last?.blocks.flatMap((block) => (block.kind === 'response' ? [block.text] : [])).join(' ') ?? '';
  const title = batch.failed ? `${session.title}: failed` : `${session.title}: done`;
  const body = answer.trim()
    ? clipText(answer, 180)
    : batch.failed
      ? 'At least one task failed. Nothing else is running.'
      : 'All tasks are done and nothing remains queued.';
  void sendSystemNotification(title, body);
}

/** Chats with "Notify when done" on, by id: kept apart from the chats, which may be read again from the CLIs' files. */
const notifyDone = $state<Record<string, true>>({});
void getPref<Record<string, true>>('chat.notifyDone').then((saved) => {
  if (saved) Object.assign(notifyDone, saved);
});
export const notifiesWhenDone = (sessionId: string) => Boolean(notifyDone[sessionId]);

/** Turns "Notify when done" on or off for a chat (asks for the system permission the first time). */
export function toggleNotifyDone(sessionId: string) {
  if (notifyDone[sessionId]) delete notifyDone[sessionId];
  else {
    notifyDone[sessionId] = true;
    void requestSystemNotificationPermission();
  }
  setPref('chat.notifyDone', { ...notifyDone });
}

/** Whether a chat has a reply in progress (for the send / stop button). */
export const isRunning = (sessionId: string) =>
  workspace.sessions.find((item) => item.id === sessionId)?.status ===
    'working' || hasSideTurn(sessionId);

/** Stops the reply of a chat now. What was written so far stays. `quiet`: the stop is only to make room for a newer
 *  message (sent while the reply ran, an edit), not the user's Stop button: the reply does not say "You stopped after". */
export function stopTurn(sessionId: string, quiet = false) {
  // Stopped by the user: a retry or a hand-over waiting to start does not start.
  delete recoveries[sessionId];
  advanceRun(sessionId, { type: 'cancel' });
  for (const [id, side] of sideTurns) {
    if (side.main !== sessionId) continue;
    void stopAgentTurn(id, side.turn).catch(() => {});
    side.reply.live = false;
    sideTurns.delete(id);
  }
  // Work handed to other agents (a flow's steps, `@agent`) stops with the chat that handed it out, all at once.
  for (const run of delegateRuns) {
    if (run.main !== sessionId || run.endedAt || run.handback === 'running')
      continue;
    run.cancelled = true;
    for (const node of run.nodes) {
      if (node.state !== 'running') continue;
      const waiter = stepWaiters.get(node.childId);
      // A step that waits for the user's answers has no reply to stop: it ends here.
      if (!replies[node.childId] && waiter) {
        stepWaiters.delete(node.childId);
        askingCards.delete(node.childId);
        waiter('', 'stopped', {
          id: nextId(),
          role: 'assistant',
          time: '',
          blocks: [],
        });
      } else stopTurn(node.childId);
    }
  }
  // A team run of this chat: its members' turns stop too (the run is cancelled above, so none starts another).
  for (const state of Object.values(teamStates)) {
    if (state.main !== sessionId || state.endedAt) continue;
    for (const member of state.members) if (replies[member.childId]) stopTurn(member.childId);
  }
  if (!replies[sessionId]) {
    // Nothing is running any more but the chat still says so (a lost event): clear it, and close the reply that was
    // still open, so its thinking and text stop spinning.
    const open = liveReplies[sessionId];
    if (open?.live) {
      open.blocks = applyAgentEvent(open.blocks, { kind: 'error', message: 'stopped' }).blocks;
      open.live = false;
    }
    const stuck = workspace.sessions.find((item) => item.id === sessionId);
    if (stuck?.status === 'working')
      stuck.status = hasSideTurn(sessionId) ? 'working' : 'idle';
    return;
  }
  const turn = turns[sessionId];
  // Every agent and provider stops here, so every stopped reply says so, with how long it had run.
  const live = liveReplies[sessionId];
  if (live?.live && !quiet) live.stoppedAfter = Date.now() - (live.startedAt ?? Date.now());
  void stopAgentTurn(sessionId, turn).catch(() => {});
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
  if (session.model && list.some((item) => item.value === session.model))
    return session.model;
  const fallback = list[0]?.value ?? null;
  if (fallback) session.model = fallback;
  return fallback;
}

const agentLabel = (agent?: Agent, model?: string | null) =>
  (agent === 'custom' && customProviderName(model)) ||
  (agent === 'custom'
    ? 'Custom provider'
    : agentDisplayName(agent ?? 'claude'));

/**
 * A user message that is only the hand-back prompt (or, in a chat saved by an older version, its stand-in
 * "↩ Claude answered"): it is never shown or quoted.
 */
const hiddenPrompt = (message: Message) =>
  message.role === 'user' &&
  message.blocks.some(
    (block) =>
      block.kind === 'response' &&
      (isHandbackPrompt(block.text) ||
        /^↩ .{1,200} answered$/.test(block.text.trim())),
  );

/**
 * What the other agents of this chat said since this agent last spoke, as text to put before the prompt.
 * Each agent keeps its own conversation, so one that joins (or comes back to) a chat would not know the turns
 * it missed. `list` ends with the message being sent, unless `sent` is false (a hand-back turn has no message of its own:
 * `list` is then all that came before). The newest turns are kept when it is long.
 */
function missedTurns(
  session: SessionItem,
  list: Message[],
  sent = true,
): string {
  const before = sent ? list.slice(0, -1) : list;
  const writer = (message: Message) =>
    message.agent ?? session.source ?? session.agent;
  let start = 0;
  for (let index = before.length - 1; index >= 0; index--) {
    if (
      before[index].role === 'assistant' &&
      writer(before[index]) === session.agent
    ) {
      start = index + 1;
      break;
    }
  }
  const missed = before.slice(start);
  if (!missed.some((message) => message.role === 'assistant')) return '';
  const lines = missed
    .filter((message) => !hiddenPrompt(message))
    .map((message) => {
      const text = message.blocks
        .flatMap((block) =>
          block.kind === 'response'
            ? [block.text]
            : block.kind === 'command'
              ? [`(ran: ${block.command.slice(0, 160)})`]
              : [],
        )
        .join('\n')
        .trim();
      const cut = text.length > 2500 ? `${text.slice(0, 2500)}…` : text;
      return `${message.role === 'user' ? 'User' : agentLabel(writer(message), message.model)}: ${cut}`;
    });
  let digest = lines.join('\n\n');
  if (digest.length > 16000) digest = `…${digest.slice(-16000)}`;
  return `Earlier in this chat, other agents answered these turns (you were not part of them):\n\n${digest}\n\n---\nNow the user says:\n`;
}

/** The chats that ran a turn since the app started: the first turn of any other chat that resumes a conversation resumes a session from before. */
const touchedThisRun = new Set<string>();
/** Chats whose next turn starts another CLI conversation (an edited message): that turn carries a "New session" line. */
const pendingNew = new Set<string>();

/** When each chat's CLI was last asked to start ahead (by chat, agent and model), so typing does not ask on every key. */
const prewarmedAt = new Map<string, number>();


/** The conversation id a Claude chat takes after an account switch (by account epoch), so the CLI started ahead for it is the one used. */
const accountNext: Record<string, { epoch: number; id: string }> = {};
function accountConversationId(sessionId: string, epoch: number): string {
  const ready = accountNext[sessionId];
  if (ready?.epoch === epoch) return ready.id;
  const id = crypto.randomUUID();
  accountNext[sessionId] = { epoch, id };
  return id;
}

/**
 * Starts the CLI of a chat whose next message is being typed (Claude, Codex, Antigravity keep it open between turns,
 * see warm.rs), with what `startTurn` will send, so the first turn does not wait for it to load. Nothing is changed in
 * the chat, except that a new Claude chat gets the conversation id its first turn will use. A turn that will start a
 * new conversation (another model, another account) is left to start on its own.
 */
export function prewarmChat(sessionId: string) {
  const session = workspace.sessions.find((item) => item.id === sessionId);
  if (!session || !inTauri() || !['claude', 'codex', 'antigravity'].includes(session.agent) || isRunning(session.id)) return;
  const selectedModel = modelFor(session);
  const now = Date.now();
  const epoch = accountEpoch[session.agent] ?? 0;
  // The account is part of the throttle: a switch makes a new CLI worth starting at once.
  const throttle = `${session.id}|${session.agent}|${selectedModel}|${epoch}`;
  if (now - (prewarmedAt.get(throttle) ?? 0) < 60_000) return;
  prewarmedAt.set(throttle, now);
  const first = session.source ?? session.agents?.[0] ?? session.agent;
  let cliId: string | null;
  let resume: boolean;
  if (session.agent === first) {
    if (session.agentEpoch !== undefined && session.agentEpoch !== epoch) {
      // The account changed: the next turn starts a new conversation under it (see startTurn). Its CLI is started now,
      // with the id that turn will use, so the first message does not wait for the start.
      cliId = session.agent === 'claude' ? accountConversationId(session.id, epoch) : null;
      resume = false;
    } else if (session.conversationModel && session.conversationModel !== selectedModel && !liveModel(session.agent)) {
      // Another model starts a new conversation (see startTurn), which its CLI starts on its own when the message comes.
      cliId = null;
      resume = false;
    } else {
      const prefix = `${session.agent}:`;
      const fromId = session.agentStarted === false ? undefined : session.id.startsWith(prefix) ? session.id.slice(prefix.length) : undefined;
      if (!session.agentSession && !fromId && session.agent === 'claude') session.agentSession = crypto.randomUUID();
      cliId = session.agentSession ?? fromId ?? null;
      resume = cliId !== null && (session.agentStarted ?? !!session.path);
    }
  } else {
    const own = session.agentIds?.[session.agent];
    // A first turn of Claude here picks its id then; Codex and Antigravity name theirs, so they start without one.
    if (!own && session.agent === 'claude') return;
    if (own && own.epoch !== undefined && own.epoch !== epoch) return;
    if (session.agentModels?.[session.agent] && session.agentModels[session.agent] !== selectedModel && !liveModel(session.agent)) return;
    cliId = own?.id || null;
    resume = cliId !== null && Boolean(own?.started);
  }
  const cwd = workspace.projects.find((project) => project.id === session.projectId)?.path ?? null;
  void prewarmAgent({
    session: session.id,
    agent: session.agent,
    agentSession: cliId,
    resume,
    model: selectedModel,
    permissionMode: PERMISSION_MODE[turnOptions.permission],
    effort: (session.stepEffort ?? turnOptions.effort) as Effort,
    allowedTools: [...(approved[session.id] ?? [])],
    cwd,
  }).catch(() => {});
}

/**
 * Starts the CLI of the agent a chat would fall back to (Settings: keep working when an agent fails) while the chat's own
 * agent works, so a failure at a limit does not also wait for a cold start. Only Codex and Antigravity name their own
 * conversation, so only they can be started ahead without an id; Claude as a fallback starts when it is needed.
 */
function prewarmFallback(session: SessionItem) {
  if (!connectors.recovery.enabled || !inTauri()) return;
  const target = recoveryChain().find((agent) => agent !== session.agent && ['codex', 'antigravity'].includes(agent));
  const model = target ? MODELS[target as Agent]?.[0]?.value : undefined;
  if (!target || !model) return;
  const throttle = `${session.id}|${target}|${model}|fallback`;
  const now = Date.now();
  if (now - (prewarmedAt.get(throttle) ?? 0) < 300_000) return;
  prewarmedAt.set(throttle, now);
  const cwd = workspace.projects.find((project) => project.id === session.projectId)?.path ?? null;
  void prewarmAgent({
    session: session.id,
    agent: target,
    agentSession: null,
    resume: false,
    model,
    permissionMode: PERMISSION_MODE[turnOptions.permission],
    effort: (session.stepEffort ?? turnOptions.effort) as Effort,
    allowedTools: [...(approved[session.id] ?? [])],
    cwd,
  }).catch(() => {});
}

/** One CLI run (`claude -p`, `codex exec`) in `session`, with a fresh live reply. Images go to disk first so the CLI can open them. */
function startTurn(
  session: SessionItem,
  text: string,
  images: Attachment[] = [],
  allow: string[] = [],
  preface = '',
  files: FileAttachment[] = [],
  silent?: Message,
) {
  const documentRequest = text.match(/^\/document(?:\s+([\s\S]*))?/i);
  if (documentRequest) {
    guide.tasks[session.id] = {
      request: documentRequest[1]?.trim() || 'Create the requested document',
      status: 'working',
      progress: 'Starting document task',
      result: '',
    };
    guide.open = true;
  }
  if (images.length || !allow.length) lastPrompt[session.id] = text;
  // The CLI's own id for the conversation. Claude takes one we choose; Codex names its thread on the first turn
  // and reports it, so a new Codex chat starts without one.
  const first = session.source ?? session.agents?.[0] ?? session.agent;
  const prefix = `${session.agent}:`;
  let cliId: string | null;
  let resume: boolean;
  /** The conversation so far was put in `forkContext` for a new session (model or account change). */
  let digested = false;
  /** The turn starts another CLI conversation than the chat had (a model or account change, an edited message). */
  let conversationChanged = pendingNew.delete(session.id);
  const selectedModel = modelFor(session);
  rememberLastChat(
    session.agent,
    selectedModel,
    (session.stepEffort ?? turnOptions.effort) as Effort,
  );
  if (session.agent === first) {
    // A conversation whose last run never started is not resumed from its id (see handleAgentEvent).
    const fromId =
      session.agentStarted === false
        ? undefined
        : session.id.startsWith(prefix)
          ? session.id.slice(prefix.length)
          : undefined;
    const known = session.agentSession ?? fromId;
    cliId = known ?? (session.agent === 'claude' ? crypto.randomUUID() : null);
    const modelChanged = Boolean(
      session.conversationModel && session.conversationModel !== selectedModel && !liveModel(session.agent),
    );
    if (modelChanged) {
      // Codex and Antigravity name their new conversation themselves.
      cliId = null;
      session.agentStarted = false;
      session.forkContext = (session.forkContext ?? '') + handoffDigest(messagesOf(session.id), MODEL_LEAD);
      digested = true;
      conversationChanged = true;
    }
    // A conversation begun under another account starts a new session: resuming the old one stalls. What was said
    // so far goes with the first message, so the agent does not lose the work in progress.
    const epoch = accountEpoch[session.agent] ?? 0;
    const accountChanged = session.agentEpoch !== undefined && session.agentEpoch !== epoch;
    if (accountChanged) {
      cliId = session.agent === 'claude' ? accountConversationId(session.id, epoch) : null;
      session.agentStarted = false;
      session.agentSession = undefined;
      if (!digested) session.forkContext = (session.forkContext ?? '') + handoffDigest(messagesOf(session.id));
      digested = true;
      conversationChanged = true;
    }
    session.agentEpoch = epoch;
    if (cliId) session.agentSession = cliId;
    resume =
      cliId !== null &&
      !modelChanged &&
      !accountChanged &&
      (session.agentStarted ?? !!session.path);
    session.conversationModel = selectedModel ?? undefined;
  } else {
    // Another agent in this chat: its own conversation, started the first time it is used here. The entry is made on
    // that first turn even before the agent has an id (Codex names its thread later), so the account it began under
    // is known from the start.
    const epoch = accountEpoch[session.agent] ?? 0;
    const known = session.agentIds?.[session.agent];
    const own = known ?? ((session.agentIds ??= {})[session.agent] = { id: '', started: false, epoch });
    cliId = own.id || (session.agent === 'claude' ? crypto.randomUUID() : null);
    if (cliId && !own.id) own.id = cliId;
    const modelChanged = Boolean(
      session.agentModels?.[session.agent] &&
      session.agentModels[session.agent] !== selectedModel &&
      !liveModel(session.agent),
    );
    if (modelChanged) {
      cliId = null;
      session.forkContext = (session.forkContext ?? '') + handoffDigest(messagesOf(session.id), MODEL_LEAD);
      digested = true;
      conversationChanged = true;
    }
    // The same account rule as the chat's first agent: a new session, with the conversation so far.
    const accountChanged = own.epoch !== undefined && own.epoch !== epoch;
    if (accountChanged) {
      cliId = session.agent === 'claude' ? crypto.randomUUID() : null;
      own.started = false;
      if (cliId) own.id = cliId;
      if (!digested) session.forkContext = (session.forkContext ?? '') + handoffDigest(messagesOf(session.id));
      digested = true;
      conversationChanged = true;
    }
    own.epoch = epoch;
    resume = cliId !== null && !modelChanged && !accountChanged && own.started;
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
  if (!isWorkerChat(session.id)) prewarmFallback(session);
  const list = messagesOf(session.id);
  // A silent turn (the hand-back) continues the finished reply that handed work out: it is one message on screen.
  const host = silent ? list.at(-1) : undefined;
  const continues =
    !!host &&
    host.role === 'assistant' &&
    !host.live &&
    host.agent === session.agent;
  const reply: Message =
    continues && host
      ? host
      : {
          id: nextId(),
          role: 'assistant',
          time: clock(new Date().toISOString()),
          at: new Date().toISOString(),
          blocks: [],
          live: true,
          startedAt: Date.now(),
          agent: session.agent,
          model: selectedModel ?? session.model,
        };
  if (continues && host) {
    host.live = true;
    host.startedAt = Date.now() - (host.ms ?? 0);
    host.callOutput = 0;
  }
  // A silent turn (the hand-back) has no message of its own: its reply simply follows the message that handed work out.
  const context =
    // The whole conversation already goes with a new session (a model or account change): the turns of the other agents
    // are in it, so they are not added a second time.
    preface + (session.forkContext ?? '') + (digested ? '' : missedTurns(session, list, !silent));
  session.forkContext = undefined;
  const asked = silent ? undefined : list.at(-1);
  // The line that says a session was resumed, compacted or replaced goes above the user's message that starts it (or
  // above the reply, for the compaction, which has no message of the user). A silent turn has none.
  const mark = silent
    ? undefined
    : sessionMark({
        resume,
        changed: conversationChanged,
        compacting: text === COMPACT_COMMAND,
        firstThisRun: !touchedThisRun.has(session.id),
        hasHistory: list.some((message) => message !== asked && message !== reply),
        now: new Date(),
      });
  touchedThisRun.add(session.id);
  if (!continues) list.push(reply);
  if (mark) (asked?.role === 'user' ? asked : list[list.length - 1]).mark = mark;
  liveReplies[session.id] = list[list.length - 1]; // the stored (reactive) object
  if (asked?.role === 'user') liveAsks[session.id] = asked;
  if (silent) {
    liveHandouts[session.id] = silent;
    delete liveAsks[session.id];
  } else delete liveHandouts[session.id];
  replies[session.id] = reply.id;
  session.status = 'working';
  const epoch = (runEpochs[session.id] = (runEpochs[session.id] ?? 0) + 1);
  advanceRun(session.id, { type: 'spawn', epoch });
  advanceRun(session.id, { type: 'turnStarted', epoch });
  const cwd =
    workspace.projects.find((project) => project.id === session.projectId)
      ?.path ?? null;
  void (async () => {
    const paths = await Promise.all(
      images.map((image) =>
        saveAttachment(image.mime, image.src.slice(image.src.indexOf(',') + 1)),
      ),
    );
    // A message that starts with `/name` is a command: its text goes to the agent, the chat keeps what was typed.
    const body = isWorkerChat(session.id) ? text : await expandSlash(text, session.agent, cwd);
    if (session.agent === 'custom') {
      // A custom provider has no CLI to keep the conversation: the whole chat goes along with every turn.
      const target = parseCustomModel(selectedModel);
      const provider = connectors.custom.find(
        (item) => item.id === target?.providerId,
      );
      if (!target || !provider)
        throw new Error(
          'This provider was removed or turned off. Pick another model.',
        );
      const spoken = (message: Message) =>
        message.blocks
          .flatMap((block) => (block.kind === 'response' ? [block.text] : []))
          .join('\n')
          .trim();
      const history = list
        .slice(0, silent ? -1 : -2)
        .filter((message) => !hiddenPrompt(message))
        // A free-tier provider only gets what the user wrote and what restricted providers answered: the other agents'
        // replies are about the project, and carry its code.
        .filter((message) => !provider.restricted || message.role === 'user' || customProviderOf(message.model)?.restricted)
        .map((message) => ({
          role: message.role,
          content: spoken(message),
        }))
        .filter((message) => message.content);
      await sendCustomTurn({
        session: session.id,
        turn,
        kind: provider.kind,
        baseUrl: provider.baseUrl,
        apiKey: provider.apiKey || null,
        model: target.model,
        messages: [
          ...history,
          {
            role: 'user',
            content: body,
            images: images.map((image) => image.src),
            files: files.map((file) => file.path),
          },
        ],
        permissionMode: PERMISSION_MODE[turnOptions.permission],
        allowedTools: [...new Set([...(approved[session.id] ?? []), ...allow])],
        cwd: provider.restricted ? null : cwd,
        effort: (session.stepEffort ?? turnOptions.effort) as Effort,
        restricted: Boolean(provider.restricted),
      });
      return;
    }
    // A flow's step is not told it may open pages: it reports back, the user did not ask to see anything.
    // Agents told about the Preview in their system prompt are not told again in every message that says "run" or "open".
    const openHint =
      !isWorkerChat(session.id) &&
      !KNOWS_PREVIEW.has(session.agent) &&
      OPEN_WORDS.test(body)
        ? AGENTDECK_HINT
        : '';
    // How to delegate (DELEGATE_HINT): said when the conversation starts, and once for a chat this app never told (an
    // imported one), not with every turn: the agent keeps it in its history. Sub chats (workers, flow steps) do not delegate.
    const hintKey = `${session.id}|${session.agent}`;
    // Auto-orchestrate (model popover): when it is off, the agent is told once not to hand work out by itself; when it
    // is switched on again, the delegation note is given again.
    const told = orchestrateTold.get(hintKey);
    const orchestrate = automation.orchestrate;
    const delegateHint = isWorkerChat(session.id)
      ? ''
      : orchestrate
        ? !resume || !delegateHinted.has(hintKey) || told === false
          ? DELEGATE_HINT
          : ''
        : told !== false
          ? NO_DELEGATE_HINT
          : '';
    if (!isWorkerChat(session.id)) orchestrateTold.set(hintKey, orchestrate);
    if (delegateHint === DELEGATE_HINT) rememberDelegateHint(hintKey);
    // The reply conventions (house-style.ts): only when the conversation starts, the agent keeps them in its history.
    const styleHint = !resume && !isWorkerChat(session.id) ? houseStyleHint(replyStyle.houseStyle, replyStyle.houseStyleText) : '';
    // The project's memory (project-memory.ts): read from its file when a conversation starts, so every agent of the
    // project begins with what earlier ones learned.
    const memoryNote = !resume && !isWorkerChat(session.id) && cwd && replyStyle.projectMemory ? memoryHint(true, cwd, await readProjectMemory(cwd)) : '';
    const hint = agentdeckLead(openHint, delegateHint, styleHint, memoryNote);
    // Pictures and documents are on disk: the prompt names their paths, which the agent opens with its own tools.
    const attached =
      (paths.length
        ? `Attached image${paths.length > 1 ? 's' : ''} (open with the Read tool):\n${paths.map((path) => `- ${path}`).join('\n')}`
        : '') +
      (files.length
        ? `${paths.length ? '\n\n' : ''}Attached file${files.length > 1 ? 's' : ''} (open with the Read tool or a shell command):\n${files.map((file) => `- ${file.path}${file.note ? ` (${file.note})` : ''}`).join('\n')}`
        : '');
    // `/compact` (auto-compress) goes as it is: anything before it would make it a plain message.
    const prompt = text === COMPACT_COMMAND
      ? text
      : hint +
      context +
      (attached
        ? `${body || (files.length && !paths.length ? 'Look at the attached file.' : 'Look at the attached image.')}\n\n${attached}`
        : body);
    logRequest(
      session.id,
      session.title,
      workspace.projects.find((project) => project.id === session.projectId)?.name ?? '',
      agentLabel(session.agent, selectedModel ?? undefined),
      selectedModel,
      [
        `Effort: ${session.stepEffort ?? turnOptions.effort}`,
        `Permission: ${turnOptions.permission}`,
        `Conversation: ${resume ? `continues ${cliId}` : cliId ? `new (${cliId})` : 'new'}`,
        `Folder: ${cwd ?? 'none'}`,
        `Message: ${prompt.length} characters${images.length ? `, ${images.length} picture(s)` : ''}${files.length ? `, ${files.length} file(s)` : ''}`,
      ].join('\n'),
      session.agent,
    );
    await sendAgentTurn({
      session: session.id,
      turn,
      agent: session.agent,
      prompt,
      keepOpen: !isWorkerChat(session.id),
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
    handleAgentEvent(
      session.id,
      { kind: 'error', message: String(error) },
      turn,
    );
  });
}

/**
 * The user's answer to an approval card. "Allow" and "Always allow" let the CLI run the refused tools and
 * continue the work in a new turn; "Deny" just closes the card.
 */
export function answerPermission(
  messageId: string,
  blockIndex: number,
  answer: 'once' | 'always' | 'deny',
) {
  const session = activeSession();
  const block = chat.messages.find((message) => message.id === messageId)
    ?.blocks[blockIndex];
  // A Copilot turn is still running while its card waits: the answer goes to the SDK request, not to a new turn.
  const waiting = session?.agent === 'copilot';
  if (
    !session ||
    block?.kind !== 'permission' ||
    block.state !== 'pending' ||
    (!waiting && replies[session.id])
  )
    return;
  for (let index = 0; index < block.requests.length; index++) advanceRun(session.id, { type: 'waitEnd', kind: 'approval' });
  const entries = block.requests.map((request) => request.allow);
  if (answer === 'always') {
    approved[session.id] = [
      ...new Set([...(approved[session.id] ?? []), ...entries]),
    ];
    setPref('permissions.allowed', JSON.parse(JSON.stringify(approved)));
  }
  if (waiting) {
    // The card shows the answer at once; if the SDK no longer waits for it (the turn was gone), it says so instead.
    block.state = answer === 'deny' ? 'denied' : 'allowed';
    persistLocal();
    void Promise.all(block.requests.map((request) => answerCopilotPermission(request.id, answer !== 'deny')))
      .catch(() => {
        block.state = 'expired';
        persistLocal();
      });
    if (answer === 'deny') sendNextQueued(session.id);
    return;
  }
  if (answer === 'deny') {
    block.state = 'denied';
    persistLocal();
    sendNextQueued(session.id);
    return;
  }
  block.state = 'allowed';
  persistLocal();
  startTurn(
    session,
    'Permission granted. Please continue with what you were doing.',
    [],
    entries,
  );
}

/** Adds the typed message to the open session. The first message turns a draft into a real session. */
/** The assistant message each live turn is writing into, per session. */
const replies: Record<string, string> = {};
/** The reply object of each running turn. */
const liveReplies: Record<string, Message> = {};
/** The message that started each running turn. Reactive, so the Tasks panel can show the prompt while the reply runs. */
export const liveAsks = $state<Record<string, Message>>({});
/** For a running hand-back turn (it has no message of its own): the message whose delegate cards it answers. */
const liveHandouts: Record<string, Message> = {};
/** For a hand-back that answers no message (a background task's report): its reply just follows the chat's last message. */
const NO_HANDOUT: Message = {
  id: 'no-handout',
  role: 'user',
  time: '',
  blocks: [],
};

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
  const handout = liveHandouts[sessionId];
  if (reply && handout === NO_HANDOUT)
    return list.includes(reply) ? list : [...list, reply];
  if (reply && handout && !list.includes(reply)) {
    // The file may already hold part of the reply, behind the message that handed work out: that is replaced by the live one.
    const at = list.findLastIndex((message) => message.id === handout.id);
    return at >= 0
      ? [...list.slice(0, at), handout, reply]
      : [...list, handout, reply];
  }
  const asked = liveAsks[sessionId];
  if (!reply || !asked || list.includes(reply)) return list;
  const want = askText(asked);
  let cut = list.length;
  // Only the last few can be this turn: an older message with the same words is another turn.
  for (
    let index = list.length - 1;
    index >= Math.max(0, list.length - 3);
    index--
  ) {
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

/** What the latest user turn still waits for: a permission request, or questions the agent asked. */
export function pendingInput(sessionId: string): 'permission' | 'question' | null {
  const list = messagesOf(sessionId);
  const messages =
    liveReplies[sessionId] && !list.includes(liveReplies[sessionId])
      ? [...list, liveReplies[sessionId]]
      : list;
  for (let index = messages.length - 1; index >= 0; index--) {
    const message = messages[index];
    if (message.role === 'user') return null;
    // The user cancelled what this reply ended with: nothing waits on it.
    if (message.cancelled) return null;
    if (
      message.blocks.some(
        (block) => block.kind === 'permission' && block.state === 'pending',
      )
    )
      return 'permission';
    if (
      message.blocks.some(
        (block) =>
          block.kind === 'response' &&
          parseQuestions(block.text).questions.length > 0,
      )
    )
      return 'question';
  }
  return null;
}

/** A queued task must wait while the latest user turn still has a question or permission request to resolve. */
export const needsUserInput = (sessionId: string): boolean => pendingInput(sessionId) !== null;

/** Told to the agent with a message that was written instead of answering its questions. What a skipped question means
 *  depends on its kind: an ordinary one is a preference the agent may settle itself; one marked `required` is not. */
const SKIPPED_QUESTIONS =
  'The user did not answer your questions and wrote this instead. Treat it as their answer; for anything it leaves open, use your best judgement and say what you assumed. Do not ask the same questions again unless you still cannot go on without them.';
const SKIPPED_REQUIRED =
  'The user did not answer the questions you marked required and wrote this instead. Do not guess those answers and do not do the work that depends on them: answer what they wrote, then ask again only what is still missing.';

/** The questions the latest user turn left open (the ones of the newest reply that has any). */
function openQuestions(sessionId: string): Question[] {
  const list = messagesOf(sessionId);
  for (let index = list.length - 1; index >= 0; index--) {
    const message = list[index];
    if (message.role === 'user') return [];
    for (const block of message.blocks) {
      if (block.kind !== 'response') continue;
      const questions = parseQuestions(block.text).questions;
      if (questions.length) return questions;
    }
  }
  return [];
}
/** Told to the agent with a message that was written instead of answering its permission request. */
const SKIPPED_PERMISSION =
  'The user did not answer your permission request and wrote this instead. The request was cancelled: the command or tool call was NOT run and nothing it would have changed was changed. Do not assume it ran; ask again only if it is still needed.';

/** The line the chat shows once a cancelled question or request has nothing left behind it (in the style of the note for a
 *  reply the app closed on). */
const CANCEL_NOTE = '> ✕ Task cancelled. Send a message to go on.';
/** Chats where the user cancelled while messages were queued behind: those run first, the note comes after them. */
const cancelNotes = new Set<string>();

function addCancelNote(sessionId: string) {
  cancelNotes.delete(sessionId);
  const session = workspace.sessions.find((item) => item.id === sessionId);
  const list = messagesOf(sessionId);
  const last = list.at(-1);
  const lastBlock = last?.blocks.at(-1);
  if (last?.role === 'assistant' && lastBlock?.kind === 'response' && lastBlock.text === CANCEL_NOTE) return;
  list.push({
    id: crypto.randomUUID(),
    role: 'assistant',
    time: clock(new Date().toISOString()),
    at: new Date().toISOString(),
    agent: session?.agent,
    blocks: [{ kind: 'response', text: CANCEL_NOTE }],
  });
  persistLocal();
}

/** The user cancels what the agent waits for (its questions or a permission request): nothing is sent to the agent. A
 *  message queued behind it goes out next and the chat works through the queue; when none is left, the chat says the
 *  task was cancelled. */
export function cancelPending(sessionId: string) {
  if (replies[sessionId] || !pendingInput(sessionId)) return;
  const list = messagesOf(sessionId);
  for (let index = list.length - 1; index >= 0; index--) {
    const message = list[index];
    if (message.role === 'user') break;
    if (message.blocks.some((block) => block.kind === 'permission' && block.state === 'pending')) closePendingPermissions(sessionId);
    message.cancelled = true;
  }
  persistLocal();
  if ((queued[sessionId]?.length ?? 0) > 0) {
    cancelNotes.add(sessionId);
    sendNextQueued(sessionId);
  } else addCancelNote(sessionId);
}

/** Closes the permission requests the latest user turn left unanswered, as "not answered" (what the cards say), and for a
 *  Copilot turn, which holds the request open, refuses it so that turn can end. */
function closePendingPermissions(sessionId: string) {
  const list = messagesOf(sessionId);
  for (let index = list.length - 1; index >= 0; index--) {
    const message = list[index];
    if (message.role === 'user') break;
    for (const block of message.blocks) {
      if (block.kind !== 'permission' || block.state !== 'pending') continue;
      if (message.agent === 'copilot') for (const request of block.requests) void answerCopilotPermission(request.id, false).catch(() => {});
      block.state = 'expired';
    }
  }
  persistLocal();
}

/** Dispatches one queued message only after the active turn and all user decisions are complete. */
function sendNextQueued(sessionId: string) {
  if (hasOutstandingSessionWork(sessionId)) return;
  // Let the current event stack finish before taking an item. Ctrl+Enter stops the current turn and
  // starts the priority message synchronously; taking an item before that message starts would
  // remove it from the queue, then `send` would reject it because the priority turn is now running.
  setTimeout(() => {
    if (hasOutstandingSessionWork(sessionId)) return;
    // Everything the user wrote while the agent was busy goes out as one message: one turn, one model, one reply.
    const waiting = dequeueAll(queued, sessionId);
    const next: Queued | undefined = waiting.length
      ? {
          text: waiting.map((item) => item.text.trim()).filter(Boolean).join('\n\n'),
          images: waiting.flatMap((item) => item.images),
          elements: waiting.flatMap((item) => item.elements),
          mentions: waiting.flatMap((item) => item.mentions ?? []),
          files: waiting.flatMap((item) => item.files ?? []),
        }
      : undefined;
    if (next) {
      send(sessionId, next.text, next.images, next.elements, next.mentions, {
        files: next.files,
      });
      return;
    }
    if (backlog.get(sessionId)?.length) {
      flushReports(sessionId, 0);
      return;
    }
    if (hasOutstandingSessionWork(sessionId)) return;
    // The queue behind a cancelled question has run out: say the task was cancelled.
    if (cancelNotes.has(sessionId)) addCancelNote(sessionId);
    notifySessionWorkDone(sessionId);
  }, 0);
}

/** A finished reply can still be animating its final Markdown into view; keep its queue behind that render. */
const renderWaiters: Record<
  string,
  { messageId: string; blocks: Set<number> }
> = $state({});

/** Whether the reply is still being written out on screen (its text is not all rendered yet, see `replyBlockRendered`). */
export function replyRendering(sessionId: string, messageId: string): boolean {
  return renderWaiters[sessionId]?.messageId === messageId;
}

/** Called after a response block's final text has reached the DOM. */
export function replyBlockRendered(
  sessionId: string,
  messageId: string,
  blockIndex: number,
) {
  const waiting = renderWaiters[sessionId];
  if (!waiting || waiting.messageId !== messageId) return;
  waiting.blocks.delete(blockIndex);
  if (waiting.blocks.size) return;
  delete renderWaiters[sessionId];
  sendNextQueued(sessionId);
}

/** If the user leaves the chat before its final animation ends, let its queued work continue in the background. */
export function releaseReplyRender(sessionId: string) {
  if (!renderWaiters[sessionId]) return;
  delete renderWaiters[sessionId];
  sendNextQueued(sessionId);
}

// ---------- keeping work going when a turn fails ----------
// A turn that ends in an error is not the end of the work: a passing fault is tried again, and otherwise (a usage
// limit, a sign-out, a crash) the next agent of the user's chain, another provider, takes over the chat (it is told
// what the others did, see `missedTurns`). Accounts are never switched here: that stays a manual step of the user. The decision is `planRecovery` (turn-failure.ts); the chat shows
// one line for each step. A new message of the user starts the count again.
type RecoveryState = { retries: number; tried: string[] };
const recoveries: Record<string, RecoveryState> = {};
const CLI_OF: Record<string, string> = { claude: 'claude', codex: 'codex', antigravity: 'agy', opencode: 'opencode', copilot: 'gh' };

/** The agents a chat can move to now, in the user's order: installed, with models, not at their limit. */
function recoveryChain(): string[] {
  return connectors.recovery.chain.filter((agent) => {
    const tool = CLI_OF[agent];
    if (!tool) return false;
    const installed = connectors.detected?.tools.find((item) => item.id === tool)?.path !== null;
    return installed && (MODELS[agent as Agent] ?? []).length > 0 && agentHasRoom(agent);
  });
}

/** What to do about a turn of `session` that ended with `message`; null when nothing is (off, or stopped by the user). */
function recoveryFor(session: SessionItem, message: string): { kind: FailureKind; plan: Recovery } | null {
  if (!connectors.recovery.enabled || !inTauri()) return null;
  const kind = classifyFailure(message);
  if (kind === 'stopped') return null;
  const state = (recoveries[session.id] ??= { retries: 0, tried: [] });
  if (!state.tried.includes(session.agent)) state.tried.push(session.agent);
  const plan = planRecovery({
    kind,
    retries: state.retries,
    chain: recoveryChain(),
    current: session.agent,
    tried: state.tried,
  });
  return { kind, plan };
}

/**
 * The CLI answered (part of) a turn with another model than the chat's. A fallback of the whole session ("session" scope,
 * or none: older CLIs only had that) is followed: the chat's model becomes the one now serving, so the picker shows the
 * truth and the next turn does not switch back. A "local" one (a sub-agent, a side question) leaves the chat's model as it
 * is. Either way the reply gets one line saying what happened.
 */
function noteModelFallback(session: SessionItem, reply: Message, line: string) {
  let data: Record<string, unknown>;
  try {
    data = JSON.parse(line) as Record<string, unknown>;
  } catch {
    return;
  }
  const text = (key: string, camel: string) => (typeof data[key] === 'string' ? data[key] : typeof data[camel] === 'string' ? data[camel] : '') as string;
  const to = text('fallback_model', 'fallbackModel');
  const from = text('original_model', 'originalModel');
  const local = data.scope === 'local';
  // Only a model the picker knows is taken over: `modelFor` would replace any other one with the agent's first model.
  const known = (MODELS[session.agent] ?? []).some((item) => item.value === to);
  const moves = !local && data.direction !== 'revert' && known;
  if (moves && session.model !== to) {
    session.model = to;
    session.conversationModel = to;
    window.dispatchEvent(new CustomEvent('agentdeck:agent-moved', { detail: { sessionId: session.id, agent: session.agent, model: to } }));
  }
  const said = text('content', 'content').trim();
  const note =
    said ||
    (local
      ? `Only that response came from ${modelLabel(to)}; the chat stays on ${modelLabel(from)}.`
      : `${modelLabel(from) || 'The model'} could not answer: the chat continues on ${modelLabel(to)}.`);
  reply.blocks = [...reply.blocks, { kind: 'response', text: `> ↻ ${note}` }];
}

/** Moves a chat to another agent (its first model), as picking it in the composer would. */
export function moveChatTo(session: SessionItem, agent: Agent) {
  const model = MODELS[agent]?.[0]?.value ?? session.model;
  if (session.agent !== agent && !session.draft) {
    session.agents = [...new Set([...(session.agents ?? [session.source ?? session.agent]), agent])];
    rememberMixed(session);
  }
  session.agent = agent;
  session.model = model;
  // A worker chat that moved on (after a failure) is handled by that agent now: the task says so, so its card and its
  // report name the agent that answers.
  const task = Object.values(delegateTasks).find((record) => record.childId === session.id);
  if (task) {
    task.agent = agent;
    task.agentId = agent;
    task.model = model;
    task.label = `${agentLabel(agent, model)} · ${modelLabel(model)}`;
  }
  // The composer keeps a picker per chat: it follows, or it would put the old agent back.
  window.dispatchEvent(new CustomEvent('agentdeck:agent-moved', { detail: { sessionId: session.id, agent, model } }));
}

/** Carries out the plan after the failed turn has been put away. */
async function runRecovery(sessionId: string, plan: Recovery, reason: string) {
  const session = workspace.sessions.find((item) => item.id === sessionId);
  const state = recoveries[sessionId];
  if (!session || !state || plan.action === 'stop') return;
  if (plan.action === 'retry') {
    state.retries++;
    await new Promise((resolve) => setTimeout(resolve, plan.delayMs));
    forgetUnstartedConversation(session);
  } else {
    moveChatTo(session, plan.agent as Agent);
    state.retries = 0;
    state.tried.push(plan.agent);
  }
  // The user may have sent something meanwhile, or stopped the chat: their message wins. A newer failure has its own
  // state, so this older plan is dropped too.
  if (replies[sessionId] || recoveries[sessionId] !== state) return;
  const same = plan.action !== 'fallback';
  send(sessionId, '', [], [], [], { prompt: continuePrompt(reason, same, lastRequest(sessionId)), silent: NO_HANDOUT });
}

// ---------- carrying on when a usage limit ends ----------
// A turn that ends at a usage limit with no other agent to move to waits for the limit to reset (its time is in the error,
// or in the account's usage windows) and then carries on by itself, as Claude Code does. The chat shows a card with the
// time, a switch to turn the automatic part off, and Try again / Cancel. A message of the user cancels it: they took over.
export type Resume = { sessionId: string; agent: string; at: number; reason: string; auto: boolean };
export const resumes = $state<Record<string, Resume>>({});
const RESUMES_KEY = 'chat.resumes';
/** A little after the reset, so the first request is not refused for being early. */
const RESUME_BUFFER_MS = 60_000;
const RESUME_MAX_WAIT_MS = 8 * 86_400_000;
const RESUME_MAX_AGE_MS = 36 * 3_600_000;
const resumeStore = { ready: false };
/** Automatic resumes made since the user last wrote: a limit that is still on after three of them stops the loop. */
const resumeTries: Record<string, number> = {};
const MAX_RESUME_TRIES = 3;
void getPref<Resume[]>(RESUMES_KEY)
  .then((saved) => {
    for (const item of Array.isArray(saved) ? saved : []) if (!resumes[item.sessionId] && item.at + RESUME_MAX_AGE_MS > Date.now()) resumes[item.sessionId] = item;
  })
  .catch(() => undefined)
  .finally(() => {
    resumeStore.ready = true;
  });
$effect.root(() => {
  $effect(() => {
    if (!resumeStore.ready) return;
    setPref(RESUMES_KEY, JSON.parse(JSON.stringify(Object.values(resumes))) as Resume[]);
  });
});

function scheduleResume(session: SessionItem, message: string) {
  const now = Date.now();
  const end = parseResetAt(message, now) ?? limitResetAt(session.agent, now);
  if (end === null || end - now > RESUME_MAX_WAIT_MS || (resumeTries[session.id] ?? 0) >= MAX_RESUME_TRIES) return;
  resumes[session.id] = {
    sessionId: session.id,
    agent: session.agent,
    at: Math.max(end, now) + RESUME_BUFFER_MS,
    reason: 'the usage limit was reached',
    auto: connectors.recovery.autoResume,
  };
}

/** Carries on now (the "Try again" button, or the time has come). */
export function resumeNow(sessionId: string) {
  const resume = resumes[sessionId];
  delete resumes[sessionId];
  if (resume) resumeTries[sessionId] = (resumeTries[sessionId] ?? 0) + 1;
  const session = workspace.sessions.find((item) => item.id === sessionId);
  if (!resume || !session || replies[sessionId]) return;
  send(sessionId, '', [], [], [], { prompt: continuePrompt(resume.reason, true, lastRequest(sessionId)), silent: NO_HANDOUT });
}
export const cancelResume = (sessionId: string) => void delete resumes[sessionId];
export function setResumeAuto(sessionId: string, auto: boolean) {
  const resume = resumes[sessionId];
  if (resume) resume.auto = auto;
}

function checkResumes() {
  const now = Date.now();
  for (const resume of Object.values(resumes)) {
    if (resume.at + RESUME_MAX_AGE_MS < now) {
      delete resumes[resume.sessionId];
      continue;
    }
    if (!resume.auto || resume.at > now) continue;
    const session = workspace.sessions.find((item) => item.id === resume.sessionId);
    // Wait for the chat to be loaded and idle: a turn that is running will have its own end.
    if (!session || isRunning(resume.sessionId) || replies[resume.sessionId]) continue;
    resumeNow(resume.sessionId);
  }
}
if (typeof window !== 'undefined') setInterval(checkResumes, 10_000);

/** The words of the user's latest message, for a turn that carries on without it. */
function lastRequest(sessionId: string): string {
  const asked = [...messagesOf(sessionId)].reverse().find((message) => message.role === 'user' && !hiddenPrompt(message));
  return asked?.blocks.flatMap((block) => (block.kind === 'response' ? [block.text] : [])).join('\n').trim() ?? '';
}

/**
 * A turn that failed before the CLI ever answered leaves an id with no conversation behind it (or, for Claude, one
 * it may refuse to reuse with `--session-id`). The retry then starts a new conversation instead.
 */
function forgetUnstartedConversation(session: SessionItem) {
  const first = session.source ?? session.agents?.[0] ?? session.agent;
  if (session.agent === first) {
    if (session.agentStarted === true || session.path) return;
    session.agentSession = undefined;
    session.agentStarted = false;
  } else if (session.agentIds?.[session.agent] && !session.agentIds[session.agent]?.started) {
    delete session.agentIds[session.agent];
  }
}

/** Apply one agent event to the reply it belongs to. */
/** Where a problem happened, for the error log (error-log.svelte.ts). No text of the conversation. */
function logCtx(sessionId: string, session?: SessionItem) {
  const found = session ?? workspace.sessions.find((item) => item.id === sessionId);
  return {
    chat: sessionId,
    agent: found?.agent,
    model: found?.model,
    parent: found?.parentId,
    step: found?.title && found.parentId ? found.title.split(':')[0] : undefined,
  };
}

// A turn that says nothing for this long is written to the log once (an agent that hangs, a tool that never returns,
// a member waiting on a message that never comes): that is the line to look for when the app seems frozen.
const SILENT_AFTER_MS = 5 * 60_000;
const lastEventAt = new Map<string, number>();
const lastEventKind = new Map<string, string>();
const silentLogged = new Set<string>();
function noteSilence(sessionId: string, session: SessionItem, event: AgentEvent) {
  if (event.kind === 'raw') return;
  lastEventAt.set(sessionId, Date.now());
  lastEventKind.set(sessionId, event.kind === 'tool_call' ? `tool_call ${event.name}` : event.kind);
  silentLogged.delete(sessionId);
  if (event.kind === 'error') logError('turn', 'turn failed', event.message, logCtx(sessionId, session));
  else if (event.kind === 'tool_result' && event.is_error) logError('mcp', 'tool failed', event.output.slice(0, 600), logCtx(sessionId, session));
}
if (typeof window !== 'undefined') {
  setInterval(() => {
    const now = Date.now();
    for (const [id, at] of lastEventAt) {
      if (!replies[id]) {
        lastEventAt.delete(id);
        lastEventKind.delete(id);
        silentLogged.delete(id);
        continue;
      }
      if (now - at > SILENT_AFTER_MS && !silentLogged.has(id)) {
        silentLogged.add(id);
        logError('turn', `no event for ${Math.round((now - at) / 60_000)} min`, `last: ${lastEventKind.get(id) ?? '?'}`, logCtx(id));
      }
    }
  }, 30_000);
}

/** An agent event as the reducer's events (see run-state.ts). Thinking and usage do not move the run. */
function recordAgentEvent(sessionId: string, event: AgentEvent) {
  switch (event.kind) {
    case 'delta':
      return advanceRun(sessionId, { type: 'output', text: event.text });
    case 'tool_result':
    case 'tool_output_delta':
      return advanceRun(sessionId, { type: 'output' });
    case 'permission_denied':
      for (let index = 0; index < event.requests.length; index++) advanceRun(sessionId, { type: 'waitStart', kind: 'approval' });
      return;
    case 'done':
      advanceRun(sessionId, { type: 'output', text: event.text });
      return advanceRun(sessionId, { type: 'turnResult', isError: false });
    case 'error':
      return advanceRun(sessionId, { type: 'turnResult', isError: true, message: event.message });
    default:
      return;
  }
}

/**
 * The CLI went on by itself (a sub-agent or a background job ended and it reports on it): that is a new reply in the
 * chat with no message of the user above it. A CLI that says so while a reply is already live (Copilot, mid-turn) just
 * goes on writing that reply.
 */
function beginAutoTurn(sessionId: string, turn: string) {
  const session = workspace.sessions.find((item) => item.id === sessionId);
  if (!session || liveReplies[sessionId]?.live || hasSideTurn(sessionId)) return;
  const list = messagesOf(sessionId);
  list.push({
    id: nextId(),
    role: 'assistant',
    time: clock(new Date().toISOString()),
    at: new Date().toISOString(),
    blocks: [],
    live: true,
    startedAt: Date.now(),
    agent: session.agent,
    model: session.model,
  });
  const reply = list[list.length - 1];
  liveReplies[sessionId] = reply; // the stored (reactive) object
  replies[sessionId] = reply.id;
  turns[sessionId] = turn;
  session.status = 'working';
  const epoch = (runEpochs[sessionId] = (runEpochs[sessionId] ?? 0) + 1);
  advanceRun(sessionId, { type: 'spawn', epoch });
  advanceRun(sessionId, { type: 'turnStarted', epoch });
}

export function handleAgentEvent(
  sessionId: string,
  event: AgentEvent,
  turn?: string,
) {
  if (event.kind === 'auto_turn') {
    if (turn !== undefined) beginAutoTurn(sessionId, turn);
    return;
  }
  if (turn !== undefined && turns[sessionId] !== turn) return; // a run that was stopped or replaced
  recordAgentEvent(sessionId, event);
  const session = workspace.sessions.find((item) => item.id === sessionId);
  // The live reply is kept by reference: the chat list can be swapped under it (another chat opened, a transcript
  // reloaded) and the turn must still finish, or the chat would spin for ever.
  const reply =
    liveReplies[sessionId] ??
    messagesOf(sessionId).find((item) => item.id === replies[sessionId]);
  if (!session) return;
  // Codex writes a thread's rollout, and Claude its session file, only once a run has started. A run stopped before
  // that leaves an id with nothing to resume. The next message then starts a new conversation, instead of resuming
  // that id again and failing the same way.
  if (
    event.kind === 'error' &&
    /no rollout found|no conversation found/i.test(event.message)
  ) {
    // The CLI no longer has the conversation this id named (a thread whose run was cut short, a cleaned-up session).
    // The next turn starts a new one, and is given what was said so far as text, so the work in progress is not lost.
    session.agentStarted = false;
    session.agentSession = undefined;
    session.forkContext = (session.forkContext ?? '') + handoffFor(sessionId, lastRequest(sessionId));
  }
  trackActivity(
    sessionId,
    session.title,
    workspace.projects.find((project) => project.id === session.projectId)
      ?.name ?? '',
    event,
  );
  noteSilence(sessionId, session, event);
  const documentTask = guide.tasks[sessionId];
  if (documentTask?.status === 'working') {
    if (event.kind === 'tool_call') documentTask.progress = `Agent is running ${event.name}`;
    if (event.kind === 'tool_result') documentTask.progress = event.is_error
      ? `Tool failed: ${event.output.slice(0, 300)}`
      : `Finished ${documentTask.progress.replace(/^Agent is running /, '')}`;
  }
  // A line of the CLI the chat does not show: it is only kept in the Audit log. A model fallback is the one it does show.
  if (event.kind === 'raw') {
    if (reply && event.source === 'claude' && /^model_(refusal_)?fallback$/.test(event.label)) noteModelFallback(session, reply, event.line);
    return;
  }
  if (!reply) {
    if (event.kind === 'done' || event.kind === 'error') {
      delete replies[sessionId];
      session.status = hasSideTurn(sessionId) ? 'working' : 'idle';
    }
    return;
  }
  if (event.kind === 'usage') {
    reply.model ??= session.model;
    addUsage(reply, event);
    return;
  }
  if (event.kind === 'done' && event.cost_usd != null)
    reply.cost = (reply.cost ?? 0) + event.cost_usd;
  const update = applyAgentEvent(reply.blocks, event);
  reply.blocks = update.blocks;
  // An auto-compress turn shows one line: what `/compact` prints is not an answer.
  if (reply.compressed && event.kind !== 'error')
    reply.blocks = event.kind === 'done'
      ? [{ kind: 'response', text: '*Context compressed: the conversation goes on from a summary of what came before.*' }]
      : [{ kind: 'loading', label: 'Compressing context…' }];
  if (event.kind === 'tool_result' && event.image)
    persistCommandImage(sessionId, reply, event.id, event.image);
  // A delegate_task call just showed up: its card follows the task the call starts.
  if (event.kind === 'tool_call' && toolKindOf(event.name) === 'delegate_task')
    bindTasks(sessionId);
  // The CLI has the conversation once it answers; only then is `--resume` valid for the next turn.
  const first = session.source ?? session.agents?.[0] ?? session.agent;
  const agent = reply.agent ?? session.agent;
  if (agent === first) {
    if (event.kind !== 'error') session.agentStarted = true;
    if (update.agentSession) session.agentSession = update.agentSession;
  } else if (
    event.kind !== 'error' &&
    (update.agentSession || session.agentIds?.[agent])
  ) {
    const id = update.agentSession ?? session.agentIds?.[agent]?.id;
    // The rest of the entry (the account epoch) stays: only the id and its start change here.
    if (id) (session.agentIds ??= {})[agent] = { ...session.agentIds?.[agent], id, started: true };
  }
  // A failed turn that can be carried on (retry, another account, the next agent) does not end the work: the chat
  // gets a line saying what happens, and the steps below that close the work (failure, reports) are skipped.
  const recovery =
    update.finished && event.kind === 'error' ? recoveryFor(session, event.message) : null;
  const recovering = recovery !== null && recovery.plan.action !== 'stop';
  if (update.finished && event.kind !== 'error') delete recoveries[sessionId];
  // A limit with nowhere to move: wait for it to reset, then carry on (see `resumes`).
  if (update.finished && event.kind === 'error' && !recovering && classifyFailure(event.message) === 'limit') scheduleResume(session, event.message);
  if (recovery) {
    const plan = recovery.plan;
    const note = recoveryNote(recovery.kind, plan, {
      from: agentLabel(session.agent, session.model),
      to: plan.action === 'fallback' ? agentLabel(plan.agent as Agent) : undefined,
    });
    reply.blocks = [...reply.blocks, { kind: 'response', text: `> ↻ ${note}` }];
  }
  if (update.finished) {
    if (documentTask?.status === 'working' && !recovering) {
      documentTask.status = event.kind === 'error' ? 'error' : 'done';
      documentTask.progress = event.kind === 'error' ? 'Document task failed' : 'Document task finished';
      documentTask.result = event.kind === 'error'
        ? event.message
        : event.kind === 'done'
          ? event.text || reply.blocks.flatMap((block) => block.kind === 'response' ? [block.text] : []).join('\n').trim()
          : reply.blocks.flatMap((block) => block.kind === 'response' ? [block.text] : []).join('\n').trim();
      documentTask.outputPath = documentTask.result.match(/(?:[A-Za-z]:\\|\\\\|\/|docs[\\/])[^\n\r"'<>]*?\.(?:docx|pdf|pptx|webm|html|md)\b/i)?.[0];
    }
    // The chat is released in `finally`: a step here that throws must not leave it working for ever.
    try {
      if (event.kind === 'error' && !recovering) failSessionWork(sessionId);
      reply.live = false;
      void finishCommandImages(sessionId, reply);
      if (
        session.path &&
        session.source &&
        reply.agent &&
        reply.agent !== session.source
      ) {
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
      // A worker or a hand-back that is being carried on is not over: its waiter gets the turn that finishes it.
      if (!recovering) {
        onDelegateDone(
          sessionId,
          reply,
          event.kind === 'error' ? event.message : null,
        );
        onHandbackDone(sessionId, reply, event.kind === 'error');
        endWaitingTasks(sessionId);
      }
      // Our own copy of the turn (agent, model, time), apart from what the CLIs store: the base for syncing later.
      archiveTurn(
        {
          id: sessionId,
          title: session.title,
          agent: session.agent,
          model: reply.model ?? session.model,
          projectPath:
            workspace.projects.find(
              (project) => project.id === session.projectId,
            )?.path ?? null,
        },
        [liveAsks[sessionId], reply].filter((message): message is Message =>
          Boolean(message),
        ),
      );
    } finally {
      delete replies[sessionId];
      delete liveReplies[sessionId];
      delete liveAsks[sessionId];
      delete liveHandouts[sessionId];
      session.status = hasSideTurn(sessionId) ? 'working' : 'idle';
      persistLocal();
      // The CLI wrote the conversation to disk: pick it up so the row gets its path, in place (same id).
      // The cached transcript of this chat is now behind the file: a new time makes the next open read it again.
      session.updatedAt = new Date().toISOString();
      transcripts.delete(sessionId);
      if (!session.path) void importHistory({ refresh: true });
      // The final Markdown animation can outlast the CLI turn. Keep queued work behind the last rendered response.
      const responseBlocks = reply.blocks.flatMap((block, index) =>
        block.kind === 'response' && block.text ? [index] : [],
      );
      if (recovering && recovery && event.kind === 'error')
        void runRecovery(sessionId, recovery.plan, clipText(event.message, 200));
      else if (
        sessionId === workspace.activeId &&
        responseBlocks.length &&
        !needsUserInput(sessionId)
      ) {
        renderWaiters[sessionId] = {
          messageId: reply.id,
          blocks: new Set(responseBlocks),
        };
      } else sendNextQueued(sessionId);
    }
  }
}

if (inTauri()) {
  // A fast stream sends dozens of deltas a second; each one re-rendered the chat. Apply them once per frame.
  let queue: { session: string; turn: string; event: AgentEvent }[] = [];
  const flush = () => {
    if (!queue.length) return; // already applied by the other of the two triggers
    const batch = queue;
    queue = [];
    for (const { session, turn, event } of batch) {
      // One event that throws must not drop the rest of the frame: a `done` behind it (of this chat or of another
      // running at the same time) would be lost, and that chat would show it is working for ever.
      try {
        if (sideTurns.has(session)) handleSideEvent(session, event, turn);
        else handleAgentEvent(session, event, turn);
      } catch (error) {
        console.error('agent event failed', session, event.kind, error);
      }
    }
  };
  void listenAgentEvents((envelope) => {
    if (queue.push(envelope) !== 1) return;
    // A hidden or minimised window gets no animation frames: the timer applies the events then, so a turn still
    // ends (and its queue, its notification and the agents waiting on it go on) while nobody looks.
    requestAnimationFrame(flush);
    setTimeout(flush, 250);
  });
}

// ---------- delegating to another agent ----------
// `@claude do this` in a chat does not switch the chat to Claude. The task runs in a chat of its own under this one
// (a child: it shows in the sidebar and can be opened), with a summary of this chat as its background. When it has
// answered, the answer goes back into this chat and the chat's own agent carries on with it. With several `@`s the
// children run side by side and the agent is called once, when the last one has answered.

type DelegateBatch = {
  main: string;
  task: string;
  /** The message whose cards show the work. */ asked: Message;
  pending: Set<string>;
  results: { label: string; text: string }[];
  run: DelegateRun;
  /** A workflow preset's note to the agent that writes the final answer. */ orchestrator?: string;
  /** More for that agent: the findings the steps shared, where their notes are. */ memo?: string;
};
type DelegateCard = Extract<Block, { kind: 'delegate' }>;
const delegations = new Map<
  string,
  { batch: DelegateBatch; label: string; card: DelegateCard; node: RunNode }
>();

// What the Workflow tab draws: one run per delegated message (main chat -> a child per agent -> handback -> main agent).
export type RunStats = {
  ms: number;
  tokens?: import('./usage').Tokens;
  cost: number | null;
  tools: number;
  last: string;
};
/** One question a step put to an earlier step (its chat answers it), kept for the Workflow tab and the Audit log. */
export type Talk = {
  to: string;
  question: string;
  answer?: string;
  at: number;
};
export type RunNode = {
  childId: string;
  talks?: Talk[];
  /** The preset step it runs, when the run comes from a workflow. */ stepId?: string;
  /** What it was handed (earlier answers) and what it answered, for the audit. */ input?: string;
  output?: string;
  handoff?: HandoffReport | null;
  attempt?: number;
  agentSession?: string;
  agent: Agent;
  model: string;
  label: string;
  startedAt: number;
  endedAt?: number;
  state: 'running' | 'done' | 'failed';
  note?: string;
  stats?: RunStats;
};
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
/** Made-up runs (demo-runs.ts) are told by this start of their id: they are never saved with the real history. */
const DEMO_RUN = 'demo-run-';
/** Puts made-up finished runs in the Workflow history, to look at how the flows are judged. */
export function loadDemoRuns() {
  clearDemoRuns();
  delegateRuns.push(...demoRuns());
}
/** Takes the made-up runs out again. */
export function clearDemoRuns() {
  for (let index = delegateRuns.length - 1; index >= 0; index--) if (delegateRuns[index].id.startsWith(DEMO_RUN)) delegateRuns.splice(index, 1);
}
export const hasDemoRuns = () => delegateRuns.some((run) => run.id.startsWith(DEMO_RUN));
// The browser mock has no flows to show, so it starts with the made-up ones (the desktop app does not).
if (!inTauri()) loadDemoRuns();

// Finished runs are kept (IndexedDB), so the Workflow tab has a history to judge the flows by after a restart. What a
// step was handed and answered is cut short: the numbers and the outcome are what the history is for.
const RUNS_KEY = 'workflow.runs';
const clip = (text: string | undefined, max = 1500) =>
  text && text.length > max ? `${text.slice(0, max)}…` : text;
void getPref<DelegateRun[]>(RUNS_KEY).then((saved) => {
  if (!Array.isArray(saved)) return;
  const known = new Set(delegateRuns.map((run) => run.id));
  delegateRuns.unshift(...saved.filter((run) => !known.has(run.id)));
});
$effect.root(() => {
  $effect(() => {
    const ended = delegateRuns.filter((run) => run.endedAt && !run.id.startsWith(DEMO_RUN));
    const snapshot = JSON.parse(JSON.stringify(ended)) as DelegateRun[];
    for (const run of snapshot)
      for (const node of run.nodes)
        Object.assign(node, {
          input: clip(node.input),
          output: clip(node.output),
        });
    setPref(RUNS_KEY, snapshot.slice(-RUN_LIMIT));
  });
});

function statsOf(
  reply: Message | undefined,
  startedAt: number,
): RunStats | undefined {
  if (!reply) return undefined;
  const text = reply.blocks.flatMap((block) =>
    block.kind === 'response' ? [block.text] : [],
  );
  const last =
    text
      .join('\n')
      .split('\n')
      .reverse()
      .find((line) => line.trim()) ?? '';
  const commands = reply.blocks.filter(
    (block): block is Extract<Block, { kind: 'command' }> =>
      block.kind === 'command',
  );
  const cost =
    reply.cost ??
    (reply.tokens ? estimateCost(reply.tokens, reply.model) : null);
  return {
    ms: reply.ms ?? Date.now() - (reply.startedAt ?? startedAt),
    tokens: reply.tokens ? { ...reply.tokens } : undefined,
    cost,
    tools: commands.length,
    last:
      last.trim() ||
      (commands.length ? commands[commands.length - 1].command : ''),
  };
}

/** The numbers of a turn still being written in `sessionId` (read it again each second: the reply is not reactive here). */
export const liveStats = (sessionId: string, startedAt: number) =>
  statsOf(liveReplies[sessionId], startedAt);

/** Whether the chat `childId` is still working on something it was handed. */
export const isDelegating = (childId: string) =>
  delegations.has(childId) || stepWaiters.has(childId);
/** Whether the chat `childId` stopped on an approval card that waits for the user (a flow step that needs a yes). Not reactive: read it with a ticker. */
export const needsApproval = (childId: string) => {
  const list =
    childId === workspace.activeId
      ? chat.messages
      : (localMessages[childId] ?? []);
  return list
    .slice(-3)
    .some((message) =>
      message.blocks.some(
        (block) => block.kind === 'permission' && block.state === 'pending',
      ),
    );
};
/** Whether a run that started some of these chats is still going (a step waiting in its queue is waiting for it). */
export const isRunActive = (childIds: string[]) =>
  delegateRuns.some(
    (run) =>
      !run.endedAt && run.nodes.some((node) => childIds.includes(node.childId)),
  );

const firstLineOf = (text: string, max: number) => {
  const line = text.split('\n')[0].trim();
  return line.length > max ? `${line.slice(0, max - 1)}…` : line;
};

const plainText = (message: Message) =>
  message.blocks
    .flatMap((block) =>
      block.kind === 'response'
        ? [block.text]
        : block.kind === 'command'
          ? [`(ran: ${block.command.slice(0, 160)})`]
          : [],
    )
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
      const messages =
        transcripts.get(mention.id)?.messages ??
        localMessages[mention.id] ??
        [];
      parts.push(
        messages.length
          ? `Chat "${mention.label}", its latest messages:\n${digestOf(messages, 4000)}`
          : `Chat "${mention.label}" (not opened in this app yet, so no text of it is known here)`,
      );
    } else if (mention.kind === 'project') {
      const project = workspace.projects.find((item) => item.id === mention.id);
      if (project)
        parts.push(`Project "${project.name}" is at ${project.path}`);
    }
  }
  return parts.length
    ? `The user attached this context:\n\n${parts.join('\n\n')}\n\n---\n`
    : '';
}

function delegate(
  main: SessionItem,
  asked: Message,
  task: string,
  targets: Mention[],
  preface: string,
  images: Attachment[],
  elements: PageElement[],
  files: FileAttachment[] = [],
) {
  delegateRuns.push({
    id: crypto.randomUUID(),
    main: main.id,
    task,
    startedAt: Date.now(),
    nodes: [],
    handback: 'waiting',
  });
  if (delegateRuns.length > RUN_LIMIT)
    delegateRuns.splice(0, delegateRuns.length - RUN_LIMIT);
  const run = delegateRuns[delegateRuns.length - 1];
  const batch: DelegateBatch = {
    main: main.id,
    task,
    asked,
    pending: new Set(),
    results: [],
    run,
  };
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
    asked.blocks.push({
      kind: 'delegate',
      childId: id,
      agent,
      model,
      state: 'running',
    });
    const card = asked.blocks[asked.blocks.length - 1] as DelegateCard;
    const label = `${agentLabel(agent)} · ${modelLabel(model)}`;
    run.nodes.push({
      childId: id,
      agent,
      model,
      label,
      startedAt: Date.now(),
      state: 'running',
    });
    const node = run.nodes[run.nodes.length - 1];
    delegations.set(id, { batch, label, card, node });
    batch.pending.add(id);
    // The child has not spoken in this chat, so every earlier turn of it is background for the child.
    const background = missedTurns({ ...child, agent: '' as Agent }, history);
    const brief = `${preface}${background}You were asked from another chat, whose own agent is ${agentLabel(main.agent)}, to do this part of the work: ${task}\n\nAnswer fully, as you would for the user. ${REPORT_RULE} ${LANGUAGE_RULE}`;
    send(id, task, images, elements, [], { preface: brief, files });
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
    waiter(
      plainText({
        ...reply,
        blocks: reply.blocks.filter((block) => block.kind === 'response'),
      }),
      error,
      reply,
    );
    return;
  }
  const entry = delegations.get(childId);
  if (!entry) return;
  delegations.delete(childId);
  const text = plainText({
    ...reply,
    blocks: reply.blocks.filter((block) => block.kind === 'response'),
  });
  const ok = !error && text.length > 0;
  entry.card.state = ok ? 'done' : 'failed';
  entry.card.note = ok ? undefined : (error ?? 'no answer');
  entry.node.state = ok ? 'done' : 'failed';
  entry.node.note = ok ? undefined : (error ?? 'no answer');
  entry.node.endedAt = Date.now();
  entry.node.stats = statsOf(reply, entry.node.startedAt);
  // Only the compact digest goes back to the asking agent (the full answer stays in the child's chat); an answer with no
  // digest goes back whole.
  // The room for digests is shared by the agents asked together: one agent may use up to 8000 characters, ten about 1600.
  const room = Math.max(
    1600,
    Math.floor(16000 / Math.max(1, entry.batch.run.nodes.length)),
  );
  const digest =
    ok && /^#{1,3}\s*handoff\b/im.test(text)
      ? handoffOf(text, Math.min(room, 8000))
      : text;
  entry.batch.results.push({
    label: entry.label,
    text: ok ? digest : `(it did not answer: ${error ?? 'no text'})`,
  });
  entry.batch.pending.delete(childId);
  keepHandout(entry.batch.main, entry.batch.asked);
  if (entry.batch.pending.size === 0) {
    if (entry.batch.run.cancelled)
      endCancelled(entry.batch.main, entry.batch.run);
    else handBack(entry.batch);
  }
}

/** A run the user stopped: nothing is called back, the chat is free again. */
function endCancelled(main: string, run: DelegateRun) {
  const session = workspace.sessions.find((item) => item.id === main);
  if (session && !replies[main]) session.status = 'idle';
  run.handback = 'failed';
  run.endedAt = Date.now();
  sendNextQueued(main);
}

// ---------- delegate_task: an agent hands a task to another agent during its own turn ----------
// Claude, Codex, Antigravity and local models can call the app's `delegate_task` tool (src-tauri/src/delegate.rs). The Rust
// side emits the call here and waits for `replyDelegateCall`. A task runs as a child chat under the caller's chat, like an
// `@agent` task, but nothing is called back by the app itself: the worker's compact report (its "## Handoff") is the tool's
// answer. `wait: true` answers when the worker is done (the caller's turn goes on in the same turn). `wait: false` answers
// at once with a task id; when the worker is done its report reaches the caller as a silent hand-back turn, as soon as the
// caller is idle (reports that arrive meanwhile wait in line and go together). The card of the call (TaskCard.svelte) shows
// the task, and stops it.

export const delegateTasks = $state<Record<string, TaskRecord>>({});
const TASKS_KEY = 'delegate.tasks';
const TASK_LIMIT = 200;

type Ending = 'user' | 'caller' | 'timeout' | 'abort';
/** What a running task needs besides its record: the tool call that waits for its report, the timer, why it is ending. */
const taskRuns = new Map<
  string,
  { callId?: string; timer?: ReturnType<typeof setTimeout>; ending?: Ending }
>();

// Tasks outlive a reload as cards: a task whose worker was running then has no run any more, so it reads as stopped.
const taskStore = $state({ ready: false });
void getPref<TaskRecord[]>(TASKS_KEY)
  .then((saved) => {
    for (const item of Array.isArray(saved) ? saved : []) {
      if (delegateTasks[item.id]) continue;
      const lost = item.state === 'queued' || item.state === 'running' || item.state === 'approval';
      delegateTasks[item.id] = lost
        ? {
            ...item,
            state: 'stopped',
            endedAt: item.endedAt ?? Date.now(),
            note: 'Stopped: AgentDeck was closed or reloaded while it ran.',
          }
        : item;
    }
  })
  .catch(() => undefined)
  .finally(() => {
    taskStore.ready = true;
  });
$effect.root(() => {
  $effect(() => {
    if (!taskStore.ready) return;
    const all = (
      JSON.parse(JSON.stringify(Object.values(delegateTasks))) as TaskRecord[]
    ).sort((a, b) => a.startedAt - b.startedAt);
    setPref(TASKS_KEY, all.slice(-TASK_LIMIT));
  });
});

/** Whether this chat is a worker of a task: it does not delegate further. */
const isTaskChild = (id: string) =>
  Object.values(delegateTasks).some((item) => item.childId === id);
/**
 * A chat that only works for another one: a delegated task's worker or a step of a flow. A fork also sits under its
 * parent in the tree (`parentId`), but it is a chat of its own: it opens pages and delegates like any other, and its
 * state is its own.
 */
function isWorkerChat(id: string): boolean {
  return (
    isTaskChild(id) ||
    delegateRuns.some((run) => run.nodes.some((node) => node.childId === id))
  );
}

/** The chat of a call. A link that names none (Antigravity's stored one) is the chat of that agent running now. */
function callerOf(chatId: string): SessionItem | undefined {
  const named = chatId
    ? workspace.sessions.find((item) => item.id === chatId)
    : undefined;
  if (named) return named;
  const running = workspace.sessions.filter(
    (item) => item.status === 'working' && replies[item.id],
  );
  return running.length === 1
    ? running[0]
    : (running.find((item) => item.id === workspace.activeId) ?? running[0]);
}

/** The agents `list_agents` shows and `delegate_task` takes: the installed CLIs, then each local provider. */
function agentRows(): AgentRow[] {
  const installed = (tool: string) =>
    connectors.detected?.tools.find((item) => item.id === tool)?.path !== null;
  const rows: AgentRow[] = [];
  for (const [id, tool] of [
    ['claude', 'claude'],
    ['codex', 'codex'],
    ['antigravity', 'agy'],
    ['opencode', 'opencode'],
  ] as const) {
    const models = (MODELS[id] ?? []).map((item) => ({
      value: item.value,
      label: item.label,
      description: item.description,
    }));
    // Its room is part of the row, so an agent that hands work out does not pick one at its limit.
    const room = roomLine(id);
    const label = `${agentLabel(id)}${room ? `, ${room}` : ''}${agentHasRoom(id) ? '' : ', AT ITS LIMIT: do not use'}`;
    if (models.length && installed(tool)) rows.push({ id, label, models });
  }
  for (const provider of connectors.custom.filter(
    (item) => item.enabled && item.kind !== 'anthropic',
  )) {
    const names = provider.models.length
      ? provider.models
      : provider.defaultModel
        ? [provider.defaultModel]
        : [];
    if (names.length)
      rows.push({
        id: provider.id,
        label: provider.name,
        models: names.map((name) => ({ value: name, label: name })),
        restricted: Boolean(provider.restricted),
      });
  }
  return rows;
}

const span = (ms: number) => elapsed(ms);

/** The text a tool answers about a task that has not ended, or has. */
function statusText(record: TaskRecord): string {
  const head = {
    taskId: record.id,
    childId: record.childId,
    label: record.label,
    took: span((record.endedAt ?? Date.now()) - record.startedAt),
  };
  if (
    record.state === 'queued' ||
    record.state === 'running' ||
    record.state === 'approval'
  ) {
    return formatTaskResult(
      { ...head, verdict: 'running' },
      record.state === 'approval'
        ? 'Waiting: the worker needs the user to approve a step in its chat.'
        : record.state === 'queued'
          ? `Queued: at most ${MAX_WORKERS} workers run at once; it starts when one finishes.`
          : 'Still working.',
    );
  }
  const verdict: Verdict =
    record.state === 'done'
      ? 'done'
      : record.state === 'stopped'
        ? 'stopped'
        : 'failed';
  return formatTaskResult(
    { ...head, verdict },
    record.digest ?? record.note ?? '',
  );
}

/** Links each delegate_task line of a caller's live reply to the task it started (the card reads the task from it). */
function bindTasks(callerId: string) {
  const reply = liveReplies[callerId];
  if (!reply) return;
  const lines = reply.blocks.filter(
    (block): block is Extract<Block, { kind: 'command' }> =>
      block.kind === 'command' &&
      kindOfCommand(block.command) === 'delegate_task',
  );
  for (const line of lines) {
    if (line.taskId) continue;
    const taken = new Set(lines.map((item) => item.taskId));
    const free = Object.values(delegateTasks).filter(
      (item) =>
        item.caller === callerId &&
        item.startedAt >= (reply.startedAt ?? 0) - 1000 &&
        !taken.has(item.id),
    );
    const id = pickTask(
      argsOfCall(line.command.replace(/^\S+\s*/, '')),
      free.map((item) => ({
        id: item.id,
        agent: item.agentId,
        task: item.task,
      })),
    );
    if (id) line.taskId = id;
  }
}

/** Starts the worker of a `delegate_task` call. `reply` answers the call; a task in the background answers at once. */
function startTask(
  call: DelegateCall,
  reply: (text: string, isError?: boolean) => void,
) {
  const parsed = parseDelegateArgs(call.args);
  if (!parsed.ok) return reply(parsed.error, true);
  const {
    agent: wanted,
    model: wantedModel,
    task,
    wait,
    timeoutS,
  } = parsed.args;
  const caller = callerOf(call.caller);
  if (!caller)
    return reply(
      'AgentDeck could not tell which chat is calling. Do the task yourself, or ask the user to run it from the chat.',
      true,
    );
  if (isTaskChild(caller.id))
    return reply(
      'You are a worker of a delegated task and cannot delegate further: do it yourself and report back.',
      true,
    );
  const rows = agentRows();
  const row = findAgent(rows, wanted);
  if (!row)
    return reply(
      `No agent "${wanted}" is available. ${formatAgents(rows)}`,
      true,
    );
  // A free tier may train on what it is sent: a task that carries a secret or the project's code never leaves.
  const leak = row.restricted ? privateContentIn(task) : null;
  if (leak)
    return reply(
      `Not sent: ${row.label} is a free tier that may keep what it is sent, and this task holds ${leak}. Do this part yourself or give it to another agent; send the free tier only public, non-sensitive text.`,
      true,
    );
  // A worker of the same provider keeps the model selected in its parent chat unless the caller asked for one.
  // Different providers cannot run that model id, so they use their own normal default.
  const inheritedModel =
    row.id === caller.agent &&
    row.models.some((item) => item.value === caller.model)
      ? caller.model
      : undefined;
  const chosen = resolveModel(
    row.models,
    wantedModel ?? inheritedModel,
    row.id === 'claude' ? 'sonnet' : undefined,
  );
  if ('error' in chosen) return reply(chosen.error, true);
  const agent: Agent =
    row.id === 'claude' ||
    row.id === 'codex' ||
    row.id === 'antigravity' ||
    row.id === 'opencode'
      ? row.id
      : 'custom';
  const model =
    agent === 'custom' ? customModelId(row.id, chosen.model) : chosen.model;
  let taskId = newTaskId();
  while (delegateTasks[taskId]) taskId = newTaskId();

  const uuid = crypto.randomUUID();
  const id = `claude:${uuid}`;
  const label = `${agentLabel(agent, model)} · ${modelLabel(model)}`;
  workspace.sessions.push({
    id,
    projectId: caller.projectId,
    title: `@${agentLabel(agent, model)}: ${firstLineOf(task, 50)}`,
    agent,
    agentSession: uuid,
    model,
    gitBranch: caller.gitBranch,
    status: 'idle',
    parentId: caller.id,
    draft: false,
  });
  delegateTasks[taskId] = {
    id: taskId,
    caller: caller.id,
    childId: id,
    state: 'queued',
    wait,
    label,
    agent,
    agentId: row.id,
    model,
    task,
    startedAt: Date.now(),
  };
  const run: {
    callId?: string;
    timer?: ReturnType<typeof setTimeout>;
    ending?: Ending;
  } = { callId: wait ? call.callId : undefined };
  taskRuns.set(taskId, run);
  run.timer = setTimeout(
    () =>
      stopTask(
        taskId,
        'timeout',
        `Timed out after ${timeoutS} s: the worker was stopped before it finished.`,
      ),
    timeoutS * 1000,
  );
  bindTasks(caller.id);

  // The worker's end: its report, or why there is none. A worker stopped on an approval card waits for the user's yes.
  const settle = (answer: string, error: string | null) => {
    const record = delegateTasks[taskId];
    if (!record || !taskRuns.has(taskId)) return;
    if (!run.ending && !error && needsApproval(id)) {
      record.state = 'approval';
      stepWaiters.set(id, settle);
      return;
    }
    const verdict: Verdict = run.ending
      ? run.ending === 'timeout'
        ? 'timed out'
        : 'stopped'
      : error === 'stopped'
        ? 'stopped'
        : error || !answer
          ? 'failed'
          : 'done';
    finishTask(taskId, verdict, run.ending ? '' : (error ?? ''), answer);
  };
  stepWaiters.set(id, settle);
  const brief = `You were asked by another agent (${agentLabel(caller.agent, caller.model)}), who is working for the user in this project, to do the task below on your own. Answer fully, as you would for the user. ${REPORT_RULE} ${LANGUAGE_RULE}`;
  // At most MAX_PARALLEL_WORKERS run at once (delegate.rs keeps the line); the rest wait as `queued`. A free place
  // starts the worker at once, so three calls of one turn run side by side.
  slotStarts.set(taskId, () => {
    const record = delegateTasks[taskId];
    if (!record || record.state !== 'queued' || !taskRuns.has(taskId)) return;
    record.state = 'running';
    record.startedAt = Date.now();
    send(id, task, [], [], [], { preface: `${brief}\n\n` });
    persistLocal();
  });
  persistLocal();
  void enterWorkerSlot(taskId)
    // A gate that cannot answer must not block the work.
    .catch(() => true)
    .then((granted) => {
      // Ended (stopped, timed out) before the gate answered: its release came first and freed nothing, so the place
      // it was just given goes back now.
      if (!taskRuns.has(taskId)) {
        void releaseWorkerSlot(taskId).catch(() => undefined);
        return;
      }
      if (!wait) reply(startedText({ taskId, childId: id, label }, !granted));
      if (granted) startQueued(taskId);
    });
}

/** The worker waits for a place: the function that starts it once the place is given. */
const slotStarts = new Map<string, () => void>();
function startQueued(taskId: string) {
  const start = slotStarts.get(taskId);
  slotStarts.delete(taskId);
  start?.();
}

/** Ends a task: the record, the worker's chat, and the answer: to the tool call that waits, or to the caller in line. */
function finishTask(
  taskId: string,
  verdict: Verdict,
  detail: string,
  answer: string,
) {
  const record = delegateTasks[taskId];
  const run = taskRuns.get(taskId);
  if (!record || !run) return;
  if (verdict === 'failed' || verdict === 'timed out')
    failSessionWork(record.caller);
  clearTimeout(run.timer);
  taskRuns.delete(taskId);
  slotStarts.delete(taskId);
  // Its place goes to the next one in line (a task that never started leaves the line).
  void releaseWorkerSlot(taskId).catch(() => undefined);
  stepWaiters.delete(record.childId);
  record.endedAt = Date.now();
  record.state =
    verdict === 'done' ? 'done' : verdict === 'stopped' ? 'stopped' : 'failed';
  const detailOf =
    run.ending === 'caller'
      ? 'you stopped it with task_stop'
      : run.ending === 'abort'
        ? 'the agent that asked was stopped'
        : detail;
  const body =
    verdict === 'done' ? reportDigest(answer) : reasonLine(verdict, detailOf);
  if (verdict === 'done') record.digest = body;
  else record.note = body;
  const text = formatTaskResult(
    {
      taskId,
      childId: record.childId,
      label: record.label,
      verdict,
      took: span(record.endedAt - record.startedAt),
    },
    body,
  );
  if (run.callId) {
    void replyDelegateCall(
      run.callId,
      text,
      verdict === 'failed' || verdict === 'timed out',
    ).catch(() => undefined);
    return;
  }
  // A task in the background: its report goes to the caller, unless the user or the caller itself stopped it.
  if (run.ending !== 'user' && run.ending !== 'caller')
    deliverReport(record.caller, {
      label: `${record.label} (task ${taskId})`,
      text: verdict === 'done' ? body : reasonLine(verdict, detailOf),
    });
}

/** Stops a task: its worker's run is killed and the task ends as stopped (or timed out). */
function stopTask(taskId: string, how: Ending, detail = '') {
  const record = delegateTasks[taskId];
  const run = taskRuns.get(taskId);
  if (!record || !run) return;
  run.ending = how;
  // The worker's run ends with an error event that settles the task (see `settle`). One with no run (it waits on an
  // approval card) is ended here.
  stopTurn(record.childId);
  if (taskRuns.has(taskId))
    finishTask(taskId, how === 'timeout' ? 'timed out' : 'stopped', detail, '');
}

/** The Stop button of a task's card. */
export function stopDelegateTask(taskId: string) {
  stopTask(taskId, 'user');
}

/** Reports of background tasks waiting for their caller to be idle, per caller. */
const backlog = new Map<string, { label: string; text: string }[]>();

function deliverReport(
  callerId: string,
  report: { label: string; text: string },
) {
  const list = backlog.get(callerId) ?? [];
  list.push(report);
  backlog.set(callerId, list);
  if (list.length === 1) flushReports(callerId, 0);
}

/** Sends the waiting reports to their caller as one silent hand-back turn, once it is idle (an hour at most). */
function flushReports(callerId: string, tries: number) {
  const list = backlog.get(callerId);
  if (!list?.length) return;
  if (!workspace.sessions.some((item) => item.id === callerId)) {
    backlog.delete(callerId);
    return;
  }
  if (replies[callerId]) {
    if (tries < 3600) setTimeout(() => flushReports(callerId, tries + 1), 1000);
    else backlog.delete(callerId);
    return;
  }
  backlog.delete(callerId);
  const asked = messagesOf(callerId).findLast(
    (message) => message.role === 'user' && !hiddenPrompt(message),
  );
  const task =
    shorten(
      askText(asked ?? { id: '', role: 'user', time: '', blocks: [] }),
      1500,
    ) || 'the one you were working on';
  send(callerId, '', [], [], [], {
    prompt: handbackPrompt({ results: list, task }),
    silent: NO_HANDOUT,
  });
}

/** A caller's turn ended: the tasks it was waiting for have nobody to answer, so they stop. Background ones go on. */
function endWaitingTasks(callerId: string) {
  for (const record of Object.values(delegateTasks)) {
    if (record.caller === callerId && record.wait && taskRuns.has(record.id))
      stopTask(record.id, 'abort');
  }
}

/** A call of one of the delegation tools, from the Rust side. */
function onDelegateCall(call: DelegateCall) {
  const reply = (text: string, isError = false) =>
    void replyDelegateCall(call.callId, text, isError).catch(() => undefined);
  if (call.tool === 'list_agents') return reply(formatAgents(agentRows()));
  if (call.tool === 'delegate_task') return startTask(call, reply);
  const id = call.args.task_id;
  const record = isTaskId(id) ? delegateTasks[id] : undefined;
  if (!record)
    return reply(
      `No task ${String(id)}: its id is the one delegate_task returned.`,
      true,
    );
  if (call.tool === 'task_status') return reply(statusText(record));
  // task_stop
  if (taskRuns.has(record.id)) stopTask(record.id, 'caller');
  reply(statusText(record));
}

if (inTauri()) {
  // The page before this one is gone with its chats: calls still waiting for it are answered with an error.
  void delegateReady().catch(() => undefined);
  void listenDelegateCalls(onDelegateCall);
  void listenWorkerSlots(startQueued);
  // The Preview agent's memory of pages (page-memory.ts): its tools read and write the store through this.
  void startPageMemoryBridge();
  void listenDelegateAborts((callId) => {
    for (const [taskId, run] of taskRuns)
      if (run.callId === callId) stopTask(taskId, 'abort');
  });
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
const shorten = (text: string, max: number) =>
  text.length > max ? `${text.slice(0, max)}…` : text;

/** The line a step ends with to send its input back to the step(s) before it. */
const REPLAN_MARK = 'REPLAN:';
const MAX_REPLANS = 1;
const replanOf = (text: string) => {
  const found = /^\s*REPLAN:\s*(.+)$/m.exec(text);
  if (found) return found[1].trim();
  // The handoff can say it too: STATUS: needs-plan-change, with what is missing in OPEN.
  if (/^\s*STATUS:\s*needs-plan-change/im.test(text))
    return (
      /^\s*OPEN:\s*(.+)$/im.exec(text)?.[1].trim() || 'the plan needs changes'
    );
  return null;
};

/** The section a step ends with: short labelled lines the next agent reads at a glance, never prose. */
const HANDOFF_RULE =
  'End with "## Handoff" (<=120 words, only this is passed on; "-" if empty): STATUS: done|blocked|needs-plan-change / RESULT: <1 line> / FILES: <paths> / FACTS: <<=5, "; "-separated, path:line> / OPEN: <<=3> / VERIFY: <cmd> -> pass|fail|not run / NEXT: <first thing the next step does>. Telegraphic: no filler, no restating the task.';

/** Told to every agent of a run: the user's language, not the language of these instructions. */
const LANGUAGE_RULE =
  "Reply in the user's language (these notes are English); code, paths, commands unchanged.";

const stepWaiters = new Map<
  string,
  (text: string, error: string | null, reply: Message) => void
>();
/** Flow steps waiting for the user's answers, by their chat. */
const askingCards = new Map<string, DelegateCard>();

/** Sends the answers to questions an agent asked (in its own chat, or a flow step's from the Flow steps card). */
export function answerQuestions(sessionId: string, text: string): boolean {
  // A turn is running (a task's report started one, say): the answers cannot go out now, and the card must not say they did.
  if (!text.trim() || replies[sessionId]) return false;
  const card = askingCards.get(sessionId);
  if (card) {
    card.state = 'running';
    askingCards.delete(sessionId);
  }
  send(sessionId, text, [], []);
  return true;
}

const passes = (step: WorkflowStep, previous: string) => {
  const { mode, text } = step.condition;
  if (mode === 'always' || !text.trim()) return true;
  const found = previous.toLowerCase().includes(text.trim().toLowerCase());
  return mode === 'contains' ? found : !found;
};

/** Reads (no `content`) or writes one file of a run's notes; '' when it cannot. */
async function runFile(
  root: string | null,
  run: string,
  name: string,
  content?: string,
): Promise<string> {
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    const args = {
      root,
      run,
      name,
      ...(content === undefined ? {} : { content }),
    };
    const out = await invoke(
      content === undefined ? 'run_file_read' : 'run_file_write',
      args,
    );
    return typeof out === 'string' ? out : '';
  } catch {
    return '';
  }
}

const slugOf = (text: string) =>
  text
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-|-$/g, '')
    .slice(0, 32) || 'step';

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

type FlowRunContext = {
  root: string | null;
  folder: string;
  board: string[];
  slugs: Map<string, string>;
  handoffs: Map<string, string>;
};

function boardText(
  task: string,
  preset: WorkflowPreset,
  ctx: FlowRunContext,
  groups: WorkflowStep[][],
): string {
  const plan = groups
    .map(
      (group, stage) =>
        `${stage + 1}. ${group.map((step) => `${step.name} (${agentLabel(step.agent, step.model)}) — notes: agents/${ctx.slugs.get(step.id)}.md`).join(' | ')}`,
    )
    .join('\n');
  return [
    `# Run board — ${preset.name}`,
    '',
    `Goal: ${firstLineOf(task, 300)}`,
    '',
    '## Plan (one line per stage; steps of a stage run at the same time)',
    plan,
    '',
    '## Handoffs',
    ...(ctx.board.length ? ctx.board : ['(none yet)']),
    '',
  ].join('\n');
}

function runStep(
  main: SessionItem,
  asked: Message,
  run: DelegateRun,
  step: WorkflowStep,
  brief: string,
  task: string,
  previous: string,
  images: Attachment[],
  elements: PageElement[],
  files: FileAttachment[],
  queued?: { uuid: string; card: DelegateCard },
) {
  const agent = step.agent;
  const model = step.model || MODELS[agent]?.[0]?.value || '';
  // Replan attempts stay in the same provider conversation. This preserves context and avoids paying a new
  // system/tool-definition prefix for a correction that belongs to the same step.
  const prior = [...run.nodes]
    .reverse()
    .find((item) => item.stepId === step.id && item.agentSession);
  const uuid = prior?.agentSession ?? queued?.uuid ?? crypto.randomUUID();
  const id = `claude:${uuid}`;
  if (!workspace.sessions.some((item) => item.id === id)) {
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
  }
  // Its card was made when the run began, as "queued": now it is working.
  let card: DelegateCard;
  if (queued) {
    card = queued.card;
    card.state = 'running';
    // A step run again (its plan was sent back) resumes its existing chat: the card follows that conversation.
    card.childId = id;
    card.note = undefined;
  } else {
    asked.blocks.push({
      kind: 'delegate',
      childId: id,
      agent,
      model,
      state: 'running',
      step: step.name,
    });
    card = asked.blocks[asked.blocks.length - 1] as DelegateCard;
  }
  const label = `${step.name} · ${agentLabel(agent, model)}`;
  run.nodes.push({
    childId: id,
    stepId: step.id,
    agent,
    model,
    label,
    startedAt: Date.now(),
    state: 'running',
    input: previous,
    attempt: (prior?.attempt ?? 0) + 1,
    agentSession: uuid,
  });
  const node = run.nodes[run.nodes.length - 1];
  const asks = new Map<string, number>();
  return new Promise<{
    step: WorkflowStep;
    label: string;
    text: string;
    ok: boolean;
  }>((resolve) => {
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
      const target =
        ask && (asks.get(id) ?? 0) < MAX_ASKS
          ? run.nodes.findLast(
              (item) =>
                item.state === 'done' &&
                item.childId !== id &&
                item.label.split(' · ')[0].toLowerCase() ===
                  ask.to.toLowerCase(),
            )
          : undefined;
      if (ask && !target)
        logError('flow', `step "${step.name}" asked @${ask.to}`, (asks.get(id) ?? 0) >= MAX_ASKS ? 'ask limit reached' : 'no finished step with that name', { run: run.id, ...logCtx(id) });
      if (ask && target) {
        asks.set(id, (asks.get(id) ?? 0) + 1);
        (node.talks ??= []).push({
          to: ask.to,
          question: ask.question,
          at: Date.now(),
        });
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
          logTalk(
            main.id,
            main.title,
            workspace.projects.find((project) => project.id === main.projectId)
              ?.name ?? '',
            step.name,
            ask.to,
            ask.question,
            talk.answer,
          );
          keepHandout(main.id, asked);
          send(id, `A from ${ask.to}: ${talk.answer}`, [], []);
        });
        send(
          target.childId,
          `Q from ${step.name}: ${ask.question}\nAnswer in at most 60 words, only the answer.`,
          [],
          [],
        );
        return;
      }
      card.questions = undefined;
      const ok = !error && text.length > 0;
      card.state = ok ? 'done' : 'failed';
      card.note = ok ? undefined : (error ?? 'no answer');
      if (!ok) logError('flow', `step "${step.name}" failed`, error ?? 'no answer', { run: run.id, ...logCtx(id) });
      node.state = ok ? 'done' : 'failed';
      node.note = card.note;
      node.endedAt = Date.now();
      node.stats = statsOf(reply, node.startedAt);
      node.output = text;
      node.handoff = parseHandoff(text);
      keepHandout(main.id, asked);
      resolve({
        step,
        label,
        text: ok ? text : `(it did not answer: ${error ?? 'no text'})`,
        ok,
      });
    };
    stepWaiters.set(id, settle);
    send(id, task, images, elements, [], { preface: brief, files });
  });
}

async function runWorkflow(
  main: SessionItem,
  asked: Message,
  preset: WorkflowPreset,
  task: string,
  preface: string,
  images: Attachment[],
  elements: PageElement[],
  files: FileAttachment[] = [],
) {
  if (preset.mode === 'team' && preset.steps.length >= 2)
    return runTeam(main, asked, preset, task, preface, images, elements, files);
  const runId = crypto.randomUUID();
  const graph = normalizeGraph(preset.steps);
  const links = inputsOf(graph);
  const groups = stepGroups(graph);
  delegateRuns.push({
    id: runId,
    main: main.id,
    task,
    preset: preset.name,
    plan: JSON.parse(JSON.stringify(graph)),
    skipped: [],
    startedAt: Date.now(),
    nodes: [],
    handback: 'waiting',
  });
  if (delegateRuns.length > RUN_LIMIT)
    delegateRuns.splice(0, delegateRuns.length - RUN_LIMIT);
  const run = delegateRuns[delegateRuns.length - 1];
  const batch: DelegateBatch = {
    main: main.id,
    task,
    asked,
    pending: new Set(),
    results: [],
    run,
    orchestrator: preset.orchestrator.trim() || undefined,
  };
  main.status = 'working';

  const root =
    workspace.projects.find((project) => project.id === main.projectId)?.path ??
    null;
  const slugs = new Map<string, string>();
  graph.forEach((step, index) =>
    slugs.set(step.id, `${index + 1}-${slugOf(step.name)}`),
  );
  const ctx: FlowRunContext = {
    root,
    folder: '',
    board: [],
    slugs,
    handoffs: new Map(),
  };
  const boardPath = await runFile(
    root,
    runId,
    'BOARD.md',
    boardText(task, preset, ctx, groups),
  );
  ctx.folder = boardPath ? boardPath.replace(/[\\/]BOARD\.md$/, '') : '';
  run.folder = ctx.folder || undefined;

  // Every step has its card in the message from the start: the ones that wait say "Queued", the ones of a stage that
  // runs together all say "Working" once it starts.
  const cards = new Map<string, { uuid: string; card: DelegateCard }>();
  for (const step of graph) {
    const uuid = crypto.randomUUID();
    asked.blocks.push({
      kind: 'delegate',
      childId: `claude:${uuid}`,
      agent: step.agent,
      model: step.model || MODELS[step.agent]?.[0]?.value || '',
      state: 'queued',
      step: step.name,
    });
    cards.set(step.id, {
      uuid,
      card: asked.blocks[asked.blocks.length - 1] as DelegateCard,
    });
  }
  keepHandout(main.id, asked);

  let first = true;
  // A step can send its input back (REPLAN): the steps that fed it run again with the reason, at most MAX_REPLANS times.
  const replanNotes = new Map<string, string>();
  let replans = 0;
  // Keep parallel fan-out useful without turning a normal flow into a provider swarm. A preset may lower or raise
  // this deliberately, but the app keeps the value bounded so an imported preference cannot exhaust the machine.
  const maxParallel = Math.max(
    1,
    Math.min(6, Math.round(preset.coordination?.maxParallel ?? 3)),
  );
  const maxReplans = Math.max(
    0,
    Math.min(3, Math.round(preset.coordination?.maxReplans ?? MAX_REPLANS)),
  );
  for (let stage = 0; stage < groups.length; stage++) {
    const group = groups[stage];
    const inputText = (step: WorkflowStep) =>
      (links[step.id] ?? [])
        .filter((from) => from !== 'start' && ctx.handoffs.has(from))
        .map(
          (from) =>
            `### ${graph.find((item) => item.id === from)?.name ?? from}\n${ctx.handoffs.get(from)}`,
        )
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
    const done: {
      step: WorkflowStep;
      label: string;
      text: string;
      ok: boolean;
    }[] = [];
    for (let offset = 0; offset < active.length; offset += maxParallel) {
      const batch = await Promise.all(
        active.slice(offset, offset + maxParallel).map((step) => {
          const slug = slugs.get(step.id) ?? slugOf(step.name);
          const previous = inputText(step);
          const others = active
            .filter((item) => item !== step)
            .map(
              (item) =>
                `"${item.name}" (notes: agents/${slugs.get(item.id)}.md)`,
            );
          const where = ctx.folder || '.agentdeck/runs/' + runId;
          const fed = (links[step.id] ?? []).some((from) => from !== 'start');
          // Written for the agent, not the user: short labelled lines. What the user reads is the chat itself.
          const brief = [
            preface,
            `Step "${step.name}" of an AgentDeck run (orchestrator: ${agentLabel(main.agent)}). Do only this job:`,
            step.prompt.trim() || 'Do your part of the task.',
            `Run folder: ${where}. BOARD.md = goal, plan, handoffs (app-written, read-only${ctx.folder ? '; read it first' : '; below'}). Notes: agents/${slug}.md, yours only, append one line each: [F] fact (path:line) | [Q @name] question | [A @name] answer. Never edit BOARD.md or others' notes.`,
            ctx.folder ? '' : board,
            previous
              ? `IN (from the step before; read its notes only if needed):\n${previous}`
              : '',
            others.length
              ? `Parallel with you: ${others.join(', ')}. Read their notes at start and after each big step, answer [Q @${step.name}] with [A], do not edit files they claim.`
              : '',
            LANGUAGE_RULE,
            QUESTION_RULE,
            fed
              ? `Need one fact only an earlier step knows: end with "ASK @<step name>: <question>" and stop (max ${MAX_ASKS}). Input too unclear or wrong to work from: do not guess, end with "${REPLAN_MARK} <what to fix>" and stop.`
              : '',
            replanNotes.get(step.id)
              ? `Sent back by the next step: "${replanNotes.get(step.id)}". Fix that.`
              : '',
            REVIEW_STEP.test(`${step.name} ${step.prompt}`) ? REVIEW_RULE : '',
            HANDOFF_RULE,
          ]
            .filter((line, index, all) => line !== '' || all[index - 1] !== '')
            .join('\n');
          return runStep(
            main,
            asked,
            run,
            step,
            brief,
            task,
            previous,
            first ? images : [],
            first ? elements : [],
            first ? files : [],
            cards.get(step.id),
          );
        }),
      );
      done.push(...batch);
    }
    first = false;
    for (const item of done) {
      const handoff = item.ok ? handoffOf(item.text) : item.text;
      ctx.handoffs.set(item.step.id, handoff);
      const name = `handoff/${slugs.get(item.step.id)}.md`;
      void runFile(root, runId, name, `# ${item.step.name}\n\n${handoff}\n`);
      ctx.board.push(
        `- ${item.step.name}: ${name}${item.ok ? '' : ' (failed)'}`,
      );
      batch.results.push({ label: item.label, text: handoff });
    }
    void runFile(root, runId, 'BOARD.md', boardText(task, preset, ctx, groups));
    // A step that failed ends the chain: the agent is told what happened so far.
    if (done.some((item) => !item.ok)) {
      logError('flow', 'a stage failed: the run stops', done.filter((item) => !item.ok).map((item) => item.label).join(', '), { run: run.id, stage });
      failSessionWork(main.id);
    }
    if (run.cancelled || done.some((item) => !item.ok)) break;
    for (const step of active) replanNotes.delete(step.id);
    // A step sent its input back: the steps that fed it run again (and every stage after them), with its reason.
    const back = done.find((item) => item.ok && replanOf(item.text) !== null);
    if (back && replans < maxReplans) {
      const feeders = (links[back.step.id] ?? []).filter(
        (from) => from !== 'start',
      );
      const from = Math.min(
        ...feeders
          .map((id) =>
            groups.findIndex((list) => list.some((item) => item.id === id)),
          )
          .filter((index) => index >= 0),
      );
      if (Number.isFinite(from)) {
        replans++;
        const reason = replanOf(back.text) ?? '';
        for (const id of feeders) replanNotes.set(id, reason);
        const again = groups.slice(from, stage + 1).flat();
        // Their cards wait again; runStep resumes each step's existing conversation when it starts.
        for (const item of again) {
          const entry = cards.get(item.id);
          if (!entry) continue;
          entry.card.state = 'queued';
          entry.card.note = feeders.includes(item.id) ? 'Sent back' : undefined;
          cards.set(item.id, { uuid: entry.uuid, card: entry.card });
          ctx.handoffs.delete(item.id);
        }
        batch.results = batch.results.filter(
          (result) =>
            !again.some((item) => result.label.startsWith(`${item.name} ·`)),
        );
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
    failSessionWork(main.id);
    sendNextQueued(main.id);
    return;
  }
  const notes = await Promise.all(
    graph.map((step) =>
      runFile(root, runId, `agents/${slugs.get(step.id)}.md`),
    ),
  );
  const facts = findingsOf(notes);
  batch.memo = [
    facts.length
      ? `Findings the steps shared:\n${facts.map((fact) => `- ${fact}`).join('\n')}`
      : '',
    ctx.folder
      ? `The run's notes are in ${ctx.folder} (BOARD.md, agents/, handoff/): open them only if you need more detail.`
      : '',
  ]
    .filter(Boolean)
    .join('\n\n');
  handBack(batch);
}

// ---------- a team run ----------
// The first step of a `team` preset plans; the others work at the same time, each in its own chat, and tell each other
// what they need or what is ready through a queue (team.ts). The queue, the members' state and a log are saved as
// they change (IndexedDB `team.state`, TEAM.md in the run folder): after a reload the run carries on from there.

const teamStates = $state<Record<string, TeamState>>({});
let teamsLoaded = false;
void getPref<Record<string, TeamState>>('team.state').then((saved) => {
  // Finished runs are kept a day, for the Workflow tab and TEAM.md; the unfinished ones until they end.
  for (const [id, state] of Object.entries(saved ?? {}))
    if (!state.endedAt || Date.now() - state.endedAt < 86_400_000) teamStates[id] = state;
  teamsLoaded = true;
  if (inTauri()) setTimeout(() => void resumeTeams(), 4000);
});
/** The messages a team run sent, for the canvas: who to whom, and whether it still waits in a queue. Reactive. */
export function teamViewOf(runId: string | undefined) {
  const state = runId ? teamStates[runId] : undefined;
  return { sent: (state?.queue ?? []).map((message) => ({ from: message.from, to: message.to, waiting: !message.taken })) };
}
/** Wakes the members of a run that wait for a message (by run id). */
const teamWakers = new Map<string, Set<() => void>>();

function saveTeam(state: TeamState, line?: string) {
  if (line) state.log.push(`${new Date().toISOString().slice(11, 19)} ${line}`);
  if (state.log.length > 400) state.log.splice(0, state.log.length - 400);
  teamStates[state.id] = state;
  if (teamsLoaded) setPref('team.state', $state.snapshot(teamStates));
  void runFile(state.root, state.id, 'TEAM.md', teamText(state));
  for (const wake of [...(teamWakers.get(state.id) ?? [])]) wake();
}

/** One turn of a member's chat that already exists: `text` goes as its message, its answer comes back. */
function teamTurn(childId: string, text: string): Promise<{ text: string; ok: boolean }> {
  return new Promise((resolve) => {
    stepWaiters.set(childId, (answer, error) =>
      resolve({ text: answer, ok: !error && answer.length > 0 }),
    );
    // A chat still in a turn takes no message: it is waited for, then the message goes.
    const go = (tries: number) => {
      if (!replies[childId]) send(childId, text, [], []);
      else if (tries < 600) setTimeout(() => go(tries + 1), 1000);
      else {
        logError('team', 'a member chat stayed busy for 10 min: its message was never sent', text.slice(0, 120), logCtx(childId));
        stepWaiters.delete(childId);
        resolve({ text: '', ok: false });
      }
    };
    go(0);
  });
}

/** Waits until `member` has messages (they are taken and returned) or the run is over (`null`). */
function teamInbox(
  state: TeamState,
  member: TeamMember,
  run: DelegateRun,
): Promise<TeamMessage[] | null> {
  return new Promise((resolve) => {
    const wakers = teamWakers.get(state.id) ?? new Set<() => void>();
    teamWakers.set(state.id, wakers);
    const check = () => {
      const inbox = inboxOf(state, member.name);
      if (!inbox.length && !run.cancelled && !teamFinished(state)) return;
      wakers.delete(check);
      clearInterval(timer);
      if (!inbox.length) return resolve(null);
      const at = Date.now();
      for (const message of inbox) message.taken = at;
      resolve(inbox);
    };
    // Also checked each second: a Stop of the main chat cancels the run without any message.
    const timer = setInterval(check, 1000);
    wakers.add(check);
    check();
  });
}

const TEAM_RULE = (self: string, others: string[]) =>
  [
    `You work in a team with ${others.map((name) => `@${name}`).join(', ')}, at the same time, each in your own chat. Their messages reach you as "FROM @<name>: ...".`,
    'To tell a member something (a part is ready, a shape changed, you need something), add a line "SEND @<name>: <message>" at the end of your answer: short and concrete (paths, endpoints, shapes). Send as soon as something is ready, not only at the end.',
    'End every answer with one line: "STATUS: done" (your part is finished and you need nothing more), "STATUS: waiting @<name>" (nothing more to do until that member answers) or "STATUS: working" (you go on in your next turn).',
    `Do not edit the files another member owns: say what you need instead. You are ${self}.`,
  ].join('\n');

type TeamCards = Map<string, { uuid: string; card: DelegateCard }>;
type TeamContext = { main: SessionItem; asked: Message; run: DelegateRun; cards: TeamCards };

async function runTeam(
  main: SessionItem,
  asked: Message,
  preset: WorkflowPreset,
  task: string,
  preface: string,
  images: Attachment[],
  elements: PageElement[],
  files: FileAttachment[],
) {
  const runId = crypto.randomUUID();
  const [planner, ...crew] = preset.steps;
  const root =
    workspace.projects.find((project) => project.id === main.projectId)?.path ?? null;
  const { run, batch, cards } = teamRun(runId, main, asked, preset, task);
  const boardPath = await runFile(root, runId, 'BOARD.md', `# ${preset.name}\n\nTask: ${task}\n`);
  const folder = boardPath ? boardPath.replace(/[\\/]BOARD\.md$/, '') : '';
  run.folder = folder || undefined;
  const state: TeamState = {
    id: runId,
    main: main.id,
    askedId: asked.id,
    task,
    root,
    folder,
    plan: '',
    members: crew.map((step) => ({
      stepId: step.id,
      name: step.name,
      childId: `claude:${cards.get(step.id)?.uuid}`,
      status: 'new',
      turns: 0,
    })),
    queue: [],
    log: [],
    startedAt: Date.now(),
  };
  saveTeam(state, `start: ${preset.name}`);
  const names = crew.map((step) => step.name);
  const brief = [
    preface,
    `Planner of an AgentDeck team run. Members: ${names.join(', ')}.`,
    planner.prompt.trim() || 'Plan the work for the members.',
    // Any roles (QA, DevOps, a designer…): each member gets the section with its own name, with the shared one.
    `Sections of the plan, exactly these headings: "## Shared" (the contract every member works against), then ${names.map((name) => `"## ${name}"`).join(', ')}: each with that member's ordered tasks, what it can do before the others are ready, and what it must tell whom and when.`,
    `Run folder: ${folder || `.agentdeck/runs/${runId}`}. Write the plan as your answer: the app saves it as PLAN.md and gives each member its part.`,
    LANGUAGE_RULE,
    QUESTION_RULE,
  ]
    .filter(Boolean)
    .join('\n');
  const plan = await runStep(main, asked, run, planner, brief, task, '', images, elements, files, cards.get(planner.id));
  if (!plan.ok || run.cancelled) {
    if (!plan.ok) logError('team', 'the planner failed', plan.text.slice(0, 300) || 'no answer', { run: runId });
    saveTeam(state, plan.ok ? 'stopped' : 'the planner failed');
    return endTeam(state, run, batch, cards);
  }
  state.plan = plan.text;
  void runFile(root, runId, 'PLAN.md', plan.text);
  saveTeam(state, 'plan written');
  const ctx: TeamContext = { main, asked, run, cards };
  await Promise.all(
    state.members.map((member) =>
      teamMember(state, member, crew.find((step) => step.id === member.stepId) as WorkflowStep, names, preface, ctx),
    ),
  );
  endTeam(state, run, batch, cards);
}

/** The run's cards and hand-back batch, for a new run or one resumed after a reload (`uuids`: the members' chats). */
function teamRun(
  runId: string,
  main: SessionItem,
  asked: Message,
  preset: WorkflowPreset,
  task: string,
  uuids?: Map<string, string>,
) {
  delegateRuns.push({
    id: runId,
    main: main.id,
    task,
    preset: preset.name,
    plan: JSON.parse(JSON.stringify(preset.steps)),
    skipped: [],
    startedAt: Date.now(),
    nodes: [],
    handback: 'waiting',
  });
  if (delegateRuns.length > RUN_LIMIT) delegateRuns.splice(0, delegateRuns.length - RUN_LIMIT);
  const run = delegateRuns[delegateRuns.length - 1];
  const batch: DelegateBatch = {
    main: main.id,
    task,
    asked,
    pending: new Set(),
    results: [],
    run,
    orchestrator: preset.orchestrator.trim() || undefined,
  };
  main.status = 'working';
  const cards: TeamCards = new Map();
  for (const step of preset.steps) {
    const uuid = uuids?.get(step.id) ?? crypto.randomUUID();
    const found = asked.blocks.find(
      (block): block is DelegateCard => block.kind === 'delegate' && block.childId === `claude:${uuid}`,
    );
    if (!found)
      asked.blocks.push({
        kind: 'delegate',
        childId: `claude:${uuid}`,
        agent: step.agent,
        model: step.model || MODELS[step.agent]?.[0]?.value || '',
        state: 'queued',
        step: step.name,
      });
    cards.set(step.id, { uuid, card: (found ?? asked.blocks[asked.blocks.length - 1]) as DelegateCard });
  }
  keepHandout(main.id, asked);
  return { run, batch, cards };
}

/** One member: its first turn (or the one a reload cut off), then one turn for each batch of messages it gets. */
async function teamMember(
  state: TeamState,
  member: TeamMember,
  step: WorkflowStep,
  names: string[],
  preface: string,
  ctx: TeamContext,
) {
  const { main, asked, run, cards } = ctx;
  const others = names.filter((name) => name !== member.name);
  const entry = cards.get(step.id);
  const setCard = (patch: Partial<DelegateCard>) => {
    if (!entry) return;
    Object.assign(entry.card, patch);
    keepHandout(main.id, asked);
  };
  let reply: { text: string; ok: boolean } | null = null;
  if (member.status === 'new') {
    member.busy = true;
    member.status = 'working';
    saveTeam(state, `${member.name}: started`);
    const brief = [
      preface,
      `Member "${member.name}" of an AgentDeck team run. Your job:`,
      step.prompt.trim() || 'Do your part of the plan.',
      `Your part of the plan, with the shared part (the whole plan: ${state.folder ? `${state.folder}/PLAN.md` : 'ask the planner'}):\n${planFor(state.plan, member.name)}`,
      TEAM_RULE(member.name, others),
      LANGUAGE_RULE,
    ]
      .filter(Boolean)
      .join('\n');
    const first = await runStep(main, asked, run, step, brief, state.task, '', [], [], [], entry);
    reply = { text: first.text, ok: first.ok };
  } else if (member.busy) {
    // A reload cut this member's turn off: it goes on, with what waits for it.
    const waiting = inboxOf(state, member.name);
    for (const message of waiting) message.taken = Date.now();
    setCard({ state: 'running', note: 'Resumed after a restart' });
    saveTeam(state, `${member.name}: resumed`);
    reply = await teamTurn(
      member.childId,
      ['AgentDeck was restarted while you worked: check what you had already done, then go on with your part.', inboxText(waiting)]
        .filter(Boolean)
        .join('\n'),
    );
  }
  for (;;) {
    if (reply) {
      member.busy = false;
      member.turns++;
      member.last = reply.text;
      if (!reply.ok) {
        member.status = 'failed';
        logError('team', `${member.name}: turn failed`, reply.text.slice(0, 300) || 'no answer', { run: state.id, member: member.name, turns: member.turns });
        setCard({ state: 'failed', note: 'Turn failed' });
        saveTeam(state, `${member.name}: turn failed`);
        break;
      }
      const parsed = parseTeamReply(reply.text, member.name, names);
      for (const message of parsed.sends)
        state.queue.push({ id: nextId(), from: member.name, to: message.to, text: message.text, at: Date.now() });
      const lost = (reply.text.match(/^\s*[-*>]?\s*\**SEND\s+@/gim)?.length ?? 0) - parsed.sends.length;
      if (lost > 0) logError('team', `${member.name}: ${lost} message(s) to an unknown member or to itself`, undefined, { run: state.id, member: member.name, known: names.join(',') });
      if (!/^\s*[-*>]?\s*\**STATUS:/im.test(reply.text)) logError('team', `${member.name}: answer without a STATUS line`, 'treated as working', { run: state.id, member: member.name });
      const spent = member.turns >= MAX_TEAM_TURNS;
      if (spent) logError('team', `${member.name}: turn limit reached`, `${MAX_TEAM_TURNS} turns`, { run: state.id, member: member.name });
      member.status = spent ? 'failed' : parsed.status;
      member.waitingOn = parsed.waitingOn;
      setCard({
        state: member.status === 'failed' ? 'failed' : member.status === 'done' ? 'done' : 'running',
        note: spent ? 'Turn limit reached' : member.status === 'waiting' ? `Waiting for ${parsed.waitingOn ?? 'a message'}` : undefined,
      });
      saveTeam(
        state,
        `${member.name}: ${member.status}${parsed.sends.length ? `, sent to ${parsed.sends.map((item) => `@${item.to}`).join(' ')}` : ''}`,
      );
      if (member.status === 'failed' || run.cancelled) break;
    }
    // `working`: it goes on at once, with whatever came in meanwhile; otherwise it waits for a message.
    let inbox: TeamMessage[] | null;
    if (member.status === 'working') {
      inbox = inboxOf(state, member.name);
      for (const message of inbox) message.taken = Date.now();
    } else inbox = await teamInbox(state, member, run);
    if (inbox === null || run.cancelled) break;
    member.busy = true;
    member.status = 'working';
    setCard({
      state: 'running',
      note: inbox.length ? `Message from ${[...new Set(inbox.map((item) => item.from))].join(', ')}` : undefined,
    });
    saveTeam(state, `${member.name}: turn ${member.turns + 1}${inbox.length ? ` with ${inbox.length} message(s)` : ''}`);
    reply = await teamTurn(member.childId, inbox.length ? inboxText(inbox) : 'Go on with your part.');
  }
  member.busy = false;
  // A member that stops early wakes the others, so none waits on it for ever.
  saveTeam(state);
}

/** Every member has stopped: the main chat gets what each one did, and the run is closed. */
function endTeam(state: TeamState, run: DelegateRun, batch: DelegateBatch, cards: TeamCards) {
  state.endedAt = Date.now();
  const stuck = state.members.filter((member) => member.status === 'waiting' && !member.waitingOn);
  if (stuck.length || state.members.some((member) => member.status === 'failed'))
    logError('team', 'run over with problems', state.members.map((member) => `${member.name}=${member.status}${member.waitingOn ? `@${member.waitingOn}` : ''}`).join(' '), { run: state.id, queued: state.queue.filter((message) => !message.taken).length });
  saveTeam(state, 'run over');
  for (const { card } of cards.values())
    if (card.state === 'queued') Object.assign(card, { state: 'skipped', note: 'Not run' });
  keepHandout(batch.main, batch.asked);
  if (run.cancelled) return endCancelled(batch.main, run);
  batch.results = state.members
    .filter((member) => member.last)
    .map((member) => ({ label: `${member.name} (${member.status})`, text: handoffOf(member.last ?? '') }));
  if (!batch.results.length) {
    const main = workspace.sessions.find((item) => item.id === batch.main);
    if (main) main.status = 'idle';
    run.handback = 'failed';
    run.endedAt = Date.now();
    failSessionWork(batch.main);
    sendNextQueued(batch.main);
    return;
  }
  const sent = state.queue.map((message) => `- ${message.from} -> ${message.to}: ${message.text}`);
  batch.memo = [
    sent.length ? `What the members told each other:\n${sent.slice(-40).join('\n')}` : '',
    state.folder ? `The run's files are in ${state.folder} (PLAN.md, TEAM.md with the queue and the log).` : '',
  ]
    .filter(Boolean)
    .join('\n\n');
  handBack(batch);
}

/** Runs cut off by a reload go on: each member takes up its queue, or the turn it was in, where it stopped. */
async function resumeTeams() {
  for (const state of Object.values(teamStates)) {
    if (state.endedAt) continue;
    const main = workspace.sessions.find((item) => item.id === state.main);
    const preset = allPresets().find(
      (item) =>
        item.mode === 'team' &&
        item.steps.length === state.members.length + 1 &&
        item.steps.slice(1).every((step, index) => step.name === state.members[index].name),
    );
    const asked = main
      ? (messagesOf(main.id).find((message) => message.id === state.askedId) ??
        otherTurns[main.id]?.find((message) => message.id === state.askedId))
      : undefined;
    if (!main || !preset || !state.plan || !asked) {
      state.endedAt = Date.now();
      saveTeam(state, 'could not be resumed after a restart');
      continue;
    }
    const uuids = new Map(state.members.map((member) => [member.stepId, member.childId.replace(/^claude:/, '')]));
    const { run, batch, cards } = teamRun(state.id, main, asked, preset, state.task, uuids);
    run.folder = state.folder || undefined;
    saveTeam(state, 'resumed after a restart');
    const names = state.members.map((member) => member.name);
    const ctx: TeamContext = { main, asked, run, cards };
    void Promise.all(
      state.members.map((member) => {
        if (member.status === 'done' || member.status === 'failed' || member.status === 'new') return Promise.resolve();
        // Not in a turn when the app closed: it waits for its messages like before.
        if (!member.busy) member.status = 'waiting';
        const step = preset.steps.find((item) => item.id === member.stepId) as WorkflowStep;
        return teamMember(state, member, step, names, '', ctx);
      }),
    ).then(() => endTeam(state, run, batch, cards));
  }
}

/** The main agent finished the turn that got the answers: the run is over. */
function onHandbackDone(sessionId: string, reply: Message, failed: boolean) {
  const run = delegateRuns.find(
    (item) => item.main === sessionId && item.handback === 'running',
  );
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

/** Longest answer put whole into the hand-back prompt; the rest stays in the agent's own chat. */
const HANDBACK_ANSWER_MAX = 8000;
/** An answer that says the work failed, is blocked or needs a new plan: then the lead has to act, not only summarise. */
const TROUBLE = /VERDICT:\s*FAIL|STATUS:\s*(?:blocked|needs-plan-change)/i;

/** The prompt that hands the answers of the agents asked back to the chat's own agent. */
function handbackPrompt(
  batch: Pick<DelegateBatch, 'results' | 'memo' | 'orchestrator' | 'task'>,
): string {
  const cut = (text: string) =>
    text.length > HANDBACK_ANSWER_MAX
      ? `${text.slice(0, HANDBACK_ANSWER_MAX)}\n… (cut here: the rest is in that agent's own chat)`
      : text;
  const answers = batch.results
    .map((result) => `### ${result.label}\n${cut(result.text)}`)
    .join('\n\n');
  // When nothing went wrong the lead only reads the answers and replies, like a manager reading a report: no tools, no
  // second round of the work. It re-checks and fixes (see `owner`) only when an answer reports trouble.
  const trouble = batch.results.some((result) => {
    const handoff = parseHandoff(result.text);
    return (
      handoff?.status === 'blocked' ||
      handoff?.status === 'needs-plan-change' ||
      TROUBLE.test(result.text)
    );
  });
  const memo = batch.memo ? `\n\n${batch.memo}` : '';
  const finish = batch.orchestrator
    ? `${batch.orchestrator}\n\nThe user's request: ${batch.task}`
    : `Using their answers, finish the user's request: ${batch.task}`;
  // A step that reviews can find the work wrong. The agent that gets the answers then owns the result: it fixes what is
  // wrong and cleans up after the steps, instead of only reporting it.
  const owner =
    "You own the result. Fix only what blocks the request: a step's VERDICT: FAIL, a failed check, or work that is wrong/unfinished. Then do not just report: see all changes (handoff FILES; git status, git diff), undo wrong ones, delete leftovers (debug output, stray files, notes), redo correctly, run the project's checks, end with what you fixed/removed. Suggestions and risks a step lists are not failures: do not edit for them, list them for the user in one short line each. If all is right, say so in one line.";
  const light =
    "The answers above are compact digests; each full answer is in that agent's own chat. Do not redo or re-check their work: run no commands, read no files, look at no git state. Write the reply to the user from the digests: combine them, say in a line where they disagree, what was done, and what the user should do next. Short.";
  return `${HANDBACK_HEAD}\n\n${answers}${memo}\n\n${finish}\n\n${trouble ? owner : light}\n\n${LANGUAGE_RULE}`;
}

function handBack(batch: DelegateBatch) {
  const prompt = handbackPrompt(batch);
  const go = (tries: number) => {
    if (!replies[batch.main]) {
      batch.run.handback = 'running';
      batch.run.handbackAt = Date.now();
      // Silent: the agent gets the prompt but the chat shows no message for it. The delegate card (on the message that
      // handed work out) is the wait state, and the agent's reply follows it as if it were the same turn.
      send(batch.main, '', [], [], [], { prompt, silent: batch.asked });
    }
    // The chat is busy with something else: the answers are not kept waiting for it, they are worked on at once, beside it.
    else if (handBackBeside(batch, prompt)) return;
    else if (tries < 900) setTimeout(() => go(tries + 1), 1000);
    else logError('handback', 'the answers never reached the chat (it stayed busy for 15 min)', undefined, { chat: batch.main, run: batch.run.id });
  };
  go(0);
}

// ---------- a hand-back that does not wait ----------
// The agents asked have answered while the chat is busy with another turn. That turn cannot be joined (one CLI
// conversation runs one turn at a time), so the answers go to a turn of their own, running beside it: it knows the chat so
// far, has no session and no tab, and its reply sits in the chat as a message (open its steps to see what it did).
// The same runs a message the user sends while the chat is busy (`sendBeside`): it is answered at once, beside the turn
// that is still working, instead of waiting in the queue.
const sideTurns = new Map<
  string,
  {
    main: string;
    reply: Message;
    turn: string;
    /** `handback`: the answers of other agents; `beside`: a message of the user. */
    kind: 'handback' | 'beside';
    onDone?: (reply: Message, failed: boolean) => void;
  }
>();
let sideCount = 0;

/** A side hand-back still occupies the main chat until its result arrives. */
function hasSideTurn(main: string): boolean {
  for (const side of sideTurns.values()) if (side.main === main) return true;
  return false;
}

const sideTurnsOf = (main: string) =>
  [...sideTurns.values()].filter((side) => side.main === main).length;

function handBackBeside(batch: DelegateBatch, prompt: string): boolean {
  if (!runBeside(batch.main, forkDigest(messagesOf(batch.main)), prompt, 'handback'))
    return false;
  batch.run.handback = 'running';
  batch.run.handbackAt = Date.now();
  return true;
}

/** Starts a turn of its own beside the chat `main`: `context` is what it knows of the chat, `prompt` what to do now. */
function runBeside(
  main: string,
  context: string,
  prompt: string,
  kind: 'handback' | 'beside',
  onDone?: (reply: Message, failed: boolean) => void,
): boolean {
  const session = workspace.sessions.find((item) => item.id === main);
  // A custom provider keeps its conversation in the app: it has no CLI turn to run beside another one.
  if (!session || session.agent === 'custom' || !inTauri()) return false;
  const id = `${main}#side${++sideCount}`;
  const turn = nextId();
  const list = messagesOf(main);
  const model = modelFor(session);
  const reply: Message = {
    id: nextId(),
    role: 'assistant',
    time: clock(new Date().toISOString()),
    at: new Date().toISOString(),
    blocks: [],
    live: true,
    startedAt: Date.now(),
    agent: session.agent,
    model: model ?? session.model,
  };
  list.push(reply);
  const stored = list[list.length - 1]; // the stored (reactive) object
  sideTurns.set(id, { main, reply: stored, turn, kind, onDone });
  session.status = 'working';
  const cwd =
    workspace.projects.find((project) => project.id === session.projectId)
      ?.path ?? null;
  void sendAgentTurn({
    session: id,
    turn,
    agent: session.agent,
    prompt: context + prompt,
    keepOpen: false,
    // A conversation of its own: Claude takes the id we choose, the others name theirs on the first turn.
    agentSession: session.agent === 'claude' ? crypto.randomUUID() : null,
    resume: false,
    model,
    permissionMode: PERMISSION_MODE[turnOptions.permission],
    effort: (session.stepEffort ?? turnOptions.effort) as Effort,
    allowedTools: [...(approved[main] ?? [])],
    cwd,
  }).catch((error: unknown) =>
    handleSideEvent(id, { kind: 'error', message: String(error) }, turn),
  );
  return true;
}

const clipText = (text: string, max: number) => {
  const flat = text.replace(/\s+/g, ' ').trim();
  return flat.length > max ? `${flat.slice(0, max - 1)}…` : flat;
};

/** One event of a turn running beside its chat: it fills that reply, and ends it. */
function handleSideEvent(id: string, event: AgentEvent, turn: string) {
  const side = sideTurns.get(id);
  if (!side || side.turn !== turn || event.kind === 'raw') return;
  const { reply, main } = side;
  if (event.kind === 'usage') {
    addUsage(reply, event);
    return;
  }
  if (event.kind === 'done' && event.cost_usd != null)
    reply.cost = (reply.cost ?? 0) + event.cost_usd;
  const update = applyAgentEvent(reply.blocks, event);
  reply.blocks = update.blocks;
  if (event.kind === 'tool_result' && event.image)
    persistCommandImage(main, reply, event.id, event.image);
  if (!update.finished) return;
  reply.live = false;
  if (reply.startedAt) reply.ms = Date.now() - reply.startedAt;
  void finishCommandImages(main, reply);
  sideTurns.delete(id);
  const session = workspace.sessions.find((item) => item.id === main);
  if (session)
    session.status = replies[main] || hasSideTurn(main) ? 'working' : 'idle';
  if (session?.path) keepOtherTurn(session, reply);
  if (side.kind === 'handback') onHandbackDone(main, reply, event.kind === 'error');
  side.onDone?.(reply, event.kind === 'error');
  persistLocal();
  if (!replies[main] && !hasSideTurn(main)) sendNextQueued(main);
}

/**
 * Send from the message box. While the agent is busy: wait in line (Enter), or stop it and go first (Ctrl+Enter).
 */
/** The picture of each picked element, sent with the message as an image next to its markup. */
const elementPictures = (elements: PageElement[]): Attachment[] =>
  elements.flatMap((item) =>
    item.picture
      ? [
          {
            id: crypto.randomUUID(),
            name: `${item.tag}.png`,
            mime: 'image/png',
            src: item.picture,
          },
        ]
      : [],
  );

export function sendMessage(
  text: string,
  images: Attachment[] = [],
  elements: PageElement[] = [],
  mentions: Mention[] = [],
  whileRunning: 'queue' | 'interrupt' = 'queue',
  files: FileAttachment[] = [],
) {
  const session = activeSession();
  images = [...images, ...elementPictures(elements)];
  if (
    !session ||
    (!text.trim() &&
      images.length === 0 &&
      elements.length === 0 &&
      files.length === 0)
  )
    return;
  // A task for another agent needs words to say what the task is.
  if (mentions.some((mention) => mention.kind === 'ai') && !text.trim()) return;
  beginSessionWork(session.id);
  // Questions are waiting and nothing runs: writing a message is the way to skip them. It goes out at once (not behind
  // the questions), says so to the agent, and the card closes because a newer message of the user follows it.
  // The same goes for a permission request: writing instead of answering it cancels it, and the agent is told the command
  // did not run.
  if (!replies[session.id] && !isRunning(session.id)) {
    const waiting = pendingInput(session.id);
    if (waiting === 'question' || waiting === 'permission') {
      if (waiting === 'permission') closePendingPermissions(session.id);
      send(session.id, text, images, elements, mentions, {
        files,
        preface: agentdeckLead(waiting === 'permission' ? SKIPPED_PERMISSION : openQuestions(session.id).some((question) => question.required) ? SKIPPED_REQUIRED : SKIPPED_QUESTIONS),
      });
      return;
    }
  }
  if (
    replies[session.id] ||
    isRunning(session.id) ||
    needsUserInput(session.id)
  ) {
    if (whileRunning !== 'interrupt') {
      // Claude takes the message into the turn that is running: the work goes on and reads it at its next step.
      if (canInject(session.id, { text, images, elements, mentions })) {
        void injectNow(session.id, { text, images, elements, mentions, files });
        return;
      }
      enqueue(queued, session.id, {
        text,
        images,
        elements,
        mentions,
        files,
      });
      return;
    }
    if (needsUserInput(session.id)) {
      enqueue(queued, session.id, {
        text,
        images,
        elements,
        mentions,
        files,
      });
      return;
    }
    stopTurn(session.id, true);
  }
  send(session.id, text, images, elements, mentions, {
    files,
  });
}

/** Whether a message can go into the turn that is running: Claude's turn on a kept process, text and files only. */
function canInject(sessionId: string, item: Pick<Queued, 'text' | 'images' | 'elements' | 'mentions'>): boolean {
  const session = workspace.sessions.find((entry) => entry.id === sessionId);
  return (
    inTauri() &&
    session?.agent === 'claude' &&
    Boolean(replies[sessionId]) &&
    !needsUserInput(sessionId) &&
    item.images.length === 0 &&
    item.elements.length === 0 &&
    !(item.mentions ?? []).some((mention) => mention.kind === 'ai') &&
    item.text.trim().length > 0
  );
}

/** Writes a message into the running turn. It shows in the chat at once; when the turn did not take it (it ended, or it
 *  does not run on a kept process) the message goes back to the queue, to be sent with the others when the turn is done. */
async function injectNow(sessionId: string, item: Queued) {
  const list = messagesOf(sessionId);
  const files = item.files ?? [];
  const shown = userMessage(item.text.trim(), [], [], files);
  list.push(shown);
  persistLocal();
  const attached = files.length ? `\n\nAttached files: ${files.map((file) => file.path).join(', ')}` : '';
  let taken = false;
  try {
    taken = await injectAgentMessage(sessionId, mentionContext(item.mentions ?? []) + item.text.trim() + attached);
  } catch {
    taken = false;
  }
  if (taken) return;
  const index = list.findIndex((message) => message.id === shown.id);
  if (index !== -1) list.splice(index, 1);
  enqueue(queued, sessionId, item);
  persistLocal();
  if (!replies[sessionId] && !isRunning(sessionId)) sendNextQueued(sessionId);
}

/** Sends a queued message now: the reply running in the chat is stopped (what it wrote stays), and this one goes out. */
export function steerQueued(sessionId: string, item: Queued) {
  beginSessionWork(sessionId);
  const rest = (queued[sessionId] ?? []).filter((entry) => entry !== item);
  if (rest.length) queued[sessionId] = rest;
  else delete queued[sessionId];
  if (replies[sessionId]) stopTurn(sessionId, true);
  send(sessionId, item.text, item.images, item.elements, item.mentions, {
    files: item.files,
  });
}

/** Messages waiting in a chat (sent while its reply runs, not yet started). */
export const queuedCount = (sessionId: string) => queued[sessionId]?.length ?? 0;

/** Sends a delegated worker its task again: the last message of its own chat that was written by the user. For a worker
 *  that failed or was interrupted; the new reply goes in the worker's chat, where the run can open it. */
export function retryWorker(childId: string) {
  const last = [...messagesOf(childId)].reverse().find((message) => message.role === 'user');
  const text = last?.blocks.map((block) => (block.kind === 'response' ? block.text : '')).filter(Boolean).join('\n\n') ?? '';
  if (!text.trim()) return;
  beginSessionWork(childId);
  send(childId, text, [], [], [], { files: [] });
}

/** A message as it is recognised again in the chat's file (where it has another id than the one it got here). */
const signature = (message: Message) =>
  `${message.role}|${message.blocks
    .map((block) => (block.kind === 'response' ? block.text.trim() : ''))
    .join('')
    .slice(0, 300)}`;

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
  beginSessionWork(session.id);
  const list = messagesOf(session.id);
  const index = list.findIndex(
    (message) => message.id === messageId && message.role === 'user',
  );
  if (index === -1) return;
  if (replies[session.id]) stopTurn(session.id, true);
  delete queued[session.id];
  // The pictures, page elements and files sent with it go again too: an edit changes the words, not what was attached.
  const { images, elements, files } = attachmentsOf(list[index]);
  const removed = list.splice(index);
  if (session.path) {
    hidden[session.id] = [
      ...new Set([
        ...(hidden[session.id] ?? []),
        ...removed.flatMap((message) => [message.id, signature(message)]),
      ]),
    ];
    setPref('chat.hidden', hidden);
    const others = otherTurns[session.id];
    if (others)
      otherTurns[session.id] = others.filter(
        (message) => !removed.includes(message),
      );
    transcripts.delete(session.id);
  } else {
    localMessages[session.id] = list;
    persistLocal();
  }
  // The CLI's conversation still holds the turns just removed: resumed, the agent would see them (and answer them again).
  // Every agent of the chat starts a new one, told what is left of the chat.
  session.agentStarted = false;
  session.agentSession = undefined;
  for (const own of Object.values(session.agentIds ?? {})) {
    own.id = '';
    own.started = false;
  }
  session.forkContext = (session.forkContext ?? '') + handoffDigest(list, EDIT_LEAD);
  pendingNew.add(session.id);
  send(session.id, text, images, elements, [], { files });
}

/** What a message of yours carried besides its words, as composer attachments again (edit, copy all). */
export function attachmentsOf(message: Message): {
  images: Attachment[];
  elements: PageElement[];
  files: FileAttachment[];
} {
  const images: Attachment[] = message.blocks.flatMap((block) =>
    block.kind === 'image'
      ? [
          {
            id: nextId(),
            name: block.name,
            mime: block.src.slice(5, block.src.indexOf(';')) || 'image/png',
            src: block.src,
          },
        ]
      : [],
  );
  const elements: PageElement[] = message.blocks.flatMap((block) =>
    block.kind === 'element'
      ? [
          {
            id: nextId(),
            tag: block.tag,
            text: block.text,
            selector: block.selector,
            html: block.html ?? '',
            around: block.around,
            box: {
              x: block.box?.x ?? 0,
              y: block.box?.y ?? 0,
              w: block.box?.w ?? 0,
              h: block.box?.h ?? 0,
            },
            style: block.style ?? {},
            url: block.url ?? '',
            title: block.title ?? '',
            note: block.note,
          },
        ]
      : [],
  );
  const files: FileAttachment[] = message.blocks.flatMap((block) =>
    block.kind === 'file'
      ? [
          {
            id: nextId(),
            name: block.name,
            path: block.path,
            size: block.size ?? 0,
          },
        ]
      : [],
  );
  return { images, elements, files };
}

/**
 * `extra.prompt` is what the agent is sent when it differs from the text shown in the chat, and `extra.preface` goes in
 * front of the prompt without being shown (context of a delegated task). `extra.silent` (the message that handed work
 * out) sends `extra.prompt` without any message in the chat: the agent's reply just continues after that message.
 */
/** A message of the user as the chat shows it: pictures, files and page elements first, then the words. */
function userMessage(
  body: string,
  images: Attachment[],
  elements: PageElement[],
  files: FileAttachment[],
): Message {
  return {
    id: nextId(),
    role: 'user',
    time: clock(new Date().toISOString()),
    blocks: [
      ...images.map((image) => ({
        kind: 'image' as const,
        src: image.src,
        name: image.name,
      })),
      ...files.map((file) => ({
        kind: 'file' as const,
        name: file.name,
        path: file.path,
        size: file.size,
      })),
      ...elements.map((item) => ({
        kind: 'element' as const,
        tag: item.tag,
        text: item.text,
        selector: item.selector,
        url: item.url,
        title: item.title,
        note: item.note,
        html: item.html,
        around: item.around,
        box: { ...item.box },
        style: { ...item.style },
        picture: item.picture,
      })),
      ...(body ? [{ kind: 'response' as const, text: body }] : []),
    ],
    at: new Date().toISOString(),
  };
}

function send(
  sessionId: string,
  text: string,
  images: Attachment[],
  elements: PageElement[],
  mentions: Mention[] = [],
  extra: {
    prompt?: string;
    preface?: string;
    files?: FileAttachment[];
    silent?: Message;
  } = {},
) {
  const session = workspace.sessions.find((item) => item.id === sessionId);
  const body = text.trim();
  const files = extra.files ?? [];
  if (!session || replies[sessionId]) return;
  // Waits behind the account switch of its agent, whole and in order (see switchingAgent).
  if (switchingAgent(session.agent)) {
    const waiting = afterSwitch.get(session.agent) ?? [];
    waiting.push(() => send(sessionId, text, images, elements, mentions, extra));
    afterSwitch.set(session.agent, waiting);
    return;
  }
  // The user took over: a wait for a limit to reset is over. (The recovery's own turns are silent.)
  if (!extra.silent) {
    delete resumes[sessionId];
    delete resumeTries[sessionId];
  }
  const messages = messagesOf(sessionId);
  if (
    extra.silent &&
    extra.prompt &&
    inTauri() &&
    (session.agent === 'claude' ||
      session.agent === 'codex' ||
      session.agent === 'antigravity' ||
      session.agent === 'opencode' ||
      session.agent === 'custom')
  ) {
    startTurn(
      session,
      extra.prompt,
      [],
      [],
      (extra.preface ?? ''),
      [],
      extra.silent,
    );
    if (!session.path) localMessages[session.id] = messages;
    persistLocal();
    return;
  }
  // Auto-compress: a Claude context near the model's window is compacted first; the message waits at the head of the
  // queue and goes once the compaction is done.
  if (!extra.prompt && !extra.preface && needsCompress(session, messages)) {
    queued[sessionId] = [{ text, images, elements, mentions, files }, ...(queued[sessionId] ?? [])];
    compressContext(session);
    return;
  }
  messages.push(userMessage(body, images, elements, files));
  // A new request from the user: retries and agents tried are counted again from here.
  delete recoveries[sessionId];
  const preface = (extra.preface ?? '') + mentionContext(mentions);
  const targets = mentions.filter((mention) => mention.kind === 'ai');
  const preset = presetOf(sessionId);
  if (inTauri() && targets.length > 0 && body)
    delegate(
      session,
      messages[messages.length - 1],
      body,
      targets,
      preface,
      images,
      elements,
      files,
    );
  else if (
    inTauri() &&
    preset &&
    preset.steps.length > 0 &&
    body &&
    !extra.prompt &&
    !extra.preface
  )
    void runWorkflow(
      session,
      messages[messages.length - 1],
      preset,
      body,
      preface,
      images,
      elements,
      files,
    );
  else if (
    inTauri() &&
    (session.agent === 'claude' ||
      session.agent === 'codex' ||
      session.agent === 'antigravity' ||
      session.agent === 'opencode' ||
      session.agent === 'copilot' ||
      session.agent === 'custom')
  )
    startTurn(
      session,
      withElements(extra.prompt ?? body, elements),
      images,
      [],
      preface,
      files,
    );
  else if (inTauri()) {
    messages.push({
      id: nextId(),
      role: 'assistant',
      time: clock(new Date().toISOString()),
      blocks: [
        {
          kind: 'response',
          text: `⚠ ${session.agent} chats are not connected yet. Claude and Codex run from here for now.`,
        },
      ],
    });
  }
  // Imported sessions reload from disk; sessions that only exist here keep their messages in memory.
  if (!session.path) localMessages[session.id] = messages;
  persistLocal();
  if (session.draft) {
    session.draft = false;
    const firstLine = body.split('\n')[0] || 'Image';
    session.title =
      firstLine.length > 60 ? `${firstLine.slice(0, 57)}…` : firstLine;
  }
}

const COMPACT_COMMAND = '/compact';

/** The context of `session` is near its model's window: the last Claude call read more than COMPRESS_AT of it. */
function needsCompress(session: SessionItem, messages: Message[]): boolean {
  if (!automation.compress || session.agent !== 'claude' || !session.agentStarted || !session.agentSession) return false;
  const last = [...messages].reverse().find((message) => message.role === 'assistant' && message.agent === 'claude');
  if (!last?.context || last.compressed) return false;
  return last.context >= contextWindow(last.model) * COMPRESS_AT;
}

/** Runs `/compact` in the chat's Claude conversation, shown as one "Compressing context" line. */
function compressContext(session: SessionItem) {
  startTurn(session, COMPACT_COMMAND);
  const reply = liveReplies[session.id];
  if (!reply) return;
  reply.compressed = true;
  reply.blocks = [{ kind: 'loading', label: 'Compressing context…' }];
}

/** A fork keeps the project and branch of its parent and nests under it in the tree. */
// Leaving or reloading the page keeps the open session and the local chats.
if (typeof window !== 'undefined') {
  window.addEventListener('pagehide', () => persistLocal());
}

/** The conversation so far for a new session, but only when the message it is about to get cannot stand without it:
 *  one that names its subject goes as it is (see context-need.ts), so a session is not loaded with history for nothing. */
function handoffFor(sessionId: string, prompt: string): string {
  return needsContext(prompt) ? handoffDigest(messagesOf(sessionId)) : '';
}

/** What an agent is told after an account switch: the conversation so far, without the message it is about to get.
 *  The new session has no history of its own, so this is how the work in progress carries over. */
function handoffDigest(
  list: Message[],
  lead = 'The account was switched, so this is a new session of the same conversation. Continue the work in progress; do not redo what is already done.',
): string {
  // Not the message about to be answered (the last one of the user), and not a reply that only holds an error line.
  const last = list.at(-1);
  const onlyError =
    last?.role === 'assistant' &&
    last.blocks.every((block) => block.kind === 'response' && (!block.text.trim() || block.text.trim().startsWith('⚠')));
  const before = last?.role === 'user' || onlyError ? list.slice(0, -1) : list;
  return forkDigest(before, lead);
}

/** Whether a chat's CLI switches its model in the same conversation (Claude's `set_model`, see warm.rs): a model change
 *  then keeps the conversation. The other agents start a new one (and carry the turns so far, see MODEL_LEAD). */
const liveModel = (agent: string) => agent === 'claude';

/** What an edited message carries over: the chat before it (the edited message and what came after are gone). */
const EDIT_LEAD = 'The user edited an earlier message, so this is a new session of the same conversation, without what came after it. Continue from here; do not repeat work that the turns below do not show.';

/** What a model change carries over: a new CLI conversation cannot resume the old one, so the turns so far go with the next message. */
const MODEL_LEAD = 'The model was changed, so this is a new session of the same conversation. Continue the work in progress; do not redo what is already done.';

/** How many of the last messages a digest keeps whole when "Compress context" is on, and how much of each older one. */
const DIGEST_WHOLE = 6;
const DIGEST_OLD_CHARS = 400;

/** What a fork is told of the conversation it came from: the turns so far, newest kept when it is long. */
function forkDigest(list: Message[], lead = 'This chat is a fork of an earlier conversation.'): string {
  const shown = list.filter((message) => !hiddenPrompt(message));
  // With "Compress context" on, only the last turns go whole: older ones keep their start, so a long chat moved to a new
  // session reads like a summary instead of filling the window with the full history.
  const whole = automation.compress ? Math.max(0, shown.length - DIGEST_WHOLE) : 0;
  const lines = shown
    .map((message, index) => {
      const limit = index < whole ? DIGEST_OLD_CHARS : 2500;
      const text = message.blocks
        .flatMap((block) =>
          block.kind === 'response'
            ? [block.text]
            : block.kind === 'command'
              ? [`(ran: ${block.command.slice(0, 160)})`]
              : [],
        )
        .join('\n')
        .trim();
      return text
        ? `${message.role === 'user' ? 'User' : 'Assistant'}: ${text.length > limit ? `${text.slice(0, limit)}…` : text}`
        : '';
    })
    .filter(Boolean);
  if (!lines.length) return '';
  let digest = lines.join('\n\n');
  if (digest.length > 16000) digest = `…${digest.slice(-16000)}`;
  return `${lead} What was said so far:\n\n${digest}\n\n---\nNow the user says:\n`;
}

/**
 * A fork is a chat of its own: it gets a copy of the messages on screen, kept with the app's own chats, and no tie to the
 * parent's transcript file or CLI conversation. Sharing those made the fork show the parent's replies again each time it
 * was opened, run under the parent's conversation, and disturb the parent's status.
 */
export function forkSession(sourceId: string) {
  const source = workspace.sessions.find((item) => item.id === sourceId);
  if (!source) return;
  const id = nextId();
  const copy = (
    JSON.parse(JSON.stringify(messagesOf(sourceId))) as Message[]
  ).map((message) => ({ ...message, live: false }));
  localMessages[id] = copy;
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  const {
    path,
    source: _origin,
    updatedAt,
    messageCount,
    agentSession,
    agentStarted,
    agentIds,
    conversationModel,
    agentModels,
    ...rest
  } = source;
  workspace.sessions.push({
    ...rest,
    id,
    title: `${source.title} (fork)`,
    status: 'idle',
    parentId: source.id,
    draft: false,
    messageCount: copy.length,
    updatedAt: new Date().toISOString(),
    forkContext: forkDigest(copy) || undefined,
  });
  workspace.activeId = id;
  persistLocal();
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
  done?: boolean;
};

/** How the sidebar lists chats: by folder (Recents, This computer, Projects), by date, or one plain list. */
export type SidebarView = {
  groupBy: 'folder' | 'date' | 'none';
  sortBy: 'updated' | 'name';
  order: 'desc' | 'asc';
};
export const DEFAULT_VIEW: SidebarView = {
  groupBy: 'folder',
  sortBy: 'updated',
  order: 'desc',
};
export type TreeOptions = {
  view: SidebarView;
  /** Heading keys that are folded. */
  collapsed: string[];
  /** Number of 20-row pages loaded after each list's initial rows. */
  showPages: Record<string, number>;
};
/** Rows a list shows before its "Show more". */
const LIST_LIMIT = 5;
const PROJECT_LIMIT = 10;
const LIST_PAGE_SIZE = 20;

/** The project of the open chat while it is still an empty new session: its row shows where the chat will go. */
export function pendingProject(): string | undefined {
  const active = activeSession();
  return active && isEmptyDraft(active) ? active.projectId : undefined;
}

export function buildTree(
  { view, collapsed, showPages }: TreeOptions = {
    view: DEFAULT_VIEW,
    collapsed: [],
    showPages: {},
  },
): ZTreeNode<TreeData>[] {
  // A session with no time yet is new: it goes first.
  const newest = (a: SessionItem, b: SessionItem) => {
    if (!a.updatedAt || !b.updatedAt)
      return Number(!b.updatedAt) - Number(!a.updatedAt);
    return b.updatedAt.localeCompare(a.updatedAt);
  };
  // Two levels, as in Claude: every fork, a fork of a fork too, sits right under the chat it all started from.
  const rootOf = (item: SessionItem): string => {
    let cursor = item;
    for (let depth = 0; cursor.parentId && depth < 50; depth++) {
      const parent = workspace.sessions.find(
        (other) => other.id === cursor.parentId,
      );
      if (!parent) break;
      cursor = parent;
    }
    return cursor.id;
  };
  const forksOf = (root: SessionItem): ZTreeNode<TreeData>[] =>
    workspace.sessions
      .filter(
        (item) =>
          item.parentId && item.id !== root.id && rootOf(item) === root.id,
      )
      .sort(newest)
      .map((item, index, list) => sessionNode(item, index === list.length - 1));

  const sessionNode = (
    session: SessionItem,
    last = false,
  ): ZTreeNode<TreeData> => ({
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
    const sorted = [...list].sort(
      view.sortBy === 'name'
        ? (x, y) => x.title.localeCompare(y.title)
        : newest,
    );
    return view.order === 'asc' ? sorted.reverse() : sorted;
  };
  // The first rows of a long list, then one 20-row page per "Show more" click.
  const limited = <T>(
    key: string,
    list: T[],
    limit: number,
    row: (item: T) => ZTreeNode<TreeData>,
  ) => {
    const page = showPages[key] ?? 0;
    const visibleCount = Math.min(list.length, limit + page * LIST_PAGE_SIZE);
    const done = list.length <= visibleCount;
    if (done && page === 0) return list.map(row);
    const open = page > 0;
    const more: ZTreeNode<TreeData> = {
      id: `more:${key}`,
      label: done ? 'Show less' : `Show ${list.length - visibleCount} more`,
      isLeaf: true,
      children: [],
      data: { kind: 'more', group: key, open, done },
    };
    return [...list.slice(0, visibleCount).map(row), more];
  };

  const projectNode = (project: Project): ZTreeNode<TreeData> => ({
    id: `project:${project.id}`,
    label: project.name,
    icon: 'lucideFolder',
    data: { kind: 'project', path: project.path },
    children: limited(
      `project:${project.id}`,
      ordered(
        workspace.sessions.filter(
          (item) =>
            item.projectId === project.id && !item.parentId && shown(item),
        ),
      ),
      LIST_LIMIT,
      (item) => sessionNode(item),
    ),
  });
  // A chat listed right under a heading: one line, no branch lines. `key` keeps ids unique when a chat is listed twice.
  const flatNode = (
    session: SessionItem,
    key = 'session',
  ): ZTreeNode<TreeData> => ({
    id: `${key}:${session.id}`,
    label: session.title,
    isLeaf: true,
    children: [],
    data: { kind: 'session', session, flat: true },
  });
  // A heading and, unless it is folded, its rows. An empty heading still shows (its + button), without a fold arrow.
  const section = (
    key: string,
    label: string,
    rows: ZTreeNode<TreeData>[],
    always = false,
  ): ZTreeNode<TreeData>[] => {
    if (!rows.length && !always) return [];
    const head: ZTreeNode<TreeData> = {
      id: `section:${key}`,
      label,
      isLeaf: true,
      children: [],
      data: { kind: 'section', count: rows.length },
    };
    return [head, ...(collapsed.includes(key) ? [] : rows)];
  };

  // Pinned: chats and project folders kept at the top, in the order they were pinned. A pinned folder moves there
  // from Projects; a pinned chat leaves Recents and the date lists but stays in its project.
  const pinnedChats = pinned.sessions
    .map((id) => workspace.sessions.find((item) => item.id === id))
    .filter((item): item is SessionItem => Boolean(item && shown(item)));
  const visibleProjects = workspace.projects.filter(
    (project) =>
      project.id !== NO_PROJECT &&
      !hiddenProjects.some((path) => samePath(path, project.path)),
  );
  const pinnedProjects = pinned.projects
    .map((path) =>
      visibleProjects.find((project) => samePath(project.path, path)),
    )
    .filter((item): item is Project => Boolean(item));
  const pins = section('pinned', 'Pinned', [
    ...pinnedProjects.map(projectNode),
    ...pinnedChats.map((item) => flatNode(item, 'pin')),
  ]);

  const chats = workspace.sessions.filter(
    (item) => shown(item) && (view.groupBy === 'folder' || !item.parentId),
  );
  const unpinned = chats.filter((item) => !isPinnedSession(item.id));
  if (view.groupBy === 'date') {
    // Today, Yesterday, Previous 7 days, Older: like the chat lists of Claude and ChatGPT.
    const day = 86_400_000;
    const midnight = new Date().setHours(0, 0, 0, 0);
    const bucket = (item: SessionItem) => {
      const time = item.updatedAt ? Date.parse(item.updatedAt) : Date.now();
      return time >= midnight
        ? 'today'
        : time >= midnight - day
          ? 'yesterday'
          : time >= midnight - 7 * day
            ? 'week'
            : 'older';
    };
    const labels = {
      today: 'Today',
      yesterday: 'Yesterday',
      week: 'Previous 7 days',
      older: 'Older',
    } as const;
    return [
      ...pins,
      ...(Object.keys(labels) as (keyof typeof labels)[]).flatMap((key) =>
        section(
          key,
          labels[key],
          limited(
            key,
            ordered(unpinned.filter((item) => bucket(item) === key)),
            LIST_LIMIT,
            (item) => flatNode(item, 'list'),
          ),
        ),
      ),
    ];
  }
  if (view.groupBy === 'none')
    return [
      ...pins,
      ...section(
        'chats',
        'Chats',
        limited('chats', ordered(unpinned), LIST_LIMIT * 3, (item) =>
          flatNode(item, 'list'),
        ),
        true,
      ),
    ];

  // By folder: Recents (newest chats of every project), This computer (chats outside any project), Projects.
  const latest = unpinned.filter((item) => !item.parentId).sort(newest);
  const computer = chats.filter(
    (item) => item.projectId === NO_PROJECT && !item.parentId,
  );
  const projects = visibleProjects.filter(
    (project) => !pinnedProjects.includes(project),
  );
  // Projects come most recently used first; sorting by name sorts them too.
  if (view.sortBy === 'name')
    projects.sort((x, y) => x.name.localeCompare(y.name));
  if (view.order === 'asc') projects.reverse();
  return [
    ...pins,
    ...section(
      'recents',
      'Recents',
      limited('recents', latest, LIST_LIMIT, (item) =>
        flatNode(item, 'recent'),
      ),
    ),
    ...section(
      'computer',
      'This computer',
      limited('computer', ordered(computer), LIST_LIMIT, (item) =>
        flatNode(item),
      ),
      true,
    ),
    ...section(
      'projects',
      'Projects',
      limited('projects', projects, PROJECT_LIMIT, projectNode),
      true,
    ),
  ];
}

// ---------- real history (Claude Code and Codex, read-only) ----------
export const chat = $state({
  messages: mockMessages as Message[],
  loading: false,
  error: '',
  older: 0,
  loadingOlder: false,
});
export const history = $state({ imported: false, loading: inTauri() });

const CACHE_KEY = 'agentdeck.history.index';

/** Last folder of a path, for either separator: `C:\work\demo` and `/home/me/demo` both give `demo`. */
const folderName = (cwd: string) =>
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
      const cached = JSON.parse(
        localStorage.getItem(CACHE_KEY) ?? '[]',
      ) as HistorySession[];
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
    projectId: projectIds.get(item.cwd)!,
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

  workspace.projects = projects;
  workspace.sessions = [
    ...merged,
    ...local.filter((item) =>
      projects.some((project) => project.id === item.projectId),
    ),
  ];
  workspace.activeId = keepActive ?? sessions[0].id;
}

let transcriptToken = 0;

// ---------- transcripts: memory + IndexedDB cache, so opening a chat is instant ----------
// A transcript is valid while the session's index entry (last update + message count) is unchanged.
// `start` is where the first loaded message sits in the chat's file: above 0, older messages are still on disk.
type Transcript = { stamp: string; messages: Message[]; start: number };
const transcripts = new Map<string, Transcript>();
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
const otherTurns: Record<string, Message[]> = {};
void getPref<Record<string, Message[]>>('chat.otherTurns').then((saved) => {
  if (saved) Object.assign(otherTurns, saved);
});
function keepOtherTurn(session: SessionItem, reply: Message) {
  const list = messagesOf(session.id);
  const index = list.indexOf(reply);
  const asked =
    index > 0 && list[index - 1].role === 'user' ? [list[index - 1]] : [];
  const kept = (otherTurns[session.id] ??= []);
  for (const message of [...asked, reply]) {
    if (!kept.some((item) => item.id === message.id))
      kept.push($state.snapshot(message) as Message);
  }
  const saved = Object.fromEntries(
    Object.entries(otherTurns).map(([id, list]) => [
      id,
      stripCommandImageData($state.snapshot(list) as Message[]),
    ]),
  );
  setPref('chat.otherTurns', saved);
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
      for (const old of next.slice(MAX_SAVED))
        setPref(`transcript.${old}`, null);
      setPref('transcripts.keys', next.slice(0, MAX_SAVED));
      const tail = {
        stamp: '',
        messages: stripCommandImageData(messages.slice(-SAVED_TAIL)),
        start: start + Math.max(0, messages.length - SAVED_TAIL),
      };
      setPref(`transcript.${session.id}`, JSON.parse(JSON.stringify(tail)));
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
function patchCache(change: (list: HistorySession[]) => HistorySession[]) {
  try {
    const list = JSON.parse(
      localStorage.getItem(CACHE_KEY) ?? '[]',
    ) as HistorySession[];
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
