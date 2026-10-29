// The error log: what went wrong while the app worked, with enough around it to trace a hang or two agents that could
// not reach each other. One JSON line per entry, kept on disk by Rust (src-tauri/src/error_log.rs) and shown in the
// Tasks view's log drawer, which also saves the file. A line holds no message text of the user's: only where it
// happened (chat, step, agent, model), what kind, and the error as the CLI or the app worded it.
import { inTauri } from './history';

export type LogArea = 'turn' | 'flow' | 'team' | 'delegate' | 'handback' | 'mcp' | 'provider' | 'app';
export type LogEntry = {
  at: string;
  area: LogArea;
  /** What happened, in a few words ("turn failed", "member stopped", "no answer in 5 min"). */
  event: string;
  /** The error as it was worded. */
  detail?: string;
  /** Where: chat id, agent, model, step, run id… */
  ctx?: Record<string, string | number | boolean | undefined>;
};
type DisplayLogEntry = LogEntry & { _rowId?: string };

/** Entries of this run of the app, newest last, for the drawer to show at once (the file holds the older ones). */
export type ErrorLogCursor = { file: 'current' | 'rotated'; offset: number };
export const errorLog = $state<{
  entries: DisplayLogEntry[];
  loaded: boolean;
  count: number;
  cursor: ErrorLogCursor | null;
  hasMore: boolean;
  loading: boolean;
  areas: LogArea[];
}>({ entries: [], loaded: false, count: 0, cursor: null, hasMore: false, loading: false, areas: [] });

let pageGeneration = 0;
let liveEntryId = 0;
let pending: string[] = [];
let timer: ReturnType<typeof setTimeout> | undefined;
// The same error repeating (a retry loop) is written once a minute with a count, not hundreds of times.
const recent = new Map<string, { at: number; skipped: number }>();

async function flush() {
  timer = undefined;
  if (!pending.length || !inTauri()) {
    pending = [];
    return;
  }
  const lines = pending;
  pending = [];
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('error_log_append', { lines });
  } catch {
    // The log must never be the thing that fails.
  }
}

/** Records a problem. Safe to call anywhere: it never throws and never waits. */
export function logError(area: LogArea, event: string, detail?: unknown, ctx?: LogEntry['ctx']) {
  try {
    const text = detail === undefined ? undefined : detail instanceof Error ? `${detail.name}: ${detail.message}` : String(detail).slice(0, 1500);
    const key = `${area}|${event}|${text?.slice(0, 120) ?? ''}|${ctx?.chat ?? ''}`;
    const now = Date.now();
    const seen = recent.get(key);
    if (seen && now - seen.at < 60_000) {
      seen.skipped++;
      return;
    }
    const entry: LogEntry = { at: new Date(now).toISOString(), area, event, ...(text ? { detail: text } : {}), ...(ctx ? { ctx: { ...ctx, ...(seen?.skipped ? { repeated: seen.skipped } : {}) } } : seen?.skipped ? { ctx: { repeated: seen.skipped } } : {}) };
    recent.set(key, { at: now, skipped: 0 });
    if (recent.size > 200) recent.delete(recent.keys().next().value as string);
    errorLog.entries.push({ ...entry, _rowId: `live:${++liveEntryId}` });
    errorLog.count++;
    pending.push(JSON.stringify(entry));
    timer ??= setTimeout(() => void flush(), 800);
  } catch {
    // See above.
  }
}

/** Sample entries for the drawer in dev mode when the log is empty, to see how it reads (never written to the file). */
export function demoEntries(): LogEntry[] {
  const at = (minutes: number) => new Date(Date.now() - minutes * 60_000).toISOString();
  const chat = { chat: 'claude:demo-1', agent: 'claude', model: 'claude-opus-5-5' };
  return [
    { at: at(2), area: 'turn', event: 'turn failed', detail: 'You have hit your usage limit. Try again at 12pm (Asia/Bangkok).', ctx: chat },
    { at: at(7), area: 'team', event: 'Backend: 1 message(s) to an unknown member or to itself', ctx: { run: 'a1b2c3', member: 'Backend', known: 'Frontend,QA,DevOps' } },
    { at: at(11), area: 'team', event: 'QA: answer without a STATUS line', detail: 'treated as working', ctx: { run: 'a1b2c3', member: 'QA' } },
    { at: at(19), area: 'flow', event: 'step "Review" failed', detail: 'no answer', ctx: { run: 'd4e5f6', chat: 'claude:demo-2', agent: 'codex', model: 'gpt-6-luna' } },
    { at: at(34), area: 'turn', event: 'no event for 6 min', detail: 'last: tool_call Bash', ctx: { chat: 'claude:demo-3', agent: 'codex', model: 'gpt-6-luna' } },
    { at: at(41), area: 'mcp', event: 'tool failed', detail: 'browser_screenshot: no page is open in the Preview', ctx: chat },
    { at: at(58), area: 'handback', event: 'the answers never reached the chat (it stayed busy for 15 min)', ctx: { chat: 'claude:demo-1', run: 'd4e5f6' } },
    { at: at(75), area: 'provider', event: 'the provider did not answer', detail: 'HTTP 429: rate limit reached for Groq (llama-3.3-70b)', ctx: { chat: 'claude:demo-4', agent: 'custom', model: 'groq::llama-3.3-70b' } },
    { at: at(120), area: 'app', event: 'unhandled promise rejection', detail: 'TypeError: Cannot read properties of undefined (reading "id")', ctx: { at: 'sessions.svelte.ts:2144' } },
  ];
}

