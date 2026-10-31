import type { Activity } from './activity.svelte';
import type { TaskRecord } from '../../tasks/delegate-task';
import type { DelegateRun } from '../session/delegation.svelte';

/** One addressable piece of background work; source records remain owned by their existing runtime. */
export type ChatWork = {
  id: string;
  sessionId: string;
  title: string;
  state: 'queued' | 'running' | 'paused' | 'waiting' | 'input' | 'done' | 'failed' | 'stopped' | 'unknown';
  agent?: string;
  model?: string | null;
  kind: 'agent' | 'command';
  sourceMessageId?: string;
  sourceTitle?: string;
  /** The live Activity entry behind a provider-native project run. */
  activityKey?: string;
  /** The tool call a provider-native task belongs to: the chat's transcript card finds its row by it. */
  callId?: string;
  childId?: string;
  delegateId?: string;
  canStop?: boolean;
  canPause?: boolean;
  canResume?: boolean;
  pausedAt?: number;
  canReturn?: boolean;
  startedAt: number;
  endedAt?: number;
  detail: string;
  /** Stable command and folder identity used to collapse repeated Run project calls. */
  projectKey?: string;
};

/** Terminal results remain available in the tray instead of disappearing at completion. */
export const isWorkActive = (work: ChatWork) => ['queued', 'running', 'paused', 'waiting', 'input'].includes(work.state);

/** The only independent Stop targets a Work row may own. There is deliberately no main-chat fallback. */
export type WorkStopTarget = 'project' | 'delegate' | 'child' | null;
export function stopTarget(work: Pick<ChatWork, 'activityKey' | 'delegateId' | 'childId' | 'canStop'>): WorkStopTarget {
  if (!work.canStop) return null;
  if (work.activityKey) return 'project';
  if (work.delegateId) return 'delegate';
  if (work.childId) return 'child';
  return null;
}

/** Prefer the provider's human description to raw tool JSON in the always-visible task title. */
function workTitle(input: string): string {
  try {
    const value = JSON.parse(input) as Record<string, unknown>;
    for (const key of ['description', 'task', 'command', 'cmd', 'prompt']) {
      if (typeof value?.[key] === 'string' && value[key]) return String(value[key]);
    }
  } catch { /* Plain tool arguments are already readable text. */ }
  return input;
}

/** Projects provider events, connected-provider workers and workflow workers into one task tray without reading chat rows. */
export function projectChatWork(events: readonly Activity[], tasks: readonly TaskRecord[], runs: readonly DelegateRun[]): ChatWork[] {
  const work: ChatWork[] = [];
  for (const event of events) {
    if ((!event.background && event.name !== 'Run project') || /delegate_task/i.test(event.name)) continue;
    work.push({
      id: `native:${event.key}`, sessionId: event.sessionId, callId: event.callId,
      activityKey: event.key,
      title: event.taskLabel || workTitle(event.input) || event.name,
      state: event.status === 'ok' ? 'done' : event.status === 'fail' ? 'failed' : event.status === 'stopped' ? 'stopped' : 'running',
      agent: event.agent, model: event.model, kind: event.nativeTaskId || /agent|task|spawn/i.test(event.name) ? 'agent' : 'command',
      sourceMessageId: event.sourceMessageId, sourceTitle: event.sourceTitle,
      // AgentDeck owns project servers started by browser_serve, so they can stop independently of the chat turn.
      // Other provider-native tasks still have no per-task cancellation API and must not stop the whole reply.
      canStop: event.name === 'Run project' && ['running', 'background'].includes(event.status),
      startedAt: event.startedAt, endedAt: event.endedAt, detail: event.output ?? '',
      projectKey: event.name === 'Run project' ? projectIdentity(event.input) : undefined,
    });
  }
  for (const task of tasks) {
    work.push({
      id: `delegate:${task.id}`, sessionId: task.caller, title: task.task,
      state: task.state === 'stopped' && /closed or reloaded/.test(task.note ?? '') ? 'unknown' : task.pausedAt !== undefined && ['queued', 'running'].includes(task.state) ? 'paused' : task.state === 'approval' ? 'input' : task.state === 'done' && task.reportState === 'waiting' ? 'waiting' : task.state === 'done' && task.reportState === 'running' ? 'running' : task.state,
      agent: task.agent, model: task.model, kind: 'agent', sourceMessageId: task.sourceMessageId, sourceTitle: task.sourceTask,
      childId: task.childId, delegateId: task.id, startedAt: task.startedAt, endedAt: task.endedAt,
      canStop: ['queued', 'running', 'approval'].includes(task.state),
      canPause: task.pausedAt === undefined && ['queued', 'running'].includes(task.state),
      canResume: task.pausedAt !== undefined && ['queued', 'running'].includes(task.state),
      pausedAt: task.pausedAt,
      canReturn: task.state === 'done' && task.reportState === 'failed' && Boolean(task.digest),
      detail: [task.reportState === 'waiting' ? 'Worker finished. Waiting to return its report.' : task.reportState === 'running' ? 'Worker finished. Preparing the final report.' : task.reportState === 'failed' ? 'Worker result saved. The summary could not finish.' : '', task.digest ?? task.note ?? ''].filter(Boolean).join('\n\n'),
    });
  }
  const knownChildren = new Set(tasks.map((task) => task.childId));
  for (const run of runs) {
    for (const node of run.nodes) {
      if (knownChildren.has(node.childId)) continue;
      work.push({
        id: `workflow:${run.id}:${node.childId}`, sessionId: run.main, title: node.input || run.task,
        state: node.state === 'idle' ? 'queued' : node.state,
        agent: node.agent, model: node.model, kind: 'agent', sourceTitle: run.task, childId: node.childId, canPause: node.state === 'running', canStop: node.state === 'running',
        startedAt: node.startedAt, endedAt: node.endedAt, detail: node.output ?? node.note ?? '',
      });
    }
  }
  const unique: ChatWork[] = [];
  const projects = new Map<string, number>();
  for (const item of work) {
    if (!item.projectKey) {
      unique.push(item);
      continue;
    }
    const previous = projects.get(item.projectKey);
    if (previous === undefined) {
      projects.set(item.projectKey, unique.length);
      unique.push(item);
      continue;
    }
    const current = unique[previous];
    if (current && !isWorkActive(current) && isWorkActive(item)) unique[previous] = item;
  }
  return unique.sort((a, b) => Number(isWorkActive(b)) - Number(isWorkActive(a)) || b.startedAt - a.startedAt);
}

function projectIdentity(input: string): string {
  const [command = '', cwd = ''] = input.split('\n');
  return `${command.trim()}\0${cwd.replaceAll('\\', '/').replace(/\/+$/, '').toLowerCase()}`;
}
