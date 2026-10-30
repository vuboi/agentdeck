// Saved flow presets describe a lead model and a roster of preferred sub-agents. They never execute a fixed node graph.
import { getPref, setPref } from '../app/platform/prefs';
import type { Agent } from '../chat/sessions.svelte';

export type WorkflowStrategy = 'adaptive' | 'parallel' | 'sequential';
export type WorkflowMember = {
  id: string;
  name: string;
  agent: Agent;
  /** Empty means the lead may choose an available model for this provider. */
  model: string;
  /** The kind of work this sub-agent can take on. */
  prompt: string;
  effort?: string;
};
export type CoordinationPolicy = { maxParallel?: number };
export type WorkflowPreset = {
  id: string;
  name: string;
  description: string;
  /** Instructions for the lead model: it performs the analysis and owns the final integration. */
  leadPrompt: string;
  /** Instructions for the lead model after it has gathered sub-agent reports. */
  orchestrator: string;
  agents: WorkflowMember[];
  strategy: WorkflowStrategy;
  coordination?: CoordinationPolicy;
  builtin?: boolean;
};

const member = (name: string, agent: Agent, prompt: string, model = ''): WorkflowMember => ({ id: crypto.randomUUID(), name, agent, model, prompt });
const defaultLead = 'Analyze the request, choose a useful division of work, delegate to the listed sub-agents, and integrate their reports.';

export const BUILT_IN_PRESETS: WorkflowPreset[] = [
  {
    id: 'builtin-plan-implement-review', name: 'Plan → Implement → Review',
    description: 'The lead plans; implementation and review go to suitable sub-agents.',
    leadPrompt: 'Read the relevant project context and decide the scope, files, and verification approach yourself. Give the implementer a clear, bounded task and ask the reviewer to inspect the resulting diff.',
    orchestrator: 'Summarise the plan, implementation, review findings, and any remaining work.', strategy: 'sequential', builtin: true,
    agents: [
      member('Implement', 'custom', 'Implement the lead’s plan. Keep edits within the assigned scope and report changed files plus verification.'),
      member('Review', 'claude', 'Review the implementer’s diff for concrete bugs, missed cases, and regressions. Do not edit files.'),
    ],
  },
  {
    id: 'builtin-parallel-research', name: 'Parallel research',
    description: 'Several sub-agents investigate independently; the lead compares and combines their findings.',
    leadPrompt: defaultLead, orchestrator: 'Compare findings, call out agreement and disagreement, and give one evidence-based conclusion.',
    strategy: 'parallel', builtin: true,
    agents: [
      member('Research A', 'claude', 'Investigate the question in relevant code and documentation. Report findings with evidence.'),
      member('Research B', 'codex', 'Investigate from another angle: alternatives, risks, and prior art. Report evidence.'),
      member('Research C', 'antigravity', 'Look for edge cases and pitfalls the others may miss. Report them briefly.'),
    ],
  },
  {
    id: 'builtin-team-frontend-backend', name: 'Product team (BA → Frontend + Backend)',
    description: 'The lead defines the contract; frontend and backend sub-agents work in parallel and the lead integrates.',
    leadPrompt: 'Act as the BA and tech lead. Define a shared contract and divide the feature into frontend and backend tasks. Ask both agents to surface contract questions in their reports; reconcile them and issue follow-up tasks when needed.',
    orchestrator: 'Say what each member built, what they agreed, and what remains or is broken.', strategy: 'parallel', builtin: true,
    agents: [
      member('Backend', 'codex', 'Build the backend against the lead’s contract: models, endpoints, validation, and tests. Report contract changes and integration notes.'),
      member('Frontend', 'claude', 'Build the frontend against the lead’s contract. Start with the contract, report gaps, and integrate backend changes when available.'),
    ],
  },
  {
    id: 'builtin-team-full', name: 'Full team (Frontend + Backend + QA + DevOps)',
    description: 'The lead coordinates frontend, backend, QA, and DevOps sub-agents, then integrates their reports.',
    leadPrompt: 'Act as BA and tech lead. Establish a shared contract and split work across frontend, backend, QA, and DevOps. Have agents report dependencies or questions; reconcile disagreements and issue follow-up tasks where needed.',
    orchestrator: 'Summarise what each member did, whether QA passed, how to run the work, and what remains.', strategy: 'parallel', builtin: true,
    agents: [
      member('Backend', 'codex', 'Build the backend to the agreed contract with tests. Report working endpoints and contract changes.'),
      member('Frontend', 'claude', 'Build screens against the agreed contract, then connect them to working APIs. Report gaps and integration needs.'),
      member('QA', 'claude', 'Create and run focused tests. Report reproducible bugs to the lead with expected and actual behavior; do not change product code.'),
      member('DevOps', 'codex', 'Set up scripts, safe environment examples, CI, and run instructions. Do not include real secrets.'),
    ],
  },
  {
    id: 'builtin-fix-bug-tests', name: 'Fix bug with tests',
    description: 'Sub-agents reproduce and fix the bug; the lead reviews and integrates the result.',
    leadPrompt: 'Inspect the report and relevant context, identify likely ownership boundaries, and sequence reproduction before implementation and review.',
    orchestrator: 'Report the root cause, fix, and verification result. Say what could not be verified.', strategy: 'sequential', builtin: true,
    agents: [
      member('Reproduce', 'codex', 'Create or identify a focused reproduction for the reported bug. Do not fix it yet.'),
      member('Fix', 'claude', 'Fix the confirmed root cause and run the focused verification. Report the result.'),
      member('Review', 'codex', 'Review the diff for concrete side effects. Do not edit files.'),
    ],
  },
];

