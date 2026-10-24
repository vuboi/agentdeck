// What the agents did, across every chat: each tool call with its status, newest last. The Tasks panel shows
// the ones still running (and those sent to the background) and, below them, the whole log, kept on disk
// in `~/.agentdeck/audit.jsonl` so it survives a restart.

import type { AgentEvent } from './agent';
import { inTauri } from './history';
import { planRunRestore, terminalStep, updateInflight, loadInflight, withTerminal, withoutTerminal, type InflightRun, type ServerLike } from './inflight';
import { sessionTail } from './terminal-session';

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
  /** A picture the call returned (a screenshot): a data URL until it is saved, then its file name in the log. */
  image?: string;
  /** Run project: the port the server's log is kept under (found from the servers list when not known). */
  port?: number;
  /** Terminal: the tab the command runs in. */
  terminalId?: string;
};

/** A saved picture, read back as a data URL; one read per picture. */
const pictures = new Map<string, Promise<string>>();
export function auditImage(image: string): Promise<string> {
  if (image.startsWith('data:')) return Promise.resolve(image);
  let found = pictures.get(image);
  if (!found) {
    found = tauriCall('audit_image_read', { name: image }).then(String);
    found.catch(() => pictures.delete(image));
    pictures.set(image, found);
  }
  return found;
}

const LIMIT = 1000;
/** The most output a run keeps (and the log file stores). */
const OUTPUT_CAP = 20_000;
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

/** The log keeps two days: today and yesterday. At midnight yesterday's day before goes, by itself. */
const startOfYesterday = () => {
  const day = new Date();
  day.setHours(0, 0, 0, 0);
  day.setDate(day.getDate() - 1);
  return day.getTime();
};
function pruneAudit() {
  const cutoff = startOfYesterday();
  for (let index = audit.length - 1; index >= 0; index--) if (audit[index].startedAt < cutoff) audit.splice(index, 1);
  return tauriCall('audit_prune', { cutoff }).catch(() => 0);
}

if (inTauri()) {
  // Checked every hour too, so an app left open over midnight drops the old day as well.
  setInterval(() => void pruneAudit(), 3_600_000);
  void pruneAudit()
    .then(() => tauriCall('audit_read'))
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
  const entry = { ...item, key: `${item.key}@${item.startedAt}`, output: item.output?.slice(0, OUTPUT_CAP) };
  audit.push(entry);
  if (!inTauri()) return;
  const picture = entry.image?.startsWith('data:') ? entry.image : undefined;
  if (!picture) {
    void tauriCall('audit_append', { line: JSON.stringify(entry) }).catch(() => {});
    return;
  }
  // The picture goes to a file of its own; the log line, and the entry once saved, keep its name.
  void tauriCall('audit_image_save', { image: picture })
    .then((name) => {
      const saved = String(name);
      pictures.set(saved, Promise.resolve(picture));
      const shown = audit.find((item) => item.key === entry.key);
      if (shown) shown.image = saved;
      return tauriCall('audit_append', { line: JSON.stringify({ ...entry, image: saved }) });
    })
    .catch(() => void tauriCall('audit_append', { line: JSON.stringify({ ...entry, image: undefined }) }).catch(() => {}));
}

/** What two steps of a flow said to each other: one entry, so the Audit log shows how they worked it out. */
export function logTalk(sessionId: string, title: string, project: string, from: string, to: string, question: string, answer: string) {
  const now = Date.now();
  record({ key: `${sessionId}:talk:${++calls}`, sessionId, title, project, name: 'Discussion', input: `${from} → ${to}: ${question}`.slice(0, 200), background: false, status: 'ok', startedAt: now, endedAt: now, output: `${from} asked ${to}:\n${question}\n\n${to} answered:\n${answer}` });
}

