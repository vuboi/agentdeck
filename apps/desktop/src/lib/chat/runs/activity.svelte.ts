// What the agents did, across every chat: each tool call with its status, newest last. The Tasks panel shows
// the ones still running (and those sent to the background) and, below them, the whole log, kept on disk
// in `~/.agentdeck/audit.jsonl` so it survives a restart.

import { pictureUrl, type AgentEvent } from '../agent';
import { inTauri } from '../../app/platform/history';
import { planRunRestore, terminalStep, updateInflight, loadInflight, withTerminal, withoutTerminal, type InflightRun, type ServerLike } from './inflight';
import { sessionTail, watchOutput } from '../../terminal/terminal-session';
import { endedByTask, statusAfterResult } from './activity-status';

export type ActivityStatus = 'running' | 'background' | 'ok' | 'fail' | 'stopped';
export type Activity = {
  key: string;
  /** The tool id the CLI gave this call. Ids restart every turn, so the key adds a counter. */
  callId?: string;
  sessionId: string;
  /** Provider turn owning this entry; concurrent replies must not settle each other's tools. */
  turn?: string;
  /** Stable provider task identity, independent of the call that launched it. */
  nativeTaskId?: string;
  taskLabel?: string;
  sourceMessageId?: string;
  sourceTitle?: string;
  title: string;
  /** The project the chat belongs to, by name ('' when none). Old entries have none. */
  project?: string;
  name: string;
  input: string;
  background: boolean;
  /** Whether the call itself was started as background work (a shell with run_in_background, an agent). A task event
   *  later sets `background` too, but a foreground call with a task event still ends when its result comes. */
  launchedInBackground?: boolean;
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
  /** A Request: the agent it went to (its id, as AgentLogo takes it) and the model. */
  agent?: string;
  model?: string | null;
};