type ErrorLogPage = { entries: { file: 'current' | 'rotated'; offset: number; line: string }[]; cursor: ErrorLogCursor | null; hasMore: boolean; count: number | null; areas: LogArea[] | null };

/** Reads one bounded page. Older pages are prepended so the drawer can keep scrolling toward history. */
async function readErrorLogPage(cursor: ErrorLogCursor | null, reset = false) {
  if (errorLog.loading || (!reset && errorLog.loaded && cursor === null)) return;
  const generation = pageGeneration;
  errorLog.loading = true;
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    await flush();
    const page = await invoke<ErrorLogPage>('error_log_read', { cursor, limit: 50 });
    if (generation !== pageGeneration) return;
    const entries: DisplayLogEntry[] = [];
    for (const item of page.entries) {
      try {
        entries.push({ ...(JSON.parse(item.line) as LogEntry), _rowId: `${item.file}:${item.offset}` });
      } catch {
        entries.push({ at: '', area: 'app', event: 'unreadable line', detail: item.line.slice(0, 200), _rowId: `${item.file}:${item.offset}` });
      }
    }
    if (reset) errorLog.entries = entries;
    else errorLog.entries.unshift(...entries);
    if (page.count !== null) errorLog.count = page.count;
    if (page.areas) errorLog.areas = page.areas;
    errorLog.cursor = page.cursor;
    errorLog.hasMore = page.hasMore;
    errorLog.loaded = true;
  } catch (error) {
    if (generation === pageGeneration) {
      if (reset || !errorLog.loaded) errorLog.entries = [{ at: new Date().toISOString(), area: 'app', event: 'the log could not be read', detail: String(error) }];
      errorLog.loaded = true;
      errorLog.hasMore = false;
    }
  } finally {
    if (generation === pageGeneration) errorLog.loading = false;
  }
}

/** Reads the newest page on first open; already loaded pages remain in place on later opens. */
export async function loadErrorLog() {
  if (!inTauri()) {
    errorLog.loaded = true;
    return;
  }
  await readErrorLogPage(null, !errorLog.loaded);
}

/** Fetches the next older page when the drawer scroll reaches its end. */
export function loadMoreErrorLog() {
  if (!errorLog.hasMore || errorLog.loading || !errorLog.cursor) return;
  void readErrorLogPage(errorLog.cursor);
}

/** Saves the whole log where the user picks. Answers the path, or null when cancelled. */
export async function saveErrorLog(): Promise<string | null> {
  const { invoke } = await import('@tauri-apps/api/core');
  await flush();
  const stamp = new Date().toISOString().slice(0, 16).replace(/[:T]/g, '-');
  return invoke<string | null>('error_log_save', { name: `agentdeck-errors-${stamp}.log` });
}

export async function clearErrorLog() {
  pageGeneration++;
  errorLog.loading = false;
  const { invoke } = await import('@tauri-apps/api/core');
  pending = [];
  await invoke('error_log_clear');
  errorLog.entries = [];
  errorLog.count = 0;
  errorLog.cursor = null;
  errorLog.hasMore = false;
  errorLog.areas = [];
  errorLog.loaded = true;
}

// The count on the Tasks view's button is right from the start: the log of earlier runs is read once the app is up.
if (typeof window !== 'undefined' && inTauri()) setTimeout(() => void loadErrorLog(), 3000);

// Whatever the page throws and nobody catches, and promises nobody handled: the "frozen" cases start here.
if (typeof window !== 'undefined') {
  window.addEventListener('error', (event) => {
    // A resize observer notice is not an error.
    if (/ResizeObserver loop/.test(event.message)) return;
    logError('app', 'uncaught error', event.error ?? event.message, { at: `${(event.filename ?? '').split('/').pop()}:${event.lineno}` });
  });
  window.addEventListener('unhandledrejection', (event) => {
    const reason = event.reason;
    // Tauri's listen() without the shell (a plain browser) is not a problem of the app.
    if (/transformCallback/.test(String(reason))) return;
    logError('app', 'unhandled promise rejection', reason instanceof Error ? reason : String(reason));
  });
}
