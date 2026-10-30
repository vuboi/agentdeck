import { type Message, type Block } from '../mock';
import { demoRuns } from '../../tasks/demo-runs';
import { inTauri } from '../../app/platform/history';
import { getPref, setPref } from '../../app/platform/prefs';
import { estimateCost } from '../runs/usage';
import { localMessages, workspace, type Agent, type Mention } from './state.svelte';
import { agentLabel } from './turns.svelte';
import { liveReplies } from './run.svelte';
import { taskWaiters } from './orchestration.svelte';
import { chat, transcripts } from './history.svelte';

// Dynamic sub-agent runs and inline mention context. Inline model mentions are resolved through AgentDeck MCP tools.

export type DelegateReports = {
  task: string;
  results: { label: string; text: string }[];
  orchestrator?: string;
  memo?: string;
};
export type DelegateCard = Extract<Block, { kind: 'delegate' }>;

// What the Workflow tab draws: one run per flow message (lead model -> delegated agents -> integration).
export type RunStats = {
  ms: number;
  tokens?: import('../runs/usage').Tokens;
  cost: number | null;
  tools: number;
  last: string;
};
export type RunNode = {
  childId: string;
  input?: string;
  output?: string;
  agent: Agent;
  model: string;
  label: string;
  startedAt: number;
  endedAt?: number;
  state: 'idle' | 'running' | 'done' | 'failed';
  note?: string;
  stats?: RunStats;
};
export type DelegateRun = {
  id: string;
  main: string;
  task: string;
  /** Name of the workflow preset that planned this run; none for an `@agent` delegation. */
  preset?: string;
  /** The user stopped the lead chat, so delegated work is cancelled. */
  cancelled?: boolean;
  startedAt: number;
  nodes: RunNode[];
  /** waiting: children still working; running: the lead model is integrating their reports. */
  handback: 'waiting' | 'running' | 'done' | 'failed';
  handbackAt?: number;
  endedAt?: number;
  handbackStats?: RunStats;
};
export const RUN_LIMIT = 100;
export const delegateRuns = $state<DelegateRun[]>([]);
const activeWorkflowByMain = new Map<string, DelegateRun>();

/** Starts a history entry for a model-led flow. Its roster is guidance; workers are added as the model delegates. */
export function beginWorkflowRun(main: string, task: string, preset: import('../../tasks/workflows.svelte').WorkflowPreset): DelegateRun {
  const startedAt = Date.now();
  const run: DelegateRun = { id: crypto.randomUUID(), main, task, preset: preset.name, startedAt, nodes: [], handback: 'running', handbackAt: startedAt };
  delegateRuns.push(run);
  if (delegateRuns.length > RUN_LIMIT) delegateRuns.splice(0, delegateRuns.length - RUN_LIMIT);
  activeWorkflowByMain.set(main, run);
  return run;
}

/** Adds the worker started by delegate_task to the current model-led workflow history. */
export function addWorkflowWorker(main: string, childId: string, agent: Agent, model: string, label: string, task: string, queued: boolean): string | undefined {
  const run = activeWorkflowByMain.get(main);
  if (!run || run.endedAt) return undefined;
  const node = { childId, agent, model, label, input: task, startedAt: Date.now(), state: queued ? 'idle' : 'running', note: queued ? 'Queued for an available worker slot' : undefined } as RunNode;
  // A follow-up to a reused child is the same worker: refresh its node instead of counting it twice
  // (updateWorkflowWorker finds the first node by childId, so a second one would never update).
  const index = run.nodes.findIndex((item) => item.childId === childId);
  if (index >= 0) run.nodes[index] = node;
  else run.nodes.push(node);
  return run.id;
}

/** Shows the lead model integrating reports returned by background workers. */
export function beginWorkflowIntegration(main: string, runId?: string): void {
  const run = runId ? delegateRuns.find((item) => item.id === runId && item.main === main) : activeWorkflowByMain.get(main);
  if (!run || run.endedAt) return;
  run.handback = 'running';
  run.handbackAt = Date.now();
}

/** Ends the lead model's integration pass for a flow run. */
export function finishWorkflowIntegration(main: string, reply: Message, failed: boolean): void {
  if (reply.taskIds && !reply.workflowRunId) return;
  const run = delegateRuns.find((item) => item.main === main && item.handback === 'running' && (!reply.workflowRunId || item.id === reply.workflowRunId));
  if (!run) return;
  run.handback = failed ? 'failed' : 'done';
  run.endedAt = Date.now();
  run.handbackStats = statsOf(reply, run.handbackAt ?? run.startedAt);
  // A finished run is no longer the chat's current one: the map entry would hold it (and its nodes) for good.
  if (activeWorkflowByMain.get(main) === run) activeWorkflowByMain.delete(main);
}

/** Updates one worker in the history as the delegated task starts or finishes. */
export function updateWorkflowWorker(runId: string | undefined, childId: string, state: RunNode['state'], text?: string, reply?: Message): void {
  if (!runId) return;
  const run = delegateRuns.find((item) => item.id === runId);
  const node = run?.nodes.find((item) => item.childId === childId);
  if (!run || !node) return;
  node.state = state;
  node.note = text;
  if (state === 'done' || state === 'failed') node.endedAt = Date.now();
  if (reply) node.stats = statsOf(reply, node.startedAt);
}

/**
 * A worker ended with no report (the user or the caller stopped it). When no other worker can still run and no report is
 * waiting to be handed back, the flow run ends: without this it stayed "waiting" for good, and the chat's work never
 * counted as finished.
 */
