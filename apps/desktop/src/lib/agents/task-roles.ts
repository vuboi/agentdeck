// Which agents do which kind of work. Pure (no stores, no Tauri) so plain node can test it.
//
// A route is `agent|model` in the form `delegate_task` takes: `claude|claude-haiku-5-5`, `codex|gpt-5.6-terra`, or a
// connector's id with its model. A role holds an ordered list of routes: the first is the highest priority. The read role
// tries them in that order for pictures (the pre-handle, run.svelte.ts); the other roles offer them to the main agent to
// choose from, and the hand-off goes through `delegate_task`, so it shows in the Workflow tab like any other delegation.

export const TASK_ROLES = [
  { id: 'read', label: 'Read', icon: 'lucideImage', hint: 'Pictures read before the agent sees them; files and documents' },
  { id: 'research', label: 'Research', icon: 'lucideGlobe', hint: 'Web search, papers and sources' },
  { id: 'plan', label: 'Plan', icon: 'lucideListChecks', hint: 'Break a task into steps before doing it' },
  { id: 'review', label: 'Review', icon: 'lucideGitCompare', hint: 'Code review and security review' },
  { id: 'write', label: 'Write', icon: 'lucideScrollText', hint: 'Docs, tests and emails' },
  { id: 'text', label: 'Text', icon: 'lucideType', hint: 'Summarize and translate' },
  { id: 'logic', label: 'Logic and math', icon: 'lucideBrain', hint: 'Step-by-step reasoning and calculations' },
] as const;

export type TaskRoleId = (typeof TASK_ROLES)[number]['id'];

/** The roles before they were grouped: a saved route or a delegate_task `role` under an old id is read as its new role. */
const LEGACY_ROLE: Record<string, TaskRoleId> = {
  image: 'read',
  files: 'read',
  documents: 'read',
  web: 'research',
  security: 'review',
  tests: 'write',
  docs: 'write',
  email: 'write',
  summary: 'text',
  translate: 'text',
};

/** The role an id names now (a current id, or an id from before the grouping), or undefined for anything else. */
export function canonicalRole(value: unknown): TaskRoleId | undefined {
  if (typeof value !== 'string') return undefined;
  if (TASK_ROLES.some((role) => role.id === value)) return value as TaskRoleId;
  return LEGACY_ROLE[value];
}

/** The ordered routes of each role (a row with no model yet is `''`), the master flag (off: no role is used at all), and the
 *  roles switched off one by one (their picks are kept, they are just not used). */
export type RoleState = { routes: Partial<Record<string, string[]>>; on: boolean; off: string[] };

/** Saved routes under the old role ids, moved to the new ones. A new role's routes are the old ones it took in, in order. */
export function migrateRoutes(saved: Partial<Record<string, string[]>>): Partial<Record<string, string[]>> {
  const next: Partial<Record<string, string[]>> = {};
  for (const [id, list] of Object.entries(saved)) {
    const role = canonicalRole(id);
    if (!role || !list) continue;
    next[role] = [...new Set([...(next[role] ?? []), ...list])];
  }
  return next;
}

/** Switched-off roles moved to the new ids: a new role is off when every old role that has picks is off. */
export function migrateOff(off: string[], saved: Partial<Record<string, string[]>>): string[] {
  return TASK_ROLES.filter((role) => {
    const picked = Object.keys(saved).filter((id) => canonicalRole(id) === role.id && (saved[id]?.length ?? 0) > 0);
    return picked.length > 0 && picked.every((id) => off.includes(id));
  }).map((role) => role.id);
}

/** The routes a role offers, in priority order: none while the master flag is off or the role is switched off, and rows
 *  without a model are left out. */
export function routesOf(state: RoleState, id: TaskRoleId): string[] {
  if (!state.on || state.off.includes(id)) return [];
  return (state.routes[id] ?? []).filter((route) => route.includes('|') && !route.startsWith('source:'));
}

/** What the chat shows while a role's work runs, and once it is done (for the delegate_task card and the live status). */
export const ROLE_STATUS: Record<TaskRoleId, { running: string; done: string }> = {
  read: { running: 'Reading', done: 'Read' },
  research: { running: 'Researching', done: 'Researched' },
  plan: { running: 'Planning', done: 'Planned' },
  review: { running: 'Reviewing', done: 'Reviewed' },
  write: { running: 'Writing', done: 'Wrote' },
  text: { running: 'Working on the text', done: 'Worked on the text' },
  logic: { running: 'Working out the logic', done: 'Worked out the logic' },
};

/** The role a delegate_task call was made for (its `role` argument), from the command the chat keeps (`tool  {json}`). */
export function roleOfCall(command: string): TaskRoleId | undefined {
  const brace = command.indexOf('{');
  if (brace < 0) return undefined;
  try {
    return canonicalRole((JSON.parse(command.slice(brace)) as { role?: unknown }).role);
  } catch {
    return undefined;
  }
}

/** The short note the main agent gets for the roles that offer routes (none when there is none). */
export function taskRoleHint(state: RoleState, labelOf: (route: string) => string): string {
  const lines = TASK_ROLES.flatMap((role) => {
    const list = routesOf(state, role.id);
    if (!list.length) return [];
    const options = list.map((route) => labelOf(route));
    return [`- ${role.label.toLowerCase()}: delegate_task with role "${role.id}" (AgentDeck sends it to the first available of: ${options.join('; then ')})`];
  });
  return lines.length ? `Task routing set by the user for this work, in order of preference:\n${lines.join('\n')}` : '';
}
