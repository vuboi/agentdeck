import { type Message, type Block } from '../mock';
import { type HandoffReport, REPORT_RULE, handoffOf } from '../../tasks/delegate-task';
import { type WorkflowStep } from '../../tasks/workflows.svelte';
import { demoRuns } from '../../tasks/demo-runs';
import { inTauri } from '../../app/platform/history';
import { getPref, setPref } from '../../app/platform/prefs';
import { estimateCost } from '../runs/usage';
import { modelLabel } from '../../agents/models.svelte';
import { localMessages, persistLocal, workspace, type Agent, type Attachment, type FileAttachment, type Mention, type PageElement, type SessionItem } from './state.svelte';
import { agentLabel, missedTurns } from './turns.svelte';
import { liveReplies, messagesOf, replies, sendNextQueued } from './run.svelte';
import { LANGUAGE_RULE, stepWaiters } from './workflow.svelte';
import { handBack, send } from './sending.svelte';
import { chat, history, keepHandout, transcripts } from './history.svelte';

// ---------- delegating to another agent ----------
// `@claude do this` in a chat does not switch the chat to Claude. The task runs in a chat of its own under this one
// (a child: it shows in the sidebar and can be opened), with a summary of this chat as its background. When it has
// answered, the answer goes back into this chat and the chat's own agent carries on with it. With several `@`s the
// children run side by side and the agent is called once, when the last one has answered.

export type DelegateBatch = {
  main: string;
  task: string;
  /** The message whose cards show the work. */ asked: Message;
  pending: Set<string>;
  results: { label: string; text: string }[];
  run: DelegateRun;
  /** A workflow preset's note to the agent that writes the final answer. */ orchestrator?: string;
  /** More for that agent: the findings the steps shared, where their notes are. */ memo?: string;
};
export type DelegateCard = Extract<Block, { kind: 'delegate' }>;
const delegations = new Map<
  string,
  { batch: DelegateBatch; label: string; card: DelegateCard; node: RunNode }
>();

// What the Workflow tab draws: one run per delegated message (main chat -> a child per agent -> handback -> main agent).
export type RunStats = {
  ms: number;
  tokens?: import('../runs/usage').Tokens;
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
  plan?: import('../../tasks/workflows.svelte').WorkflowStep[];
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
export const RUN_LIMIT = 100;
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

export function statsOf(
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

export const firstLineOf = (text: string, max: number) => {
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
export function mentionContext(mentions: Mention[]): string {
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

export function delegate(
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
export function onDelegateDone(childId: string, reply: Message, error: string | null) {
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
export function endCancelled(main: string, run: DelegateRun) {
  const session = workspace.sessions.find((item) => item.id === main);
  if (session && !replies[main]) session.status = 'idle';
  run.handback = 'failed';
  run.endedAt = Date.now();
  sendNextQueued(main);
}