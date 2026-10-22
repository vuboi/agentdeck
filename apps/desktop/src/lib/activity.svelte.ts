// What the agents did, across every chat: each tool call with its status, newest last. The Tasks panel shows
// the ones still running (and those sent to the background); the Audit panel shows the whole log.

import type { AgentEvent } from './agent';

export type ActivityStatus = 'running' | 'background' | 'ok' | 'fail' | 'stopped';
export type Activity = {
  key: string;
  sessionId: string;
  title: string;
  name: string;
  input: string;
  background: boolean;
  status: ActivityStatus;
  startedAt: number;
  endedAt?: number;
  output?: string;
};

const LIMIT = 1000;
export const activity = $state<Activity[]>([]);

/** Keeps the log in step with one event of a chat's run. */
export function trackActivity(sessionId: string, title: string, event: AgentEvent) {
  if (event.kind === 'tool_call') {
    activity.push({
      key: `${sessionId}:${event.id}`,
      sessionId,
      title,
      name: event.name,
      input: event.input,
      background: event.background,
      status: 'running',
      startedAt: Date.now(),
    });
    if (activity.length > LIMIT) activity.splice(0, activity.length - LIMIT);
  } else if (event.kind === 'tool_result') {
    const item = activity.findLast((entry) => entry.key === `${sessionId}:${event.id}`);
    if (!item) return;
    item.output = event.output.length > 4000 ? `${event.output.slice(0, 4000)}…` : event.output;
    if (event.is_error) item.status = 'fail';
    // A background shell answers at once with its id; it keeps running after that.
    else item.status = item.background ? 'background' : 'ok';
    if (item.status !== 'background') item.endedAt = Date.now();
  } else if (event.kind === 'done' || event.kind === 'error') {
    // The run is over: what was still in progress ended with it. The CLI exits after a run, and the shells it
    // left in the background go with it.
    for (const item of activity) {
      if (item.sessionId !== sessionId || (item.status !== 'running' && item.status !== 'background')) continue;
      item.status = event.kind === 'error' || item.status === 'background' ? 'stopped' : 'ok';
      item.endedAt = Date.now();
    }
  }
}

export const clearActivity = () => activity.splice(0, activity.length);
