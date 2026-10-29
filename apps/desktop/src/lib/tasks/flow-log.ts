// The flow log, page side: one line per step of how a chat's turn moves (send, first event, done, a queue that waits,
// a worker waiting for a place…), written beside the Rust side's lines to `logs/flow.log` (src-tauri/src/flow_log.rs).
// The audit log only holds calls that finished and the error log only holds failures: a turn that never ends, a lock
// nobody gives or a queue that waits for a render that never comes leaves nothing in either. Never one line per token.
import { inTauri } from '../app/platform/history';

let pending: string[] = [];
let timer: ReturnType<typeof setTimeout> | undefined;

async function flush() {
  timer = undefined;
  const lines = pending;
  pending = [];
  if (!lines.length || !inTauri()) return;
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('flow_log_append', { lines });
  } catch {
    // The log must never be the thing that fails.
  }
}

/** Notes a step of a chat's flow. Safe anywhere: it never throws and never waits. */
export function logFlow(chat: string, kind: string, detail?: string) {
  if (!inTauri()) return;
  try {
    pending.push(JSON.stringify({ at: new Date().toISOString(), src: 'page', chat, kind, detail: (detail ?? '').slice(0, 400) }));
    if (pending.length >= 64) void flush();
    else timer ??= setTimeout(() => void flush(), 500);
  } catch {
    // See above.
  }
}

// Hang trace: the Settings → Debug switch. On, the page writes its own timings every few seconds and each step it
// takes that is slow; off, it still writes a short heartbeat so a freeze is visible in the log either way.
const HANG_KEY = 'agentdeck.debug.hangTrace';
let hangTrace = (() => {
  try {
    return localStorage.getItem(HANG_KEY) === '1';
  } catch {
    return false;
  }
})();
export const hangTraceOn = () => hangTrace;
export function setHangTrace(on: boolean) {
  hangTrace = on;
  try {
    localStorage.setItem(HANG_KEY, on ? '1' : '0');
  } catch {
    // Browser storage can be blocked: the switch then lasts until the app restarts.
  }
  logFlow('-', 'trace', on ? 'hang trace on' : 'hang trace off');
}

// The page cannot write while its main thread is stuck, so the last step it started is kept here: a watchdog that runs
// after the freeze names where it was. Set by the steps that do the heavy work.
let lastStep = { name: 'page start', at: Date.now() };
/** Marks the step the page is on. Cheap: a freeze is then pinned to it in the log. */
export function markStep(name: string) {
  lastStep = { name, at: Date.now() };
}
const stepNote = () => `last step "${lastStep.name}" started ${Math.round((Date.now() - lastStep.at) / 100) / 10} s ago`;

// The page stopping answering is the other half of "the app froze": a turn can be fine while the page cannot draw it.
// A timer that fires late means the main thread was busy that long; a long task says how long one piece of work held it.
if (typeof window !== 'undefined' && inTauri()) {
  const TICK = 1000;
  let expected = Date.now() + TICK;
  setInterval(() => {
    const late = Date.now() - expected;
    expected = Date.now() + TICK;
    // A hidden or minimised window gets its timers slowed down: that is not a freeze.
    if (late > 2000 && document.visibilityState === 'visible') logFlow('-', 'ui blocked', `the page did not run for ${Math.round(late / 100) / 10} s; ${stepNote()}`);
  }, TICK);
  try {
    new PerformanceObserver((list) => {
      for (const entry of list.getEntries()) {
        if (entry.duration >= 500 && document.visibilityState === 'visible') logFlow('-', 'long task', `${Math.round(entry.duration)} ms; ${stepNote()}`);
      }
    }).observe({ entryTypes: ['longtask'] });
  } catch {
    // Not every webview reports long tasks.
  }
  // Page vitals every few seconds (20 s when the trace is off): alive, how big the page is, the memory it holds and the
  // step it is on. The Rust side notes when these stop coming.
  const vitals = () => {
    const memory = (performance as Performance & { memory?: { usedJSHeapSize: number } }).memory;
    const mb = memory ? `${Math.round(memory.usedJSHeapSize / 1048576)} MB` : 'n/a';
    const nodes = document.getElementsByTagName('*').length;
    logFlow('-', 'page alive', `dom=${nodes} heap=${mb} queued=${pendingCount()} visible=${document.visibilityState} ${stepNote()}`);
  };
  const beat = () => {
    vitals();
    setTimeout(beat, hangTrace ? 5_000 : 20_000);
  };
  setTimeout(beat, hangTrace ? 5_000 : 20_000);
  document.addEventListener('visibilitychange', () => logFlow('-', 'window', document.visibilityState));
  window.addEventListener('pagehide', () => logFlow('-', 'page unloading', stepNote()));
  logFlow('-', 'page started', navigator.userAgent.slice(0, 120));
}

/** Lines waiting to be written (for the vitals). */
function pendingCount() {
  return pending.length;
}

/** Saves one file for a bug report where the user picks: the error log and the whole debug log. Answers the path, or null when cancelled. */
export async function saveDebugBundle(): Promise<string | null> {
  await flush();
  const { invoke } = await import('@tauri-apps/api/core');
  const stamp = new Date().toISOString().slice(0, 16).replace(/[:T]/g, '-');
  return invoke<string | null>('debug_bundle_save', { name: `agentdeck-debug-${stamp}.log` });
}