/** A saved picture, read back as a data URL; one read per picture. */
const pictures = new Map<string, Promise<string>>();
/** Most pictures kept as data URLs at once; the oldest is read again when it is shown again. */
const PICTURE_CACHE = 48;
function rememberPicture(name: string, found: Promise<string>) {
  pictures.delete(name);
  pictures.set(name, found);
  while (pictures.size > PICTURE_CACHE) pictures.delete(pictures.keys().next().value!);
}
export function auditImage(image: string): Promise<string> {
  if (image.startsWith('data:')) return Promise.resolve(image);
  let found = pictures.get(image);
  if (!found) {
    // The file's own URL (asset protocol): the browser reads it when drawn, so no base64 crosses IPC.
    found = tauriCall('audit_image_path', { name: image }).then((path) => pictureUrl(String(path)));
    found.catch(() => pictures.delete(image));
    rememberPicture(image, found);
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

/** Live and finished calls plus chat errors, oldest first: live rows enter at start and persist on completion. */
export const audit = $state<Activity[]>([]);
export const auditPaging = $state({ cursor: null as number | null, hasMore: false, loading: false });
let auditPageGeneration = 0;

async function tauriCall(command: string, args?: Record<string, unknown>) {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke(command, args);
}

/** The local time zone as JS reports it (minutes, positive west of UTC): the Audit index cuts its days by it. */
const tzOffset = () => new Date().getTimezoneOffset();

/** The log keeps two days: today and yesterday. At midnight yesterday's day before goes, by itself. */
const startOfYesterday = () => {
  const day = new Date();
  day.setHours(0, 0, 0, 0);
  day.setDate(day.getDate() - 1);
  return day.getTime();
};
function pruneAudit(): Promise<number> {
  const cutoff = startOfYesterday();
  for (let index = audit.length - 1; index >= 0; index--) if (audit[index].startedAt < cutoff) audit.splice(index, 1);
  return tauriCall('audit_prune', { cutoff }).then(Number).catch(() => 0);
}

type AuditPage = { entries: { offset: number; line: string }[]; cursor: number | null; hasMore: boolean };

async function readAuditPage(limit: number, cursor: number | null) {
  if (!inTauri() || auditPaging.loading) return;
  const generation = auditPageGeneration;
  auditPaging.loading = true;
  try {
    const page = (await tauriCall('audit_read', { cursor, limit })) as AuditPage;
    if (generation !== auditPageGeneration) return;
    const loaded: Activity[] = [];
    for (const { offset, line } of page.entries) {
      try {
        const entry = JSON.parse(line) as Activity;
        loaded.push({ ...entry, key: `${entry.key}#${offset}` });
      } catch {
        // A corrupt line is skipped; the Rust cursor still advances past it.
      }
    }
    audit.unshift(...loaded);
    auditPaging.cursor = page.cursor;
    auditPaging.hasMore = page.hasMore;
  } catch {
    // Keep the current page visible if disk access fails; a later scroll can retry.
  } finally {
    if (generation === auditPageGeneration) auditPaging.loading = false;
  }
}

function resetAuditPages() {
  auditPageGeneration++;
  auditPaging.cursor = null;
  auditPaging.hasMore = false;
  auditPaging.loading = false;
  for (let index = audit.length - 1; index >= 0; index--) {
    if (/#\d+$/.test(audit[index].key)) audit.splice(index, 1);
  }
  void readAuditPage(100, null);
}

export function loadMoreAudit() {
  if (!auditPaging.hasMore || auditPaging.loading) return;
  void readAuditPage(50, auditPaging.cursor);
}

/** The full count of each day the log holds (all calls and failed ones), known before the pages are read. */
export const auditDays = $state<{ day: number; total: number; failed: number }[]>([]);
let auditDaysRequest = 0;
export async function refreshAuditDays() {
  if (!inTauri()) return;
  const request = ++auditDaysRequest;
  try {
    const days = (await tauriCall('audit_days', { offsetMin: tzOffset() })) as { day: number; total: number; failed: number }[];
    if (request === auditDaysRequest) auditDays.splice(0, auditDays.length, ...days);
  } catch {
    // Keep the last counts; the list still shows what is loaded.
  }
}

/** The local day number of a time (ms since the epoch), the same day cut the log's index uses. */
export const dayNumber = (ms: number) => Math.floor((ms - tzOffset() * 60_000) / 86_400_000);

if (inTauri()) {
  // Checked every hour too, so an app left open over midnight drops the old day as well.
  setInterval(() => void pruneAudit().then((dropped) => dropped > 0 && resetAuditPages()), 3_600_000);
  void pruneAudit()
    .then(() => readAuditPage(100, null))
    .catch(() => {});
}

/** A finished call goes to the Audit log and to disk. */
function record(item: Activity) {
  // Tool ids restart in every turn (Codex counts from item_0 each time), so a saved entry gets its own key.
  const entry = { ...item, key: `${item.key}@${item.startedAt}`, output: item.output?.slice(0, OUTPUT_CAP) };
  const liveIndex = audit.findIndex((saved) => saved.key === item.key);
  if (liveIndex >= 0) audit.splice(liveIndex, 1, entry);
  else audit.push(entry);
  if (!inTauri()) return;
  const picture = entry.image?.startsWith('data:') ? entry.image : undefined;
  if (!picture) {
    void tauriCall('audit_append', { line: JSON.stringify(entry), offsetMin: tzOffset() }).catch(() => {});
    return;
  }
  // The picture goes to a file of its own; the log line, and the entry once saved, keep its name.
  void tauriCall('audit_image_save', { image: picture })
    .then((name) => {
      const saved = String(name);
      rememberPicture(saved, Promise.resolve(picture));
      const shown = audit.find((item) => item.key === entry.key);
      if (shown) shown.image = saved;
      return tauriCall('audit_append', { line: JSON.stringify({ ...entry, image: saved }), offsetMin: tzOffset() });
    })
    .catch(() => void tauriCall('audit_append', { line: JSON.stringify({ ...entry, image: undefined }), offsetMin: tzOffset() }).catch(() => {}));
}

/** The message a chat sent and is waiting on: one entry from the send to the end of the reply, so the log shows who
 *  answered, how long it took, and what went wrong (a limit, a time out, a failed start). */
const requests = new Map<string, { item: Activity; firstAt?: number }>();
/**
 * The user stopped this chat: nothing of it keeps running. Items whose turn ends with no matching event (a background
 * tool of a reply that already closed, or a turn id that never reached the handler) would otherwise stay "running" for good.
 */
export function closeRunningActivity(sessionId: string) {
  for (const [key, request] of requests) {
    if (request.item.sessionId !== sessionId) continue;
    requests.delete(key);
    // The request's own log line is closed too, not only dropped: it would stay "running" in the audit.
    if (request.item.status === 'running' || request.item.status === 'background') {
      request.item.status = 'stopped';
      request.item.endedAt = Date.now();
      record(request.item);
    }
  }
  for (const item of activity) {
    if (item.sessionId !== sessionId || (item.status !== 'running' && item.status !== 'background')) continue;
    item.status = 'stopped';
    item.endedAt = Date.now();
    record(item);
  }
}

/** Separates foreground/background reply lifetimes within the same visible chat. */
const runKey = (sessionId: string, turn?: string) => turn ? `${sessionId}:${turn}` : sessionId;

const seconds = (ms: number) => (ms < 10_000 ? `${(ms / 1000).toFixed(1)} s` : `${Math.round(ms / 1000)} s`);

/** A message was sent to an agent: its entry runs until the reply ends. `detail` is what the turn was started with. */
export function logRequest(sessionId: string, title: string, project: string, agent: string, model: string | null, detail: string, agentId?: string, turn?: string) {
  endRequest(sessionId, 'stopped', 'A new message was sent before this reply ended.', turn);
  const item: Activity = { key: `${sessionId}:request:${++calls}`, sessionId, turn, title, project, name: 'Request', input: `${agent} · ${model ?? 'default model'}`, agent: agentId, model, background: false, status: 'running', startedAt: Date.now(), output: detail };
  requests.set(runKey(sessionId, turn), { item });
  audit.push(item);
}

/** The reply began: how long the agent took to start answering. */
function noteFirstReply(sessionId: string, turn?: string) {
  const request = requests.get(runKey(sessionId, turn));
  if (!request || request.firstAt) return;
  request.firstAt = Date.now();
  request.item.output = `${request.item.output ?? ''}
First reply after ${seconds(request.firstAt - request.item.startedAt)}.`;
}

/** The reply ended (or failed): the entry is final, with how long it took and the error if there was one. */
function endRequest(sessionId: string, status: 'ok' | 'fail' | 'stopped', note: string, turn?: string) {
  const request = requests.get(runKey(sessionId, turn));
  if (!request) return;
  requests.delete(runKey(sessionId, turn));
  const { item } = request;
  item.status = status;
  item.endedAt = Date.now();
  item.output = `${item.output ?? ''}
${status === 'ok' ? 'Answered' : status === 'fail' ? 'Failed' : 'Ended'} after ${seconds(item.endedAt - item.startedAt)}${request.firstAt ? '' : ', with no reply'}.${note ? `

${note}` : ''}`;
  record(item);
}

/** The reasoning of a chat's run so far: it streams in pieces and is one log entry, written when something else happens. */
const thoughts = new Map<string, { text: string; at: number; title: string; project: string }>();

function flushThinking(sessionId: string, turn?: string) {
  const thought = thoughts.get(runKey(sessionId, turn));
  if (!thought) return;
  thoughts.delete(runKey(sessionId, turn));
  const text = thought.text.trim();
  if (!text) return;
  record({ key: `${sessionId}:thinking:${++calls}`, sessionId, title: thought.title, project: thought.project, name: 'Thinking', input: text.replace(/\s+/g, ' ').slice(0, 200), background: false, status: 'ok', startedAt: thought.at, endedAt: Date.now(), output: text });
}

/** Keeps the log in step with one event of a chat's run. */
export function trackActivity(sessionId: string, title: string, project: string, event: AgentEvent, turn?: string, context?: { agent: string; model?: string; messageId?: string; request?: string }) {
  if (event.kind === 'scoped') {
    const parent = activity.findLast((item) => item.sessionId === sessionId && item.turn === turn && item.callId === event.parent);
    if (!parent) return;
    if (event.event.kind === 'delta') parent.output = `${parent.output ?? ''}${event.event.text}`.slice(-OUTPUT_CAP);
    if (event.event.kind === 'tool_call') parent.output = `${parent.output ?? ''}\nRunning ${event.event.name}`.trim().slice(-OUTPUT_CAP);
    return;
  }
  if (event.kind === 'thinking' || event.kind === 'delta' || event.kind === 'tool_call') noteFirstReply(sessionId, turn);
  if (event.kind === 'thinking') {
    const thought = thoughts.get(runKey(sessionId, turn)) ?? { text: '', at: Date.now(), title, project };
    thought.text += event.text;
    thoughts.set(runKey(sessionId, turn), thought);
    return;
  }
  // Anything else ends the reasoning that came before it (a tool, the answer, the end of the run).
  if (event.kind !== 'usage') flushThinking(sessionId, turn);
  if (event.kind === 'raw') {
    // A step of the CLI the app does not turn into anything: its line as the CLI wrote it, to copy when debugging.
    const now = Date.now();
    record({ key: `${sessionId}:raw:${++calls}`, sessionId, title, project, name: `${event.source} · ${event.label}`, input: event.line.slice(0, 200), background: false, status: 'ok', startedAt: now, endedAt: now, output: event.line });
  } else if (event.kind === 'tool_call') {
    const item: Activity = {
      key: `${sessionId}:${event.id}:${++calls}`,
      callId: event.id,
      sessionId,
      turn,
      agent: context?.agent,
      model: context?.model,
      sourceMessageId: context?.messageId,
      sourceTitle: context?.request,
      title,
      project,
      name: event.name,
      input: event.input,
      background: event.background,
      launchedInBackground: event.background,
      status: 'running',
      startedAt: Date.now(),
    };
    activity.push(item);
    // The Audit view shares this live entry; `record` replaces it with the final snapshot on completion.
    audit.push(item);
    // Active background work must not disappear when newer tools fill the log.
    if (activity.length > LIMIT) {
      const finished = activity.findIndex((entry) => entry.status !== 'running' && entry.status !== 'background');
      if (finished >= 0) activity.splice(finished, 1);
    }
  } else if (event.kind === 'tool_output_delta') {
    const same = (entry: Activity) => entry.sessionId === sessionId && entry.turn === turn && entry.callId === event.id;
    const item = activity.findLast((entry) => same(entry) && (entry.status === 'running' || entry.status === 'background'));
    if (item) item.output = `${item.output ?? ''}${event.delta}`.slice(-OUTPUT_CAP);
  } else if (event.kind === 'tool_result') {
    // The call still open with this id, else the latest one with it.
    const same = (entry: Activity) => entry.sessionId === sessionId && entry.turn === turn && entry.callId === event.id;
    const item = activity.findLast((entry) => same(entry) && (entry.status === 'running' || entry.status === 'background')) ?? activity.findLast(same);
    if (!item) return;
    item.output = event.output.length > 4000 ? `${event.output.slice(0, 4000)}…` : event.output;
    if (event.image) item.image = event.image;
    // A call its task event already ended was recorded then: the result adds output and keeps that status.
    const ended = endedByTask(item);
    item.status = statusAfterResult({ ...item, background: item.launchedInBackground ?? item.background }, event.is_error);
    if (item.status !== 'background' && !ended) {
      item.endedAt = Date.now();
      record(item);
      // The log keeps the picture; the live list does not need a copy of it.
      item.image = undefined;
    }
    if (item.status === 'ok' && (item.name === 'Write' || item.name === 'Edit' || item.name === 'Delete' || item.name === 'MultiEdit')) fileWrites.tick++;
  } else if (event.kind === 'task') {
    const callId = event.tool_use_id ?? event.id;
    const sameTask = (entry: Activity) => entry.sessionId === sessionId && entry.nativeTaskId === event.id;
    const matchingCalls = activity.filter((entry) => entry.sessionId === sessionId && entry.callId === callId && (!entry.nativeTaskId || entry.nativeTaskId === event.id));
    let item = activity.findLast(sameTask) ?? matchingCalls.findLast((entry) => entry.turn === turn) ?? (matchingCalls.length === 1 ? matchingCalls[0] : undefined);
    if (!item) {
      item = { key: `${sessionId}:task:${event.id}`, sessionId, turn, callId, title, project, name: 'Agent', input: event.label ?? 'Background task', background: true, status: 'background', startedAt: Date.now(), agent: context?.agent, model: context?.model, sourceMessageId: context?.messageId, sourceTitle: context?.request };
      activity.push(item);
      item = activity[activity.length - 1];
    }
    if (item.endedAt && item.nativeTaskId === event.id) return;
    // A synchronous tool result may have preceded the provider's task-start notification.
    if ((event.status === 'started' || event.status === 'progress') && !item.nativeTaskId) item.endedAt = undefined;
    item.nativeTaskId = event.id;
    item.taskLabel = event.label ?? item.taskLabel;
    item.background = true;
    if (event.status === 'started' || event.status === 'progress') {
      item.status = 'background';
      if (event.summary) item.output = event.summary;
      return;
    }
    item.status = event.status === 'failed' ? 'fail' : 'ok';
    item.output = event.summary ?? item.output;
    item.endedAt = Date.now();
    record(item);
  } else if (event.kind === 'done' || event.kind === 'error') {
    const asked = requests.has(runKey(sessionId, turn));
    endRequest(sessionId, event.kind === 'done' ? 'ok' : event.message === 'stopped' ? 'stopped' : 'fail', event.kind === 'error' && event.message !== 'stopped' ? event.message : '', turn);
    // The failure is on the request's entry: a second "Chat error" for the same thing would only repeat it.
    if (event.kind === 'error' && event.message !== 'stopped' && !asked) {
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
    // A successful answer does not end provider background work. Only its own task event proves completion.
    for (const item of activity) {
      if (item.sessionId !== sessionId || (item.status !== 'running' && item.status !== 'background')) continue;
      if (item.turn !== turn || (event.kind === 'done' && item.background)) continue;
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
 * Keeps a Terminal entry's `output` in step with what its tab prints: the terminal pushes the tail whenever the shell
 * writes (throttled there), nothing is read on a timer. Returns the function that stops it.
 */
export function watchTerminalOutput(item: Activity): () => void {
  if (!item.terminalId) return () => {};
  return watchOutput(item.terminalId, (text) => {
    // A finished entry keeps what it ended with: the tab's next command is another entry.
    if (item.status !== 'running') return;
    item.output = text.slice(-OUTPUT_CAP);
  });
}

// ---- The Rust side tells when a server's log grew (`server-log`, {port}), so a card reads it again at once.
const logWatchers = new Map<number, Set<() => void>>();
let listeningToLogs: Promise<void> | null = null;

/** Calls `changed` whenever the log of the server on `port` grows. Returns the function that stops it. */
export function watchServerLog(port: number, changed: () => void): () => void {
  if (!inTauri()) return () => {};
  listeningToLogs ??= import('../../app/platform/tauri-event').then(async ({ listen }) => {
    await listen<{ port: number }>('server-log', ({ payload }) => {
      for (const watcher of logWatchers.get(payload.port) ?? []) watcher();
    });
  });
  void listeningToLogs.catch(() => (listeningToLogs = null));
  const set = logWatchers.get(port) ?? new Set();
  set.add(changed);
  logWatchers.set(port, set);
  return () => {
    set.delete(changed);
    if (set.size === 0 && logWatchers.get(port) === set) logWatchers.delete(port);
  };
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

/**
 * An image generation run, which goes on in the background while its panel is closed. It shows as running in the
 * Tasks panel and, once it ends, is written to the Audit log. Returns the function that ends it.
 */
export function trackImageRun(project: string, prompt: string, models: string) {
  activity.push({ key: `image:${++calls}`, sessionId: '', title: 'Image generator', project, name: 'Image generation', input: `${models}\n${prompt}`.slice(0, 200), background: true, status: 'running', startedAt: Date.now() });
  const item = activity[activity.length - 1];
  return (status: 'ok' | 'fail' | 'stopped', output: string) => {
    item.status = status;
    item.output = output.slice(0, OUTPUT_CAP);
    item.endedAt = Date.now();
    record(item);
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
  auditPageGeneration++;
  auditPaging.cursor = null;
  auditPaging.hasMore = false;
  auditPaging.loading = false;
  if (inTauri()) void tauriCall('audit_clear').catch(() => {});
}