export function settleWorkflowRun(runId: string | undefined, reportPending: boolean): void {
  if (!runId || reportPending) return;
  const run = delegateRuns.find((item) => item.id === runId);
  if (!run || run.endedAt || run.handback !== 'waiting') return;
  if (run.nodes.some((item) => item.state === 'idle' || item.state === 'running')) return;
  run.handback = 'done';
  run.endedAt = Date.now();
  if (activeWorkflowByMain.get(run.main) === run) activeWorkflowByMain.delete(run.main);
}

/** Marks the main model's pass over a flow; background workers may still return and trigger a final hand-back. */
export function finishWorkflowTurn(main: string, failed: boolean, reply?: Message): void {
  const run = activeWorkflowByMain.get(main);
  if (!run || run.endedAt) return;
  if (failed || run.nodes.length === 0) {
    run.handback = 'failed';
    run.endedAt = Date.now();
    if (reply) run.handbackStats = statsOf(reply, run.handbackAt ?? run.startedAt);
    activeWorkflowByMain.delete(main);
    return;
  }
  run.handbackAt = Date.now();
  if (reply) run.handbackStats = statsOf(reply, run.startedAt);
  if (run.nodes.some((item) => item.state === 'idle' || item.state === 'running')) {
    run.handback = 'waiting';
    run.handbackAt = Date.now();
  } else {
    run.handback = 'done';
    run.endedAt = Date.now();
    activeWorkflowByMain.delete(main);
  }
}
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
const clip = (text: string | undefined, max = 1500) => (text && text.length > max ? `${text.slice(0, max)}…` : text);
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

export function statsOf(reply: Message | undefined, startedAt: number): RunStats | undefined {
  if (!reply) return undefined;
  const text = reply.blocks.flatMap((block) => (block.kind === 'response' ? [block.text] : []));
  const last =
    text
      .join('\n')
      .split('\n')
      .reverse()
      .find((line) => line.trim()) ?? '';
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
export const isDelegating = (childId: string) => taskWaiters.has(childId);
/** Whether the chat `childId` stopped on an approval card that waits for the user. Not reactive: read it with a ticker. */
export const needsApproval = (childId: string) => {
  const list = childId === workspace.activeId ? chat.messages : (localMessages[childId] ?? []);
  return list.slice(-3).some((message) => message.blocks.some((block) => block.kind === 'permission' && block.state === 'pending'));
};
/** Whether a run that started some of these chats is still going (a queued worker may still be waiting for a slot). */
export const isRunActive = (childIds: string[]) => delegateRuns.some((run) => !run.endedAt && run.nodes.some((node) => childIds.includes(node.childId)));

export const firstLineOf = (text: string, max: number) => {
  const line = text.split('\n')[0].trim();
  return line.length > max ? `${line.slice(0, max - 1)}…` : line;
};

const plainText = (message: Message) =>
  message.blocks
    .flatMap((block) => (block.kind === 'response' ? [block.text] : block.kind === 'command' ? [`(ran: ${block.command.slice(0, 160)})`] : []))
    .join('\n')
    .trim();

/** The last messages of a chat as plain text, newest kept when it is long. */
export function digestOf(messages: Message[], limit = 6000): string {
  const lines = messages.slice(-8).map((message) => {
    const text = plainText(message);
    return `${message.role === 'user' ? 'User' : agentLabel(message.agent, message.model)}: ${text.length > 1500 ? `${text.slice(0, 1500)}…` : text}`;
  });
  const joined = lines.join('\n\n');
  return joined.length > limit ? `…${joined.slice(-limit)}` : joined;
}

/** Context and explicit sub-agent instructions resolved from inline mentions. */
export function mentionContext(mentions: Mention[]): string {
  const parts: string[] = [];
  const agents: string[] = [];
  for (const mention of mentions) {
    if (mention.kind === 'session') {
      const messages = transcripts.get(mention.id)?.messages ?? localMessages[mention.id] ?? [];
      parts.push(messages.length ? `Chat "${mention.label}", its latest messages:\n${digestOf(messages, 4000)}` : `Chat "${mention.label}" (not opened in this app yet, so no text of it is known here)`);
    } else if (mention.kind === 'project') {
      const project = workspace.projects.find((item) => item.id === mention.id);
      if (project) parts.push(`Project "${project.name}" is at ${project.path}`);
    } else if (mention.kind === 'ai') {
      const [agent, model] = mention.id.split('/');
      agents.push(`- @${mention.label}: provider=${agent}; model=${model}.`);
    }
  }
  const instructions = agents.length
    ? `The user included these inline sub-agent requests in their prompt:\n${agents.join('\n')}\nUse AgentDeck MCP tools to fulfill them: call list_agents to confirm availability, then delegate_task with the requested provider and exact model (including when it matches your provider). Keep ownership of integration and the final response. Start independent requests together with wait=false; use wait=true when a later task depends on an earlier result. If a requested model/provider is unavailable or the task fails because of quota/server errors, try available fallback agents from list_agents. If no agent can take the work, handle it yourself and say so. Do not silently skip a mention.`
    : '';
  const context = parts.length ? `The user included this chat/project context inline:\n\n${parts.join('\n\n')}\n\n---\n` : '';
  return [instructions, context].filter(Boolean).join('\n\n');
}

/** A child chat finished (or stopped, or failed): its card is settled, and the last one of a batch calls the agent back. */
export function onDelegateDone(childId: string, reply: Message, error: string | null) {
  const waiter = taskWaiters.get(childId);
  if (waiter) {
    taskWaiters.delete(childId);
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
}

