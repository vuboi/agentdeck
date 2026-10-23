// Workflow presets: a saved list of steps (agent + model + role prompt) that a chat's orchestrator runs on each
// message. Built-in presets are fixed; the ones the user makes are kept locally (prefs). Which preset a chat uses is
// kept apart from the chat (`assigned`), so chats imported from the CLIs' history can have one too.
import { getPref, setPref } from './prefs';
import type { Agent } from './sessions.svelte';

export type StepCondition = { mode: 'always' | 'contains' | 'missing'; text: string };
export type WorkflowStep = {
  id: string;
  name: string;
  agent: Agent;
  /** Empty: the first model listed for the agent. */
  model: string;
  /** The role of the agent in this step. */
  prompt: string;
  /** Runs at the same time as the step before it instead of after it. */
  parallel: boolean;
  /** Runs only if the answer of the step(s) before it passes this check. */
  condition: StepCondition;
  /** How hard the step's model thinks; unset: the chat's own level. A light step on a low level saves tokens. */
  effort?: string;
  /** Where the canvas draws the card (auto layout while unset). */
  x?: number;
  y?: number;
  /** What feeds this step: 'start' (the task) and/or ids of other steps. Unset: the step(s) of the group before it. */
  inputs?: string[];
};
export type WorkflowPreset = {
  id: string;
  name: string;
  description: string;
  /** What the orchestrating agent (the chat's own agent) is told when it writes the final answer. */
  orchestrator: string;
  steps: WorkflowStep[];
  builtin?: boolean;
};

const step = (name: string, agent: Agent, prompt: string, extra: Partial<WorkflowStep> = {}): WorkflowStep => ({
  id: `step-${name.toLowerCase().replace(/\W+/g, '-')}`,
  name,
  agent,
  model: '',
  prompt,
  parallel: false,
  condition: { mode: 'always', text: '' },
  ...extra,
});

export const BUILT_IN_PRESETS: WorkflowPreset[] = [
  {
    id: 'builtin-plan-implement-review',
    name: 'Plan → Implement → Review',
    description: 'One agent plans, another builds it, a third reviews the change.',
    orchestrator: 'Summarise what was planned, what was built and what the review found. Say plainly what is left to do.',
    builtin: true,
    steps: [
      step('Plan', 'claude', 'You are the planner. Read the code that matters and write a short, ordered plan with the files to change. Do not edit anything.'),
      step('Implement', 'codex', 'You are the implementer. Carry out the plan you were given, with the smallest change that does the job. Say which files you changed.'),
      step('Review', 'claude', 'You are the reviewer. Check the change for bugs, missed cases and style. List concrete problems with file and line, or say it is good.'),
    ],
  },
  {
    id: 'builtin-parallel-research',
    name: 'Parallel research',
    description: 'Several agents look into the question side by side, then one merges their findings.',
    orchestrator: 'Compare the findings, point out where they agree or differ, and give one clear conclusion.',
    builtin: true,
    steps: [
      step('Research A', 'claude', 'Investigate the question in the code and docs. Report findings with file paths and evidence.'),
      step('Research B', 'codex', 'Investigate the question from another angle (alternatives, risks, prior art). Report findings with evidence.', { parallel: true }),
      step('Research C', 'antigravity', 'Look for pitfalls and edge cases the others may miss. Report them briefly.', { parallel: true }),
    ],
  },
  {
    id: 'builtin-fix-bug-tests',
    name: 'Fix bug with tests',
    description: 'Reproduce with a failing test, fix it, then confirm and review.',
    orchestrator: 'Report the root cause, the fix, and the test result. Say if anything could not be verified.',
    builtin: true,
    steps: [
      step('Reproduce', 'codex', 'Write a test that fails because of the bug described. Run it and show the failure. Do not fix the bug yet.'),
      step('Fix', 'claude', 'Fix the root cause so the failing test passes without breaking others. Run the tests and report the result.'),
      step('Review', 'codex', 'Review the fix for side effects. Only if you find a real problem, describe it precisely.'),
    ],
  },
];

/** The steps in the order they run: a step with `parallel` joins the group of the one before it. */
export const stepGroups = (steps: WorkflowStep[]) =>
  steps.reduce<WorkflowStep[][]>((groups, step) => {
    if (step.parallel && groups.length) groups[groups.length - 1].push(step);
    else groups.push([step]);
    return groups;
  }, []);

