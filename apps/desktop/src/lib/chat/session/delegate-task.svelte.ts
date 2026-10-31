import { invoke } from '@tauri-apps/api/core';
import { type TaskRecord, type AgentRow, formatTaskResult, replacementTask, MAX_WORKERS, type Verdict, kindOfCommand, pickTask, argsOfCall, parseDelegateArgs, findAgent, formatAgents, privateContentIn, resolveModel, newTaskId, REPORT_RULE, startedText, reportDigest, reasonLine, isTaskId, workerLanguageRule, FOLLOW_UP_PREFACE } from '../../tasks/delegate-task';
import { getPref, setPref } from '../../app/platform/prefs';
import { connectors, roomLine, agentHasRoom, usagePct } from '../../agents/connectors.svelte';
import { elapsed } from '../content/blocks';
import { type Block } from '../mock';
import { type DelegateCall, enterWorkerSlot, releaseWorkerSlot, replyDelegateCall, delegateReady, listenDelegateCalls, listenWorkerSlots, listenDelegateAborts } from '../agent';
import { customModelId, modelLabel } from '../../agents/models.svelte';
import { logFlow } from '../../tasks/flow-log';
import { inTauri } from '../../app/platform/history';
import { startPageMemoryBridge } from '../../browser/page-memory-bridge';
import { MODELS, localMessages, persistLocal, workspace, type Agent, type SessionItem } from './state.svelte';
import { agentLabel, chatTurnOptions, failSessionWork, hasTurnWork, hiddenPrompt, queuePaused, queued, stopTurn, turnOptionsOf } from './turns.svelte';
import { NO_HANDOUT, askText, liveAsks, liveReplies, messagesOf, replies, runReplies, sendNextQueued } from './run.svelte';
import { addWorkflowWorker, delegateRuns, digestOf, firstLineOf, needsApproval, settleWorkflowRun, updateWorkflowWorker } from './delegation.svelte';
import { shorten, taskWaiters } from './orchestration.svelte';
import { handbackPrompt, send } from './sending.svelte';
import { lastEventAt, lastEventKind, manuallyStopped } from './events.svelte';
import { activity } from '../runs/activity.svelte';
import { continuePrompt } from '../recovery/turn-failure';
import { canonicalRole, routesOf } from '../../agents/task-roles';
import { taskRoles } from '../../agents/task-roles.svelte';

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
type TaskRun = {
  callId?: string;
  timer?: ReturnType<typeof setTimeout>;
  ending?: Ending;
  remainingMs: number;
  deadline: number;
  started?: boolean;
  start?: () => void;
  slotRequest?: Promise<boolean>;
  release?: Promise<void>;
  generation: number;
};
export const taskRuns = new Map<string, TaskRun>();

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
        : item.reportState === 'running' || item.reportState === 'waiting'
          ? { ...item, reportState: 'failed', note: 'Worker result saved; returning its report was interrupted by reload.' }
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
    const active = all.filter((item) => ['queued', 'running', 'approval'].includes(item.state) || item.reportState === 'waiting' || item.reportState === 'running');
    const activeIds = new Set(active.map((item) => item.id));
    setPref(TASKS_KEY, [...active, ...all.filter((item) => !activeIds.has(item.id)).slice(-TASK_LIMIT)]);
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

/** The chat identified by the bridge; an unknown caller must not be assigned to another chat. */
function callerOf(chatId: string): SessionItem | undefined {
  return chatId ? workspace.sessions.find((item) => item.id === chatId) : undefined;
}

