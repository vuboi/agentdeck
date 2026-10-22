// What the agents did, across every chat: each tool call with its status, newest last. The Tasks panel shows
// the ones still running (and those sent to the background); the Audit panel shows the whole log, kept on disk
// in `~/.agentdeck/audit.jsonl` so it survives a restart.

import type { AgentEvent } from './agent';
import { inTauri } from './history';

export type ActivityStatus = 'running' | 'background' | 'ok' | 'fail' | 'stopped';
export type Activity = {
  key: string;
  /** The tool id the CLI gave this call. Ids restart every turn, so the key adds a counter. */
  callId?: string;
  sessionId: string;
  title: string;
  /** The project the chat belongs to, by name ('' when none). Old entries have none. */
  project?: string;
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
let calls = 0;

/** Bumped whenever an agent writes, edits or deletes a file, so Explore reads its folders again. */
export const fileWrites = $state({ tick: 0 });

/** Finished calls and chat errors, oldest first: loaded from disk at start, then kept in step with each finish. */
export const audit = $state<Activity[]>([]);

async function tauriCall(command: string, args?: Record<string, unknown>) {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke(command, args);
}

if (inTauri()) {
  void tauriCall('audit_read')
    .then((text) => {
      const loaded: Activity[] = [];
      for (const line of String(text).split('\n')) {
        if (!line.trim()) continue;
        try {
          // Entries saved before keys were unique can share one: each loaded entry gets its own position in the key.
          const entry = JSON.parse(line) as Activity;
          loaded.push({ ...entry, key: `${entry.key}#${loaded.length}` });
        } catch {
          // A line cut short by a crash is skipped, the rest still loads.
        }
      }
      audit.unshift(...loaded);
    })
    .catch(() => {});
}

/** A finished call goes to the Audit log and to disk. */
function record(item: Activity) {
  // Tool ids restart in every turn (Codex counts from item_0 each time), so a saved entry gets its own key.
  const entry = { ...item, key: `${item.key}@${item.startedAt}`, output: item.output?.slice(0, 4000) };
  audit.push(entry);
  if (inTauri()) void tauriCall('audit_append', { line: JSON.stringify(entry) }).catch(() => {});
}

/** Keeps the log in step with one event of a chat's run. */
export function trackActivity(sessionId: string, title: string, project: string, event: AgentEvent) {
  if (event.kind === 'tool_call') {
    activity.push({
      key: `${sessionId}:${event.id}:${++calls}`,
      callId: event.id,
      sessionId,
      title,
      project,
      name: event.name,
      input: event.input,
      background: event.background,
      status: 'running',
      startedAt: Date.now(),
    });
    if (activity.length > LIMIT) activity.splice(0, activity.length - LIMIT);
  } else if (event.kind === 'tool_result') {
    // The call still open with this id, else the latest one with it.
    const same = (entry: Activity) => entry.sessionId === sessionId && entry.callId === event.id;
    const item = activity.findLast((entry) => same(entry) && (entry.status === 'running' || entry.status === 'background')) ?? activity.findLast(same);
    if (!item) return;
    item.output = event.output.length > 4000 ? `${event.output.slice(0, 4000)}…` : event.output;
    if (event.is_error) item.status = 'fail';
    // A background shell answers at once with its id; it keeps running after that.
    else item.status = item.background ? 'background' : 'ok';
    if (item.status !== 'background') {
      item.endedAt = Date.now();
      record(item);
    }
    if (item.status === 'ok' && (item.name === 'Write' || item.name === 'Edit' || item.name === 'Delete' || item.name === 'MultiEdit')) fileWrites.tick++;
  } else if (event.kind === 'done' || event.kind === 'error') {
    if (event.kind === 'error' && event.message !== 'stopped') {
      const now = Date.now();
      record({
        key: `${sessionId}:error:${now}`,
        sessionId,
        title,
        project,
        name: 'Chat error',
        input: event.message,
        background: false,
        status: 'fail',
        startedAt: now,
        endedAt: now,
        output: event.message,
      });
    }
    // The run is over: what was still in progress ended with it. The CLI exits after a run, and the shells it
    // left in the background go with it.
    for (const item of activity) {
      if (item.sessionId !== sessionId || (item.status !== 'running' && item.status !== 'background')) continue;
      item.status = event.kind === 'error' || item.status === 'background' ? 'stopped' : 'ok';
      item.endedAt = Date.now();
      record(item);
    }
  }
}

export const clearActivity = () => activity.splice(0, activity.length);

/** Empties the Audit log, in memory and on disk. */
export function clearAudit() {
  audit.splice(0, audit.length);
  if (inTauri()) void tauriCall('audit_clear').catch(() => {});
}
