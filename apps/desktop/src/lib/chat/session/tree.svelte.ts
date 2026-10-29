import type { ZTreeNode } from '@shival99/z-ui-svelte';
import { samePath } from '../../files/paths';
import { agentDisplayName, customProviderOf } from '../../agents/provider-registry';
import { NO_PROJECT, activeSession, hiddenProjects, isEmptyDraft, isPinnedSession, pinned, workspace, type Project, type SessionItem } from './state.svelte';

// ---------- tree: Project > Session > Fork ----------
// A fork is a branch of the conversation (not a git branch): it nests under the session it came from.
export type TreeData = {
  /**
   * `section` is a heading row (Recents, This computer, Projects, or a date); a `session` with `flat` sits right under
   * one; `more` is the "Show more / Show less" row at the end of a long list.
   */
  kind: 'project' | 'session' | 'section' | 'more';
  /** Provider identity for a plain-chat heading and its brand logo. */
  provider?: { agent: SessionItem['agent']; model: string };
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
  // Found once per build (by id, memoised), not by scanning every chat for every row: a rebuild stays linear.
  const byId = new Map(workspace.sessions.map((item) => [item.id, item]));
  const roots = new Map<string, string>();
  const rootOf = (item: SessionItem): string => {
    const known = roots.get(item.id);
    if (known) return known;
    let cursor = item;
    for (let depth = 0; cursor.parentId && depth < 50; depth++) {
      const parent = byId.get(cursor.parentId);
      if (!parent) break;
      cursor = parent;
    }
    roots.set(item.id, cursor.id);
    return cursor.id;
  };
  const forksByRoot = new Map<string, SessionItem[]>();
  for (const item of workspace.sessions) {
    if (!item.parentId) continue;
    const root = rootOf(item);
    if (root === item.id) continue;
    const list = forksByRoot.get(root);
    if (list) list.push(item);
    else forksByRoot.set(root, [item]);
  }
  const forksOf = (root: SessionItem): ZTreeNode<TreeData>[] =>
    (forksByRoot.get(root.id) ?? [])
      .slice()
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
  // Use provider ids as keys: two custom providers may share a display name.
  const providers = new Map<string, SessionItem[]>();
  for (const item of ordered(computer)) {
    const key = item.agent === 'custom'
      ? `custom:${customProviderOf(item.model)?.id ?? 'unknown'}`
      : item.agent;
    const list = providers.get(key);
    if (list) list.push(item);
    if (!list) providers.set(key, [item]);
  }
  const providerRows = [...providers].flatMap(([id, items]) => {
    const key = `provider:${id}`;
    const first = items[0];
    const rows = section(key, agentDisplayName(first.agent, first.model),
      limited(key, items, LIST_LIMIT, (item) => sessionNode(item)));
    if (rows[0]?.data) rows[0].data.provider = { agent: first.agent, model: first.model };
    return rows;
  });
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
      providerRows,
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
