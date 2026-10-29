import { type TaskRecord, type AgentRow, formatTaskResult, MAX_WORKERS, type Verdict, kindOfCommand, pickTask, argsOfCall, parseDelegateArgs, findAgent, formatAgents, privateContentIn, resolveModel, newTaskId, REPORT_RULE, startedText, reportDigest, reasonLine, isTaskId } from '../../tasks/delegate-task';
import { getPref, setPref } from '../../app/platform/prefs';
import { connectors, roomLine, agentHasRoom } from '../../agents/connectors.svelte';
import { elapsed } from '../content/blocks';
import { type Block } from '../mock';
import { type DelegateCall, enterWorkerSlot, releaseWorkerSlot, replyDelegateCall, delegateReady, listenDelegateCalls, listenWorkerSlots, listenDelegateAborts } from '../agent';
import { customModelId, modelLabel } from '../../agents/models.svelte';
import { logFlow } from '../../tasks/flow-log';
import { inTauri } from '../../app/platform/history';
import { startPageMemoryBridge } from '../../browser/page-memory-bridge';
import { MODELS, persistLocal, workspace, type Agent, type SessionItem } from './state.svelte';
import { agentLabel, failSessionWork, hiddenPrompt, stopTurn } from './turns.svelte';
import { NO_HANDOUT, askText, liveReplies, messagesOf, replies } from './run.svelte';
import { addWorkflowWorker, delegateRuns, firstLineOf, needsApproval, updateWorkflowWorker } from './delegation.svelte';
import { LANGUAGE_RULE, shorten, taskWaiters } from './orchestration.svelte';
import { handbackPrompt, send } from './sending.svelte';

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
export const taskRuns = new Map<
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
 * A chat that only works for another one: a delegated task's worker. A fork also sits under its
 * parent in the tree (`parentId`), but it is a chat of its own: it opens pages and delegates like any other, and its
 * state is its own.
 */
export function isWorkerChat(id: string): boolean {
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
export function bindTasks(callerId: string) {
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
  delegateTasks[taskId].workflowRunId = addWorkflowWorker(caller.id, id, agent, model, label, task, true);
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
  const settle = (answer: string, error: string | null, workerReply?: import('../mock').Message) => {
    const record = delegateTasks[taskId];
    if (!record || !taskRuns.has(taskId)) return;
    if (!run.ending && !error && needsApproval(id)) {
      record.state = 'approval';
      taskWaiters.set(id, settle);
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
    finishTask(taskId, verdict, run.ending ? '' : (error ?? ''), answer, workerReply);
  };
  taskWaiters.set(id, settle);
  const brief = `You were asked by another agent (${agentLabel(caller.agent, caller.model)}), who is working for the user in this project, to do the task below on your own. Answer fully, as you would for the user. ${REPORT_RULE} ${LANGUAGE_RULE}`;
  // At most MAX_PARALLEL_WORKERS run at once (delegate.rs keeps the line); the rest wait as `queued`. A free place
  // starts the worker at once, so three calls of one turn run side by side.
  slotStarts.set(taskId, () => {
    const record = delegateTasks[taskId];
    if (!record || record.state !== 'queued' || !taskRuns.has(taskId)) return;
    record.state = 'running';
    record.startedAt = Date.now();
    updateWorkflowWorker(record.workflowRunId, record.childId, 'running');
    send(id, task, [], [], [], { preface: `${brief}\n\n` });
    persistLocal();
  });
  persistLocal();
  logFlow(id, 'worker slot', `task=${taskId} asks for a place`);
  void enterWorkerSlot(taskId)
    // A gate that cannot answer must not block the work.
    .catch(() => true)
    .then((granted) => {
      logFlow(id, 'worker slot', `task=${taskId} ${granted ? 'granted' : 'waits in line'}`);
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
  workerReply?: import('../mock').Message,
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
  logFlow(record.childId, 'worker slot', `task=${taskId} released (${verdict})`);
  void releaseWorkerSlot(taskId).catch(() => undefined);
  taskWaiters.delete(record.childId);
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
  updateWorkflowWorker(record.workflowRunId, record.childId, verdict === 'done' ? 'done' : 'failed', body, workerReply);
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
export const backlog = new Map<string, { label: string; text: string }[]>();

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
export function flushReports(callerId: string, tries: number) {
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
export function endWaitingTasks(callerId: string) {
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