/** Keeps the log in step with one event of a chat's run. */
export function trackActivity(sessionId: string, title: string, project: string, event: AgentEvent) {
  if (event.kind === 'raw') {
    // A step of the CLI the app does not turn into anything: its line as the CLI wrote it, to copy when debugging.
    const now = Date.now();
    record({ key: `${sessionId}:raw:${++calls}`, sessionId, title, project, name: `${event.source} · ${event.label}`, input: event.line.slice(0, 200), background: false, status: 'ok', startedAt: now, endedAt: now, output: event.line });
  } else if (event.kind === 'tool_call') {
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
    if (event.image) item.image = event.image;
    if (event.is_error) item.status = 'fail';
    // A background shell answers at once with its id; it keeps running after that.
    else item.status = item.background ? 'background' : 'ok';
    if (item.status !== 'background') {
      item.endedAt = Date.now();
      record(item);
      // The log keeps the picture; the live list does not need a copy of it.
      item.image = undefined;
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

/** The server a Run project entry started: its port, found by command and folder in the servers list. */
type ServerRow = { key: number; command: string; cwd: string; running: boolean };
async function serverFor(item: Activity): Promise<ServerRow | undefined> {
  const list = (await tauriCall('servers_list')) as ServerRow[];
  if (item.port !== undefined) return list.find((row) => row.key === item.port);
  const [command, cwd] = item.input.split('\n');
  const row = list.find((entry) => entry.command.trim() === command?.trim() && entry.cwd === cwd);
  if (row) item.port = row.key;
  return row;
}

/**
 * Reads again what a Run project or Terminal entry prints and puts it in `output`. Returns whether it is still
 * alive (the server runs, the command has not returned to the prompt); other entries are never alive.
 */
export async function refreshOutput(item: Activity): Promise<boolean> {
  try {
    if (item.name === 'Run project' && inTauri()) {
      const row = await serverFor(item);
      // A server that is not in the list any more (stopped, or from an earlier session) keeps the text it has.
      if (!row) {
        endRestored(item);
        return false;
      }
      item.output = String(await tauriCall('server_log', { port: item.port, lines: 200 })).slice(-OUTPUT_CAP);
      if (!row.running) endRestored(item);
      return row.running;
    }
    if (item.name === 'Terminal' && item.terminalId) {
      const text = sessionTail(item.terminalId, 200);
      if (text !== null) item.output = text.slice(-OUTPUT_CAP);
      return item.status === 'running';
    }
  } catch {
    // The output stays as it was.
  }
  return false;
}

/**
 * A project started from the Preview: shown as running in the Tasks panel, and written to the Audit log when it
 * answers or fails. Returns the function that ends it.
 */
export function trackRun(sessionId: string, title: string, project: string, input: string, port?: number) {
  // Every start is an entry of its own (new key, own start), also the same project started again.
  activity.push({ key: `${sessionId}:run:${++calls}`, sessionId, title, project, name: 'Run project', input, background: false, status: 'running', startedAt: Date.now(), port });
  // The entry as the list holds it (reactive), so the panel sees it end.
  const item = activity[activity.length - 1];
  // Kept for a reload: the page may go away while the server is still starting or running.
  const saved: InflightRun = { sessionId, title, project, input, port, startedAt: item.startedAt, settled: false };
  updateInflight((state) => void state.runs.push(saved));
  const forget = () => updateInflight((state) => void (state.runs = state.runs.filter((entry) => entry !== saved && !(entry.startedAt === saved.startedAt && entry.input === saved.input))));
  return (ok: boolean, output: string) => {
    if (ok) updateInflight((state) => void state.runs.forEach((entry) => entry.startedAt === saved.startedAt && entry.input === saved.input && (entry.settled = true)));
    else forget();
    item.status = ok ? 'ok' : 'fail';
    item.output = output.length > OUTPUT_CAP ? `${output.slice(0, OUTPUT_CAP)}…` : output;
    item.endedAt = Date.now();
    // The log keeps the end of what the server printed, not only the address it came up at.
    void (ok && inTauri() ? refreshOutput(item) : Promise.resolve(false)).finally(() => {
      item.output ||= output;
      record(item);
    });
  };
}

/** Terminal tabs: the entry of the command each one runs now. */
const terminalRuns = new Map<string, { item: Activity; runId: number }>();

/**
 * A command a Terminal tab runs (reported when it starts or changes, and with `null` when the prompt is back). A run
 * is told apart by its id, not by its command: the same command run again after the first one ended is a new entry.
 */
export function trackTerminal(id: string, run: { runId: number; command: string; cwd?: string; startedAt: number } | null) {
  const current = terminalRuns.get(id);
  const step = terminalStep(current?.runId, run);
  if (step === 'none') return;
  if (current && (step === 'finish' || step === 'restart')) {
    terminalRuns.delete(id);
    const item = current.item;
    item.status = 'ok';
    item.endedAt = Date.now();
    const text = sessionTail(id, 200);
    if (text !== null) item.output = text.slice(-OUTPUT_CAP);
    record(item);
    if (step === 'finish') updateInflight((state) => void Object.assign(state, withoutTerminal(state, id)));
  }
  if (!run) return;
  activity.push({ key: `terminal:${id}:${++calls}`, sessionId: '', title: 'Terminal', project: '', name: 'Terminal', input: run.command, background: false, status: 'running', startedAt: run.startedAt, terminalId: id });
  terminalRuns.set(id, { item: activity[activity.length - 1], runId: run.runId });
  updateInflight((state) => void Object.assign(state, withTerminal(state, { tabId: id, command: run.command, cwd: run.cwd, startedAt: run.startedAt })));
}

/** A run from before a reload that ended meanwhile (its tab is gone, or its shell is at the prompt): into the Audit log. */
export function finishSavedTerminal(saved: { tabId: string; command: string; cwd?: string; startedAt: number }, status: 'ok' | 'stopped') {
  const text = status === 'ok' ? sessionTail(saved.tabId, 200) : null;
  record({ key: `terminal:${saved.tabId}:${++calls}`, sessionId: '', title: 'Terminal', project: '', name: 'Terminal', input: saved.command, background: false, status, startedAt: saved.startedAt, endedAt: Date.now(), output: text?.slice(-OUTPUT_CAP) });
  updateInflight((state) => void Object.assign(state, withoutTerminal(state, saved.tabId)));
}

// ---- Run project entries found again after a reload: they end when their server does.
const restoredRuns = new Set<Activity>();
function endRestored(item: Activity) {
  if (!restoredRuns.delete(item) || item.status !== 'running') return;
  item.status = 'stopped';
  item.endedAt = Date.now();
  record(item);
  updateInflight((state) => void (state.runs = state.runs.filter((entry) => !(entry.startedAt === item.startedAt && entry.input === item.input))));
}

/** Gives the servers of the Rust side that outlived a reload their running entry again, and ends those that are gone. */
export function restoreRunProjects(servers: ServerLike[]) {
  const state = loadInflight();
  const known = { ports: new Set<number>(), inputs: [] as string[] };
  for (const item of activity) {
    if (item.name !== 'Run project' || item.status !== 'running') continue;
    if (item.port !== undefined) known.ports.add(item.port);
    known.inputs.push(item.input);
  }
  const plan = planRunRestore(state.runs, servers, known, Date.now());
  for (const { server, record: saved, startedAt } of plan.resume) {
    activity.push({ key: `${saved?.sessionId ?? ''}:run:${++calls}`, sessionId: saved?.sessionId ?? '', title: saved?.title ?? '', project: saved?.project ?? '', name: 'Run project', input: `${server.command}\n${server.cwd}`, background: false, status: 'running', startedAt, port: server.key });
    restoredRuns.add(activity[activity.length - 1]);
    if (!saved) state.runs.push({ sessionId: '', title: '', project: '', input: `${server.command}\n${server.cwd}`, port: server.key, startedAt, settled: true });
    else saved.port = server.key;
  }
  for (const saved of plan.stopped) {
    record({ key: `${saved.sessionId}:run:${++calls}`, sessionId: saved.sessionId, title: saved.title, project: saved.project, name: 'Run project', input: saved.input, background: false, status: 'stopped', startedAt: saved.startedAt, endedAt: Date.now(), port: saved.port });
  }
  const gone = new Set([...plan.stopped, ...plan.forget]);
  state.runs = state.runs.filter((entry) => !gone.has(entry));
  updateInflight((fresh) => void (fresh.runs = state.runs));
}

if (inTauri()) {
  // After a reload: the servers the Rust side still runs. Read once here; the list is not polled for this.
  void tauriCall('servers_list').then((list) => restoreRunProjects(list as ServerLike[])).catch(() => {});
}

export const clearActivity =() => activity.splice(0, activity.length);

/** Empties the Audit log, in memory and on disk. */
export function clearAudit() {
  audit.splice(0, audit.length);
  if (inTauri()) void tauriCall('audit_clear').catch(() => {});
}