/** The other way round: groups back into the flat list (the first step of a group runs after, the rest beside it). */
export const flattenGroups = (groups: WorkflowStep[][]) =>
  groups.filter((group) => group.length).flatMap((group) => group.map((step, at) => ({ ...step, parallel: at > 0 })));

/** What feeds every step (ids of steps, or 'start'), from `inputs` or, when unset, from the order of the list. */
export function inputsOf(steps: WorkflowStep[]): Record<string, string[]> {
  const ids = new Set(steps.map((item) => item.id));
  const groups = stepGroups(steps);
  const out: Record<string, string[]> = {};
  groups.forEach((group, at) => {
    for (const item of group) {
      const own = item.inputs?.filter((id) => id === 'start' || (ids.has(id) && id !== item.id));
      out[item.id] = item.inputs ? (own?.length ? own : ['start']) : at === 0 ? ['start'] : groups[at - 1].map((before) => before.id);
    }
  });
  return out;
}

/** The list with its links written out, in the order they can run (a step after everything that feeds it), with the
 *  steps of one stage marked `parallel`: the form the runner reads. */
export function normalizeGraph(steps: WorkflowStep[]): WorkflowStep[] {
  const inputs = inputsOf(steps);
  const level = new Map<string, number>();
  const depth = (id: string, seen: Set<string>): number => {
    if (id === 'start') return -1;
    const known = level.get(id);
    if (known !== undefined) return known;
    if (seen.has(id)) return 0;
    seen.add(id);
    const value = 1 + Math.max(-1, ...(inputs[id] ?? []).map((from) => depth(from, seen)));
    level.set(id, value);
    return value;
  };
  for (const item of steps) depth(item.id, new Set());
  const groups: WorkflowStep[][] = [];
  for (const item of steps) (groups[level.get(item.id) ?? 0] ??= []).push({ ...item, inputs: inputs[item.id] });
  return flattenGroups(groups.filter(Boolean));
}

/** Whether a link from `from` to `to` would loop back on itself. */
export function wouldCycle(steps: WorkflowStep[], from: string, to: string): boolean {
  if (from === to) return true;
  const inputs = inputsOf(steps);
  const stack = [from];
  const seen = new Set<string>();
  while (stack.length) {
    const id = stack.pop()!;
    if (id === to) return true;
    if (seen.has(id)) continue;
    seen.add(id);
    stack.push(...(inputs[id] ?? []));
  }
  return false;
}

export function addLink(steps: WorkflowStep[], from: string, to: string): WorkflowStep[] {
  const inputs = inputsOf(steps);
  if (inputs[to]?.includes(from) || wouldCycle(steps, from, to)) return steps;
  const only = inputs[to]?.length === 1 && inputs[to][0] === 'start';
  return normalizeGraph(steps.map((item) => ({ ...item, inputs: item.id === to ? [...(only && from !== 'start' ? [] : inputs[to]), from] : inputs[item.id] })));
}

export function removeLink(steps: WorkflowStep[], from: string, to: string): WorkflowStep[] {
  const inputs = inputsOf(steps);
  return normalizeGraph(steps.map((item) => ({ ...item, inputs: item.id === to ? inputs[to].filter((id) => id !== from) : inputs[item.id] })));
}

/** Takes a step out; what it fed is fed by what fed it. */
export function removeStep(steps: WorkflowStep[], id: string): WorkflowStep[] {
  const inputs = inputsOf(steps);
  const gone = inputs[id] ?? [];
  return normalizeGraph(steps.filter((item) => item.id !== id).map((item) => ({ ...item, inputs: [...new Set(inputs[item.id].flatMap((from) => (from === id ? gone : [from])))] })));
}

export const workflows = $state<{ presets: WorkflowPreset[]; assigned: Record<string, string> }>({ presets: [], assigned: {} });

void getPref<WorkflowPreset[]>('workflows.presets').then((saved) => {
  if (Array.isArray(saved)) workflows.presets.push(...saved.filter((item) => !item.builtin));
});
void getPref<Record<string, string>>('workflows.assigned').then((saved) => {
  if (saved) Object.assign(workflows.assigned, saved);
});

