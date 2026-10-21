import type { ZTreeNode } from '@shival99/z-ui-svelte';

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
  model: string;
  /** Git branch the session works on, if any. A conversation branch is `parentId`, not this. */
  gitBranch?: string;
  status: Status;
  /** Set on a fork: the session it was forked from. */
  parentId?: string;
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

export const workspace = $state({
  activeId: 's1',
  projects: [
    { id: 'p1', name: 'agentdeck', path: 'E:\\WORK\\PROJECTS\\agentdeck' },
    { id: 'p2', name: 'z-ui', path: 'E:\\WORK\\PROJECTS\\z-ui' },
    { id: 'p3', name: 'msg-new-template', path: 'E:\\WORK\\PROJECTS\\msg-new-template' },
  ] as Project[],
  sessions: [
    { id: 's1', projectId: 'p1', title: 'Build the settings panel', agent: 'claude', model: 'sonnet-5-5', gitBranch: 'main', status: 'working' },
    { id: 's2', projectId: 'p1', title: 'Try it with Codex', agent: 'codex', model: 'gpt-6-luna', gitBranch: 'main', status: 'waiting', parentId: 's1' },
    { id: 's3', projectId: 'p1', title: 'Codex, second attempt', agent: 'codex', model: 'gpt-5-6-luna', gitBranch: 'main', status: 'idle', parentId: 's2' },
    { id: 's4', projectId: 'p1', title: 'Virtual chat list', agent: 'claude', model: 'opus-5-5', gitBranch: 'feat/chat-virtual', status: 'idle' },
    { id: 's5', projectId: 'p2', title: 'Svelte 5 z-tabs styling', agent: 'claude', model: 'sonnet-5-5', gitBranch: 'main', status: 'idle' },
    { id: 's6', projectId: 'p2', title: 'Review the carousel API', agent: 'gemini', model: 'gemini-3-pro', gitBranch: 'feat/carousel', status: 'idle' },
    { id: 's7', projectId: 'p3', title: 'Zoom check on mobile table', agent: 'claude', model: 'haiku-5-5', gitBranch: 'fix/mobile-zoom', status: 'waiting' },
  ] as SessionItem[],
});

export const activeSession = () => workspace.sessions.find((item) => item.id === workspace.activeId);
export const projectOf = (session: SessionItem) => workspace.projects.find((item) => item.id === session.projectId);

export function newSession(projectId = activeSession()?.projectId ?? workspace.projects[0].id) {
  const base = workspace.sessions.find((item) => item.projectId === projectId);
  const id = `s${workspace.sessions.length + 1}`;
  workspace.sessions.push({
    id,
    projectId,
    title: 'New session',
    agent: 'claude',
    model: 'sonnet-5-5',
    gitBranch: base?.gitBranch,
    status: 'idle',
  });
  workspace.activeId = id;
}

/** A fork keeps the project and branch of its parent and nests under it in the tree. */
export function forkSession(sourceId: string) {
  const source = workspace.sessions.find((item) => item.id === sourceId);
  if (!source) return;
  const id = `s${workspace.sessions.length + 1}`;
  workspace.sessions.push({ ...source, id, title: `${source.title} (fork)`, status: 'idle', parentId: source.id });
  workspace.activeId = id;
}

// ---------- tree: Project > Session > Fork ----------
// A fork is a branch of the conversation (not a git branch): it nests under the session it came from.
export type TreeData = { kind: 'project' | 'session'; session?: SessionItem };

export function buildTree(): ZTreeNode<TreeData>[] {
  const forksOf = (parent: SessionItem): ZTreeNode<TreeData>[] =>
    workspace.sessions.filter((item) => item.parentId === parent.id).map(sessionNode);

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
    data: { kind: 'project' },
    children: workspace.sessions.filter((item) => item.projectId === project.id && !item.parentId).map(sessionNode),
  }));
}