const plain = <T>(value: T): T => JSON.parse(JSON.stringify(value));
const validStrategies = new Set<WorkflowStrategy>(['adaptive', 'parallel', 'sequential']);

/** Converts saved step-based presets to lead-and-roster flows while preserving their IDs and assignments. */
function migratePreset(value: unknown): WorkflowPreset | null {
  if (!value || typeof value !== 'object') return null;
  const raw = value as Partial<WorkflowPreset> & { steps?: Array<Record<string, unknown>>; mode?: string };
  if (!raw.id || !raw.name) return null;
  if (Array.isArray(raw.agents)) {
    return {
      id: raw.id, name: raw.name, description: raw.description ?? '',
      leadPrompt: raw.leadPrompt ?? defaultLead,
      orchestrator: raw.orchestrator ?? 'Combine the reports into one answer.',
      strategy: validStrategies.has(raw.strategy as WorkflowStrategy) ? raw.strategy as WorkflowStrategy : 'adaptive',
      coordination: raw.coordination,
      agents: raw.agents.filter((item): item is WorkflowMember => Boolean(item && item.id && item.name && item.agent)).map((item) => ({ ...item, prompt: item.prompt ?? '' })),
    };
  }
  const old = Array.isArray(raw.steps) ? raw.steps : [];
  const planner = raw.mode === 'team'
    ? old[0]
    : old.find((item) => /\b(plan|planner|planning|ba|tech lead)\b/i.test(`${String(item.name ?? '')} ${String(item.prompt ?? '')}`));
  const agents: WorkflowMember[] = old.filter((item) => item !== planner).map((item) => ({
    id: String(item.id ?? crypto.randomUUID()), name: String(item.name ?? 'Agent'),
    agent: (item.agent ?? 'claude') as Agent, model: String(item.model ?? ''),
    prompt: String(item.prompt ?? ''), effort: typeof item.effort === 'string' ? item.effort : undefined,
  }));
  if (agents.length === 0) agents.push(member('Agent 1', 'claude', 'Take an appropriate part of the request and report your result.'));
  const parallel = old.filter((item) => item !== planner).map((item) => Boolean(item.parallel));
  const parallelCount = parallel.filter(Boolean).length;
  const strategy: WorkflowStrategy = raw.mode === 'team' || (parallel.length > 0 && parallelCount === parallel.length - 1)
    ? 'parallel'
    : parallelCount > 0 ? 'adaptive' : 'sequential';
  return {
    id: raw.id, name: raw.name, description: raw.description ?? '',
    leadPrompt: planner ? String(planner.prompt ?? defaultLead) : defaultLead,
    orchestrator: raw.orchestrator ?? 'Combine the reports into one answer.', agents, strategy,
    coordination: raw.coordination,
  };
}