const plain = <T>(value: T): T => JSON.parse(JSON.stringify(value));
const saveAll = () => setPref('workflows.presets', plain(workflows.presets));

export const allPresets = () => [...BUILT_IN_PRESETS, ...workflows.presets];
export const presetById = (id: string | undefined) => (id ? allPresets().find((item) => item.id === id) : undefined);
export const presetOf = (sessionId: string) => presetById(workflows.assigned[sessionId]);

/** A blank preset to start the editor from. */
export const newPreset = (): WorkflowPreset => ({
  id: crypto.randomUUID(),
  name: 'New workflow',
  description: '',
  orchestrator: 'Combine the results into one answer for the user.',
  steps: [{ ...step('Step 1', 'claude', ''), id: crypto.randomUUID() }],
});

export const newStep = (n: number): WorkflowStep => ({ ...step(`Step ${n}`, 'claude', ''), id: crypto.randomUUID() });

/** Saves a preset (a copy of the editor's draft); a built-in one is never overwritten. */
export function savePreset(preset: WorkflowPreset) {
  const copy: WorkflowPreset = { ...plain(preset), builtin: undefined };
  const at = workflows.presets.findIndex((item) => item.id === copy.id);
  if (at === -1) workflows.presets.push(copy);
  else workflows.presets[at] = copy;
  saveAll();
}

/** A copy of any preset the user can then edit. */
export function duplicatePreset(id: string): WorkflowPreset | undefined {
  const source = presetById(id);
  if (!source) return undefined;
  const copy: WorkflowPreset = { ...plain(source), id: crypto.randomUUID(), name: `${source.name} (copy)`, builtin: undefined };
  copy.steps = copy.steps.map((item) => ({ ...item, id: crypto.randomUUID() }));
  workflows.presets.push(copy);
  saveAll();
  return copy;
}

export function deletePreset(id: string) {
  const at = workflows.presets.findIndex((item) => item.id === id);
  if (at === -1) return;
  workflows.presets.splice(at, 1);
  for (const [session, preset] of Object.entries(workflows.assigned)) if (preset === id) delete workflows.assigned[session];
  saveAll();
  setPref('workflows.assigned', plain(workflows.assigned));
}

/** Gives a chat a preset, or takes it away (`null`). */
export function assignPreset(sessionId: string, presetId: string | null) {
  if (presetId) workflows.assigned[sessionId] = presetId;
  else delete workflows.assigned[sessionId];
  setPref('workflows.assigned', plain(workflows.assigned));
}

/** How a flow runs: one step after the other, all at once, or both (stages, some with several steps). */
export function flowKind(steps: WorkflowStep[]): 'sequential' | 'parallel' | 'mixed' {
  const groups = stepGroups(normalizeGraph(steps));
  if (groups.length <= 1 && steps.length > 1) return 'parallel';
  return groups.some((group) => group.length > 1) ? 'mixed' : 'sequential';
}

// ---- the full-screen flow editor ----
// One editor for the whole app: the Workflow tab and the composer's flow menu open it. It edits a copy of the flow
// (`draft`), so Cancel loses nothing; a built-in flow is copied first (they cannot be changed).

export const flowEditor = $state<{ draft: WorkflowPreset | null; isNew: boolean }>({ draft: null, isNew: false });


export function editFlow(preset: WorkflowPreset) {
  const source = preset.builtin ? { ...plain(preset), id: crypto.randomUUID(), name: `${preset.name} (copy)`, builtin: undefined } : plain(preset);
  if (preset.builtin) source.steps = source.steps.map((item) => ({ ...item, id: crypto.randomUUID(), inputs: undefined }));
  flowEditor.draft = source;
  flowEditor.isNew = Boolean(preset.builtin) || !workflows.presets.some((item) => item.id === preset.id);
}

export const createFlow = () => editFlow(newPreset());

export function closeFlow() {
  flowEditor.draft = null;
  flowEditor.isNew = false;
}

/** Why the draft cannot be saved yet, or '' when it can. */
export function flowProblem(preset: WorkflowPreset | null): string {
  if (!preset) return '';
  if (!preset.name.trim()) return 'Give the flow a name.';
  if (preset.steps.length === 0) return 'Add at least one step.';
  if (preset.steps.some((item) => !item.prompt.trim())) return 'Every step needs a role prompt.';
  return '';
}
