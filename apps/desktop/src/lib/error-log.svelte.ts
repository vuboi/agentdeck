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

/** Entries of this run of the app, newest last, for the drawer to show at once (the file holds the older ones). */
export const errorLog = $state<{ entries: LogEntry[]; loaded: boolean; count: number }>({ entries: [], loaded: false, count: 0 });

const SHOWN = 500;
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
    errorLog.entries.push(entry);
    errorLog.count++;
    if (errorLog.entries.length > SHOWN) errorLog.entries.splice(0, errorLog.entries.length - SHOWN);
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

/** Reads the log from disk into the drawer (the end of it), parsed. */
export async function loadErrorLog() {
  if (!inTauri()) {
    errorLog.loaded = true;
    return;
  }
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    await flush();
    const text = await invoke<string>('error_log_read');
    const entries: LogEntry[] = [];
    for (const line of text.split('\n')) {
      if (!line.trim()) continue;
      try {
        entries.push(JSON.parse(line) as LogEntry);
      } catch {
        entries.push({ at: '', area: 'app', event: 'unreadable line', detail: line.slice(0, 200) });
      }
    }
    errorLog.entries = entries.slice(-SHOWN);
    errorLog.count = entries.length;
  } catch (error) {
    errorLog.entries = [{ at: new Date().toISOString(), area: 'app', event: 'the log could not be read', detail: String(error) }];
  } finally {
    errorLog.loaded = true;
  }
}

/** Saves the whole log where the user picks. Answers the path, or null when cancelled. */
export async function saveErrorLog(): Promise<string | null> {
  const { invoke } = await import('@tauri-apps/api/core');
  await flush();
  const stamp = new Date().toISOString().slice(0, 16).replace(/[:T]/g, '-');
  return invoke<string | null>('error_log_save', { name: `agentdeck-errors-${stamp}.log` });
}

export async function clearErrorLog() {
  const { invoke } = await import('@tauri-apps/api/core');
  pending = [];
  await invoke('error_log_clear');
  errorLog.entries = [];
  errorLog.count = 0;
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