/** The agents `list_agents` shows and `delegate_task` takes: the installed CLIs, then each local provider. */
function agentRows(): AgentRow[] {
  const installed = (tool: string) =>
    Boolean(connectors.detected?.tools.find((item) => item.id === tool)?.path);
  const rows: AgentRow[] = [];
  for (const [id, tool] of [
    ['claude', 'claude'],
    ['codex', 'codex'],
    ['antigravity', 'agy'],
    ['opencode', 'opencode'],
    ['copilot', 'gh'],
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
    (record.state === 'done' ? record.digest : record.note) ?? '',
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
/** The first agent of a role's routes (Settings > Task roles) that is available now, with its model. */
function firstRouted(role: string, rows: AgentRow[]): { agent: string; model: string } | undefined {
  const id = canonicalRole(role);
  if (!id) return undefined;
  for (const route of routesOf(taskRoles, id)) {
    const [agent, ...rest] = route.split('|');
    if (findAgent(rows, agent)) return { agent, model: rest.join('|').replace(/^credit:/, '') };
  }
  return undefined;
}

function startTask(
  call: DelegateCall,
  answer: (text: string, isError?: boolean) => void,
) {
  const parsed = parseDelegateArgs(call.args);
  if (!parsed.ok) return answer(parsed.error, true);
  const {
    agent: wanted,
    model: requestedModel,
    task,
    wait,
    timeoutS,
    continueId,
    role,
  } = parsed.args;
  const caller = callerOf(call.caller);
  const reply = answer;
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
  // `continue`: reuse an idle worker chat (found by task id or chat id) instead of creating a new one.
  let reuse: { session: SessionItem; record?: TaskRecord } | undefined;
  if (continueId) {
    const prior = delegateTasks[continueId];
    let childId = prior?.childId ?? continueId;
    // Follow persisted replacement links without changing historical task cards.
    const seen = new Set<string>();
    while (!seen.has(childId)) {
      seen.add(childId);
      const next = Object.values(delegateTasks).find((item) => item.childId === childId && item.replacementChildId)?.replacementChildId;
      if (!next) break;
      childId = next;
    }
    const session = workspace.sessions.find((item) => item.id === childId && item.parentId);
    if (!session)
      return reply(
        `continue="${continueId}" is unknown: no delegated task or worker chat has that id. Use a task_id from an earlier delegate_task result or a worker chat id.`,
        true,
      );
    const records = Object.values(delegateTasks).filter((item) => item.childId === childId);
    if (session.status === 'working' || records.some((item) => taskRuns.has(item.id)))
      return reply('worker is still running, use task_status or wait', true);
    reuse = { session, record: records.sort((a, b) => a.startedAt - b.startedAt).at(-1) };
  }
  const rows = agentRows();
  // A role with agents set in Task roles: the first of them that is available is used, whatever agent the caller named.
  const routed = role ? firstRouted(role, rows) : undefined;
  const wantedModel = routed ? routed.model : requestedModel;
  const oldRowId = reuse?.record?.agentId ?? reuse?.session.agent;
  const row = findAgent(rows, routed?.agent ?? (wanted || oldRowId || ''));
  if (!row) return reply(`No agent "${wanted || oldRowId}" is available. ${formatAgents(rows)}`, true);
  const inheritedModel = row.id === caller.agent && row.models.some((item) => item.value === caller.model) ? caller.model : undefined;
  const sameProvider = Boolean(reuse && row.id === oldRowId);
  const chosen = sameProvider && !wantedModel ? { model: reuse!.session.model ?? '' } : resolveModel(
    row.models, wantedModel ?? inheritedModel, row.id === 'claude' ? 'sonnet' : undefined,
  );
  if ('error' in chosen) return reply(chosen.error, true);
  const agent: Agent = row.id === 'claude' || row.id === 'codex' || row.id === 'antigravity' || row.id === 'opencode' || row.id === 'copilot' ? row.id : 'custom';
  const model = sameProvider && !wantedModel ? chosen.model : agent === 'custom' ? customModelId(row.id, chosen.model) : chosen.model;
  const old = reuse;
  const failed = old?.record && (old.record.state === 'failed' || old.record.state === 'stopped');
  const replace = Boolean(old && (failed || !sameProvider || model !== old.session.model));
  let workerTask = task;
  if (replace && old) {
    const transcript = digestOf(messagesOf(old.session.id).filter((message) => message.blocks.some((block) => block.kind === 'response' ? Boolean(block.text.trim()) : block.kind === 'command' && Boolean(block.command.trim()))));
    const report = old.record?.digest ?? '';
    const reason = failed ? old.record?.note ?? `Previous worker ${old.record?.state}.` : '';
    workerTask = replacementTask(task, old.record?.label ?? `${agentLabel(old.session.agent, old.session.model)} · ${modelLabel(old.session.model ?? '')}`, transcript, report, reason);
    reuse = undefined;
  }
  // Check every byte of the seed as well as the new request before selecting a restricted provider.
  const leak = row.restricted ? privateContentIn(workerTask) : null;
  if (leak) return reply(`Not sent: ${row.label} is a free tier that may keep what it is sent, and this task holds ${leak}. Do this part yourself or give it to another agent; send the free tier only public, non-sensitive text.`, true);
  let taskId = newTaskId();
  while (delegateTasks[taskId]) taskId = newTaskId();

  const uuid = crypto.randomUUID();
  const id = reuse ? reuse.session.id : `${agent}:${uuid}`;
  const label = `${agentLabel(agent, model)} · ${modelLabel(model)}`;
  if (replace && old) for (const record of Object.values(delegateTasks)) if (record.childId === old.session.id) record.replacementChildId = id;
  const originatingRun = [...runReplies.values()].find((run) => run.sessionId === caller.id && run.reply.blocks.some((block) => block.kind === 'command' && kindOfCommand(block.command) === 'delegate_task' && argsOfCall(block.command.replace(/^\S+\s*/, '')).task === task));
  const source = originatingRun?.ask ?? liveAsks[caller.id];
  if (!reuse) workspace.sessions.push({
    id,
    projectId: caller.projectId,
    title: `@${agentLabel(agent, model)}: ${firstLineOf(task, 50)}`,
    agent,
    agentSession: agent === 'claude' ? uuid : undefined,
    model,
    gitBranch: caller.gitBranch,
    status: 'idle',
    parentId: caller.id,
    draft: false,
  });
  // A chat with no messages and no draft flag is not a local chat: the next history import rebuilt the list without it, and a
  // worker waiting for a free place then had no chat to start in.
  localMessages[id] ??= [];
  delegateTasks[taskId] = {
    id: taskId,
    caller: caller.id,
    sourceMessageId: source?.id,
    sourceTask: source ? askText(source) : task,
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
  const run: TaskRun = { callId: wait ? call.callId : undefined, remainingMs: timeoutS * 1000, deadline: Date.now() + timeoutS * 1000, generation: 0 };
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
  // The worker runs with the settings of the chat that called it, not with those of whichever chat is on screen.
  chatTurnOptions[id] = { ...turnOptionsOf(caller.id) };
  bindTasks(caller.id);

  // The worker's end: its report, or why there is none. A worker stopped on an approval card waits for the user's yes.
  const settle = (answer: string, error: string | null, workerReply?: import('../mock').Message) => {
    const record = delegateTasks[taskId];
    if (!record || !taskRuns.has(taskId)) return;
    // Pausing ends only the provider turn. Keep the task's completion waiter for Resume.
    if (record.pausedAt !== undefined && !run.ending) {
      taskWaiters.set(id, settle);
      return;
    }
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
  const languageRule = workerLanguageRule(delegateTasks[taskId].sourceTask ?? task, usagePct(row.id, model));
  const brief = reuse ? `${FOLLOW_UP_PREFACE} ${REPORT_RULE} ${languageRule}` : `You were asked by another agent (${agentLabel(caller.agent, caller.model)}), who is working for the user in this project, to do the task below on your own. Answer fully, as you would for the user. ${REPORT_RULE} ${languageRule}`;
  // At most MAX_PARALLEL_WORKERS run at once (delegate.rs keeps the line); the rest wait as `queued`. A free place
  // starts the worker at once, so three calls of one turn run side by side.
  run.start = () => {
    const record = delegateTasks[taskId];
    if (!record || record.pausedAt !== undefined || record.state !== 'queued' || !taskRuns.has(taskId)) return;
    record.state = 'running';
    if (!run.started) record.startedAt = Date.now();
    updateWorkflowWorker(record.workflowRunId, record.childId, 'running');
    delete manuallyStopped[id];
    if (run.started) send(id, '', [], [], [], { prompt: continuePrompt('the user paused this worker', true, record.task), silent: NO_HANDOUT });
    if (!run.started) send(id, workerTask, [], [], [], { preface: `${brief}\n\n` });
    run.started = true;
    persistLocal();
  };
  slotStarts.set(taskId, run.start);
  persistLocal();
  logFlow(id, 'worker slot', `task=${taskId} asks for a place`);
  run.slotRequest = enterWorkerSlot(taskId, caller.id).catch(() => true);
  const generation = run.generation;
  void run.slotRequest
    .then((granted) => {
      logFlow(id, 'worker slot', `task=${taskId} ${granted ? 'granted' : 'waits in line'}`);
      // Ended (stopped, timed out) before the gate answered: its release came first and freed nothing, so the place
      // it was just given goes back now.
      if (!taskRuns.has(taskId)) {
        void releaseWorkerSlot(taskId).catch(() => undefined);
        return;
      }
      if (!wait) reply(startedText({ taskId, childId: id, label }, !granted));
      if (generation !== run.generation) return;
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
  delete record.pausedAt;
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
  else { record.note = body; if (answer.trim()) record.digest = reportDigest(answer); }
  updateWorkflowWorker(record.workflowRunId, record.childId, verdict === 'done' ? 'done' : 'failed', body, workerReply);
  // Stopped by the user or the caller: no report is delivered, so nothing else would ever close the flow run.
  if (run.ending === 'user' || run.ending === 'caller') {
    const sameRun = (other: TaskRecord) => other.id !== taskId && other.workflowRunId === record.workflowRunId;
    const pending = Object.values(delegateTasks).some((other) => sameRun(other) && (other.reportState === 'waiting' || other.reportState === 'running'))
      || (backlog.get(record.caller) ?? []).some((report) => report.taskId !== taskId && delegateTasks[report.taskId]?.workflowRunId === record.workflowRunId);
    settleWorkflowRun(record.workflowRunId, pending);
  }
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
      taskId,
      sourceMessageId: record.sourceMessageId,
      sourceTask: record.sourceTask ?? record.task,
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

/** The user answered an approval card in a worker's chat: "no" ends its task, "yes" puts it back to running. */
export function answerWorkerApproval(childId: string, allowed: boolean) {
  const record = Object.values(delegateTasks).find((item) => item.childId === childId && item.state === 'approval');
  if (!record) return;
  if (allowed) record.state = 'running';
  else stopTask(record.id, 'user');
}

/** Pauses one worker without completing its task or returning a partial report to the caller. */
export function pauseDelegateTask(taskId: string): void {
  const record = delegateTasks[taskId];
  const run = taskRuns.get(taskId);
  if (!record || !run || record.pausedAt !== undefined || !['queued', 'running'].includes(record.state)) return;
  record.pausedAt = Date.now();
  run.remainingMs = Math.max(1, run.deadline - Date.now());
  clearTimeout(run.timer);
  run.generation++;
  slotStarts.delete(taskId);
  manuallyStopped[record.childId] = true;
  if (run.started) stopTurn(record.childId);
  // Wait for an outstanding grant before releasing it; a quick Resume must not lose its new slot.
  run.release = (run.slotRequest ?? Promise.resolve(false)).then(() => releaseWorkerSlot(taskId)).catch(() => undefined);
  updateWorkflowWorker(record.workflowRunId, record.childId, 'idle', 'Paused by you');
}

/** Resumes in the same child conversation and returns the final result to the original task. */
export async function resumeDelegateTask(taskId: string): Promise<void> {
  const record = delegateTasks[taskId];
  const run = taskRuns.get(taskId);
  if (!record || !run || record.pausedAt === undefined || !run.start) return;
  delete record.pausedAt;
  record.state = 'queued';
  const generation = ++run.generation;
  run.deadline = Date.now() + run.remainingMs;
  run.timer = setTimeout(() => stopTask(taskId, 'timeout', 'Worker time limit reached.'), run.remainingMs);
  await run.release;
  if (!taskRuns.has(taskId) || generation !== run.generation) return;
  slotStarts.set(taskId, run.start);
  run.slotRequest = enterWorkerSlot(taskId, record.caller).catch(() => true);
  const granted = await run.slotRequest;
  if (generation !== run.generation) return;
  if (!taskRuns.has(taskId)) {
    void releaseWorkerSlot(taskId).catch(() => undefined);
    return;
  }
  if (granted) startQueued(taskId);
}

/** Reports of background tasks waiting for their caller to be idle, per caller. */
type BackgroundReport = { label: string; text: string; taskId: string; sourceMessageId?: string; sourceTask: string };
export const backlog = new Map<string, BackgroundReport[]>();

function deliverReport(
  callerId: string,
  report: BackgroundReport,
) {
  const list = backlog.get(callerId) ?? [];
  if (list.some((item) => item.taskId === report.taskId)) return;
  list.push(report);
  backlog.set(callerId, list);
  const record = delegateTasks[report.taskId];
  if (record) record.reportState = 'waiting';
  sendNextQueued(callerId);
}

/** Returns a saved worker result without rerunning the completed work after an interrupted summary. */
export function returnTaskResult(taskId: string): void {
  const record = delegateTasks[taskId];
  if (!record || record.state !== 'done' || record.reportState !== 'failed' || !record.digest) return;
  deliverReport(record.caller, { taskId, sourceMessageId: record.sourceMessageId, sourceTask: record.sourceTask ?? record.task, label: record.label, text: record.digest });
}

/** Delivers reports from one original request when the chat is idle; unrelated requests are never merged. */
export function flushReports(callerId: string, tries: number) {
  const list = backlog.get(callerId);
  if (!list?.length) return;
  if (!workspace.sessions.some((item) => item.id === callerId)) {
    backlog.delete(callerId);
    return;
  }
  if (hasTurnWork(callerId) || queuePaused[callerId] || queued[callerId]?.length) return;
  const origin = list[0];
  const batch = list.filter((report) => report.sourceMessageId === origin.sourceMessageId && report.sourceTask === origin.sourceTask);
  const rest = list.filter((report) => !batch.includes(report));
  if (rest.length) backlog.set(callerId, rest);
  else backlog.delete(callerId);
  const taskIds = batch.map((report) => report.taskId);
  for (const id of taskIds) if (delegateTasks[id]) delegateTasks[id].reportState = 'running';
  send(callerId, '', [], [], [], {
    prompt: handbackPrompt({ results: batch, task: origin.sourceTask }),
    silent: NO_HANDOUT,
    report: { triggeredByMessageId: origin.sourceMessageId, taskTitle: origin.sourceTask, taskIds, workflowRunId: delegateTasks[taskIds[0]]?.workflowRunId },
    // A reply started before this hand-back could (a deferred send woke late): the report waits in line again and goes
    // out when the chat is idle, instead of being lost with its task stuck on "returning".
    onDropped: () => {
      const again = backlog.get(callerId) ?? [];
      for (const report of batch) if (!again.some((item) => item.taskId === report.taskId)) again.unshift(report);
      backlog.set(callerId, again);
      for (const id of taskIds) if (delegateTasks[id]) delegateTasks[id].reportState = 'waiting';
    },
  });
}

/** A caller's turn ended: the tasks it was waiting for have nobody to answer, so they stop. Background ones go on. */
export function endWaitingTasks(callerId: string) {
  for (const record of Object.values(delegateTasks)) {
    if (record.caller === callerId && record.wait && taskRuns.has(record.id))
      stopTask(record.id, 'abort');
  }
}

// ---------- watchdog: a worker that takes long is checked by its caller, a hung one is stopped ----------

/** First check-in after this long; the next ones come after twice the time before (10, 20, 40 min...). */
const CHECK_IN_MS = 10 * 60_000;
/** A worker silent this long (no event from its CLI) is asked about. */
const QUIET_MS = 4 * 60_000;
/** A worker silent this long is hung: it is stopped, and its caller hands the task on or does it itself. */
const HUNG_MS = 15 * 60_000;
/** The same while one of its commands still runs without printing (builds, installs, debug sessions). */
const HUNG_COMMAND_MS = 60 * 60_000;
/** The output size of each task's running commands, and when it last grew. */
const outputSeen = new Map<string, { size: number; at: number }>();
const descendantSeen = new Map<string, number>();
const checkIns = new Map<string, { at: number; count: number; quietTold: boolean }>();

let watchingWorkers = false;
async function watchWorkers() {
  if (watchingWorkers) return;
  watchingWorkers = true;
  try {
  const now = Date.now();
  for (const [taskId, run] of taskRuns) {
    const record = delegateTasks[taskId];
    if (!record || record.state !== 'running' || record.pausedAt !== undefined || !run.started) continue;
    const since = now - record.startedAt;
    // Activity is the CLI's events, and also the output of a command it runs (a build, a test run, a dev server): a
    // command that keeps printing is working even when the CLI itself says nothing until it ends.
    const tools = activity.filter((item) => item.sessionId === record.childId && item.name !== 'Request' && (item.status === 'running' || item.status === 'background'));
    const outputSize = tools.reduce((sum, item) => sum + (item.output?.length ?? 0), 0);
    const seen = outputSeen.get(taskId);
    if (!seen || seen.size !== outputSize) outputSeen.set(taskId, { size: outputSize, at: now });
    const lastOutput = outputSeen.get(taskId)!.at;
    // Sample every tick, so the decision compares CPU and I/O since the preceding check.
    let descendantActive = false;
    try {
      descendantActive = await invoke<boolean>('worker_descendant_activity', { chat: record.childId });
    } catch (error) {
      // An unavailable process snapshot is not evidence that the worker is idle.
      descendantActive = true;
      logFlow(record.childId, 'watchdog', `process check failed: ${String(error)}`);
    }
    if (taskRuns.get(taskId) !== run || delegateTasks[taskId] !== record || record.state !== 'running' || record.pausedAt !== undefined) continue;
    if (descendantActive) descendantSeen.set(taskId, Date.now());
    const quiet = Date.now() - Math.max(lastEventAt.get(record.childId) ?? record.startedAt, tools.length ? lastOutput : 0, descendantSeen.get(taskId) ?? 0);
    // A command still running with no output (a long build, a debugger at a breakpoint) gets far longer before it
    // counts as hung; plain silence with nothing running is a stuck CLI sooner.
    const hungAfter = tools.length ? HUNG_COMMAND_MS : HUNG_MS;
    if (quiet >= hungAfter && !descendantActive) {
      logFlow(record.childId, 'watchdog', `task=${taskId} hung (${span(quiet)} silent): stopped`);
      checkIns.delete(taskId);
      stopTask(taskId, 'timeout', `No activity from the worker for ${span(quiet)}: AgentDeck stopped it as hung. Hand the task to another agent from list_agents (delegate_task), or do it yourself if none can take it.`);
      continue;
    }
    // A worker that waits for the caller's own answer is checked by the caller already.
    if (run.callId) continue;
    const state = checkIns.get(taskId) ?? { at: 0, count: 0, quietTold: false };
    const due = since >= CHECK_IN_MS * 2 ** state.count && now - state.at > 60_000;
    const quietDue = quiet >= QUIET_MS && !state.quietTold;
    if (!due && !quietDue) continue;
    if (due) state.count++;
    if (quietDue) state.quietTold = true;
    state.at = now;
    checkIns.set(taskId, state);
    logFlow(record.childId, 'watchdog', `task=${taskId} check-in (running ${span(since)}, quiet ${span(quiet)})`);
    deliverReport(record.caller, {
      taskId: `${taskId}#check${state.count}${state.quietTold ? 'q' : ''}`,
      sourceMessageId: record.sourceMessageId,
      sourceTask: record.sourceTask ?? record.task,
      label: `${record.label} (task ${taskId}) - progress check, still running`,
      text:
        `Not finished yet: running ${span(since)}, last activity ${span(quiet)} ago (${lastEventKind.get(record.childId) ?? 'unknown'}). ` +
        `Check it with task_status ${taskId}. If it is progressing, let it run and tell the user briefly. If it looks stuck, ` +
        `stop it with task_stop and send a hint with delegate_task continue=${taskId}, or hand the task to another agent from list_agents, ` +
        `or do it yourself if no agent can take it. A worker silent for ${span(HUNG_MS)} is stopped by AgentDeck.`,
    });
  }
  for (const taskId of checkIns.keys()) if (!taskRuns.has(taskId)) checkIns.delete(taskId);
  for (const taskId of outputSeen.keys()) if (!taskRuns.has(taskId)) outputSeen.delete(taskId);
  for (const taskId of descendantSeen.keys()) if (!taskRuns.has(taskId)) descendantSeen.delete(taskId);
  } finally { watchingWorkers = false; }
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
  // The watchdog (watchWorkers): long workers get a progress check by their caller, hung ones are stopped.
  setInterval(() => void watchWorkers(), 60_000);
  // The Preview agent's memory of pages (page-memory.ts): its tools read and write the store through this.
  void startPageMemoryBridge();
  void listenDelegateAborts((callId) => {
    for (const [taskId, run] of taskRuns)
      if (run.callId === callId) stopTask(taskId, 'abort');
  });
}