export const workflows = $state<{ presets: WorkflowPreset[]; assigned: Record<string, string> }>({ presets: [], assigned: {} });
void getPref<unknown[]>('workflows.presets').then((saved) => {
  if (!Array.isArray(saved)) return;
  const migrated = saved.map(migratePreset).filter((item): item is WorkflowPreset => Boolean(item && !item.builtin));
  workflows.presets.push(...migrated);
  if (saved.some((item) => Boolean(item && typeof item === 'object' && 'steps' in item))) setPref('workflows.presets', plain(migrated));
});
void getPref<Record<string, string>>('workflows.assigned').then((saved) => { if (saved) Object.assign(workflows.assigned, saved); });

const saveAll = () => setPref('workflows.presets', plain(workflows.presets));
export const allPresets = () => [...BUILT_IN_PRESETS, ...workflows.presets];
export const presetById = (id: string | undefined) => (id ? allPresets().find((item) => item.id === id) : undefined);
export const presetOf = (sessionId: string) => presetById(workflows.assigned[sessionId]);

export const newMember = (index: number): WorkflowMember => member(`Agent ${index}`, 'claude', 'Take an appropriate part of the request and report your result.');
export const newPreset = (): WorkflowPreset => ({
  id: crypto.randomUUID(), name: 'New flow', description: '', leadPrompt: defaultLead,
  orchestrator: 'Combine the reports into one answer for the user.', agents: [newMember(1)], strategy: 'adaptive',
});

export const flowEditor = $state<{ draft: WorkflowPreset | null; isNew: boolean }>({ draft: null, isNew: false });
export function savePreset(preset: WorkflowPreset) {
  const copy: WorkflowPreset = { ...plain(preset), builtin: undefined };
  const at = workflows.presets.findIndex((item) => item.id === copy.id);
  if (at === -1) workflows.presets.push(copy); else workflows.presets[at] = copy;
  saveAll();
}
export function editFlow(preset: WorkflowPreset) {
  const source = preset.builtin ? { ...plain(preset), id: crypto.randomUUID(), name: `${preset.name} (copy)`, builtin: undefined } : plain(preset);
  if (preset.builtin) source.agents = source.agents.map((item) => ({ ...item, id: crypto.randomUUID() }));
  flowEditor.draft = source;
  flowEditor.isNew = Boolean(preset.builtin) || !workflows.presets.some((item) => item.id === preset.id);
}
export const createFlow = () => editFlow(newPreset());
export function closeFlow() { flowEditor.draft = null; flowEditor.isNew = false; }
export function duplicatePreset(id: string): WorkflowPreset | undefined {
  const source = presetById(id);
  if (!source) return undefined;
  const copy = { ...plain(source), id: crypto.randomUUID(), name: `${source.name} (copy)`, builtin: undefined };
  copy.agents = copy.agents.map((item) => ({ ...item, id: crypto.randomUUID() }));
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
export function assignPreset(sessionId: string, presetId: string | null) {
  if (presetId) workflows.assigned[sessionId] = presetId; else delete workflows.assigned[sessionId];
  setPref('workflows.assigned', plain(workflows.assigned));
}
export function flowProblem(preset: WorkflowPreset | null): string {
  if (!preset) return '';
  if (!preset.name.trim()) return 'Give the flow a name.';
  if (!preset.agents.length) return 'Add at least one sub-agent.';
  if (preset.agents.some((item) => !item.name.trim())) return 'Give every sub-agent a role name.';
  return '';
}
