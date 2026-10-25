// The live side of the Terminal panel: one xterm.js terminal per tab, each wired to a shell of the Rust side
// (src-tauri/src/pty.rs). The tab list and its saved state live in terminal.svelte.ts; this module owns what cannot
// be saved: the xterm objects and the shells. A panel that is hidden keeps its shells running, so the terminals
// outlive the component that shows them and are moved into a new container when it comes back.

import { Terminal, type ITheme } from '@xterm/xterm';
import '@xterm/xterm/css/xterm.css';
import { FitAddon } from '@xterm/addon-fit';
import { SearchAddon } from '@xterm/addon-search';
import { WebLinksAddon } from '@xterm/addon-web-links';
import { WebglAddon } from '@xterm/addon-webgl';
import { inTauri } from './history';
import { stripLocationPrefix } from './inflight';

export type SessionStatus = 'starting' | 'running' | 'exited';

/** What a terminal tells the tab that shows it. */
export type SessionHooks = {
  title: (name: string) => void;
  status: (status: SessionStatus) => void;
  /** Ctrl+F in the terminal. */
  find: () => void;
  /** Where the view is: at the top of the scrollback, at the bottom (for the scroll arrows). */
  scrolled?: (top: boolean, bottom: boolean) => void;
};

type Live = {
  term: Terminal;
  fit: FitAddon;
  search: SearchAddon;
  cwd?: string;
  hooks: SessionHooks;
  exited: boolean;
  /** The foreground command being watched (see `evaluate`), the address it printed, and the timer of the next look. */
  run?: { runId: number; command: string; cwd?: string; startedAt: number; line: number };
  url: string | null;
  timer: number;
  /** The shell is being adopted after a reload: its kept output is written first, what arrives meanwhile waits. */
  replaying: boolean;
  held: { bytes: Uint8Array; end: number }[];
  /** The start (or adoption) in progress or done. */
  ready?: Promise<unknown>;
  /** The shell is up (started or adopted) and not ended: only then is typed text sent to it. */
  up: boolean;
  /** The next fit, waiting for the panel to stop sliding (0: none). */
  fitFrame: number;
  /** Who wants the tail of the output as it grows (the Tasks panel), and the timer of the next push. */
  watchers: Set<(text: string) => void>;
  pushTimer: number;
  /** Tells the tab's scroll arrows where the view is. */
  reportScroll: () => void;
};

/** A command a tab is still running (a dev server, a watcher…): what the "running in the background" list shows. */
export type RunInfo = { runId: number; command: string; cwd?: string; startedAt: number; url: string | null };
let runWatcher: ((id: string, run: RunInfo | null) => void) | null = null;
/** Tells `watch` every time a tab starts, updates or ends a long-running command. */
export const watchRuns = (watch: (id: string, run: RunInfo | null) => void) => {
  runWatcher = watch;
};

const live = new Map<string, Live>();

export const terminalAvailable = () => inTauri();

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<T>(command, args);
}

// ---- Colours: the app's tokens (oklch, color-mix…) as the plain colours xterm takes.

let probe: CanvasRenderingContext2D | null = null;
function toHex(css: string): string {
  probe ??= document.createElement('canvas').getContext('2d', { willReadFrequently: true });
  if (!probe) return '#000000';
  probe.clearRect(0, 0, 1, 1);
  probe.fillStyle = '#000';
  probe.fillStyle = css;
  probe.fillRect(0, 0, 1, 1);
  const [r, g, b, a] = probe.getImageData(0, 0, 1, 1).data;
  const hex = (value: number) => value.toString(16).padStart(2, '0');
  return `#${hex(r)}${hex(g)}${hex(b)}${a === 255 ? '' : hex(a)}`;
}

/** The colour a CSS expression (`var(--primary)`, `color-mix(…)`) has on the page. */
function resolve(expression: string): string {
  const element = document.createElement('span');
  element.style.color = expression;
  document.body.append(element);
  const computed = getComputedStyle(element).color;
  element.remove();
  return toHex(computed);
}

const DARK = ['#3b3b3b', '#f14c4c', '#23d18b', '#f5f543', '#3b8eea', '#d670d6', '#29b8db', '#e5e5e5', '#666666', '#f14c4c', '#23d18b', '#f5f543', '#3b8eea', '#d670d6', '#29b8db', '#ffffff'];
const LIGHT = ['#000000', '#cd3131', '#00bc00', '#949800', '#0451a5', '#bc05bc', '#0598bc', '#555555', '#666666', '#cd3131', '#14ce14', '#b5ba00', '#0451a5', '#bc05bc', '#0598bc', '#a5a5a5'];

function theme(): ITheme {
  const background = resolve('var(--background)');
  const lightness = parseInt(background.slice(1, 3), 16) * 0.3 + parseInt(background.slice(3, 5), 16) * 0.59 + parseInt(background.slice(5, 7), 16) * 0.11;
  const palette = lightness > 140 ? LIGHT : DARK;
  const names = ['black', 'red', 'green', 'yellow', 'blue', 'magenta', 'cyan', 'white'] as const;
  const colours: Record<string, string> = {};
  names.forEach((name, index) => {
    colours[name] = palette[index];
    colours[`bright${name[0].toUpperCase()}${name.slice(1)}`] = palette[index + 8];
  });
  return {
    ...colours,
    background,
    foreground: resolve('var(--foreground)'),
    cursor: resolve('var(--primary)'),
    cursorAccent: background,
    selectionBackground: resolve('color-mix(in oklab, var(--primary) 35%, transparent)'),
    scrollbarSliderBackground: resolve('color-mix(in oklab, var(--muted-foreground) 30%, transparent)'),
    scrollbarSliderHoverBackground: resolve('color-mix(in oklab, var(--muted-foreground) 55%, transparent)'),
    scrollbarSliderActiveBackground: resolve('color-mix(in oklab, var(--muted-foreground) 70%, transparent)'),
  };
}

/** Paints every terminal again with the page's colours, after the theme changed. */
function retheme() {
  const next = theme();
  for (const { term } of live.values()) term.options.theme = next;
}

let watching = false;
function watchTheme() {
  if (watching) return;
  watching = true;
  let frame = 0;
  const schedule = () => {
    cancelAnimationFrame(frame);
    frame = requestAnimationFrame(retheme);
  };
  new MutationObserver(schedule).observe(document.documentElement, { attributes: true, attributeFilter: ['class', 'style', 'data-theme'] });
  window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', schedule);
}

// ---- The shells' output.

let listening: Promise<void> | null = null;
function listenToShells(): Promise<void> {
  listening ??= (async () => {
    const { listen } = await import('@tauri-apps/api/event');
    await listen<{ id: string; data: string; end?: number }>('pty-data', ({ payload }) => {
      const session = live.get(payload.id);
      if (!session) return;
      const bytes = decode(payload.data);
      if (session.replaying) {
        session.held.push({ bytes, end: payload.end ?? 0 });
        return;
      }
      if (session.watchers.size > 0) session.term.write(bytes, () => pushOutput(payload.id, session));
      else session.term.write(bytes);
      if (session.run) schedule(payload.id, session, 800);
    });
    await listen<{ id: string; code: number | null }>('pty-exit', ({ payload }) => {
      const session = live.get(payload.id);
      if (!session) return;
      session.exited = true;
      session.up = false;
      endRun(payload.id, session);
      session.hooks.status('exited');
      const how = payload.code === null || payload.code === 0 ? 'exited' : `exited with code ${payload.code}`;
      session.term.write(`\r\n\x1b[2m[shell ${how}. Press Enter to start a new one.]\x1b[0m\r\n`);
    });
  })();
  return listening;
}

function decode(data: string) {
  const text = atob(data);
  const bytes = new Uint8Array(text.length);
  for (let index = 0; index < text.length; index++) bytes[index] = text.charCodeAt(index);
  return bytes;
}

/** Text sent to a tab whose shell is not up yet; it goes in as soon as the shell starts. */
const waiting = new Map<string, string>();

/** Types `data` into the shell of tab `id` (held back until the shell is up). */
export function writeToSession(id: string, data: string) {
  const session = live.get(id);
  if (session && session.up && !session.exited) void call('pty_write', { id, data }).catch(() => {});
  else waiting.set(id, (waiting.get(id) ?? '') + data);
}

// ---- What a tab is running. There is no shell integration, so a command counts as running when it was sent (Enter, or
// `noteCommand`) and, a moment later, the cursor's line is not a prompt any more. It is looked at when the shell writes
// (at most every 0.8 s, and only while a command is watched), never on a timer of its own.

const PROMPT = /^\s*(PS [^>]*>|[A-Za-z]:[\\/][^>]*>|\S+@\S+.*?[$#%]|[$#%>❯»➜λ])(\s|$)/;
const LOCAL = String.raw`(?:localhost|127\.0\.0\.1|0\.0\.0\.0|\[::1\])`;
const LOCAL_URL = new RegExp(String.raw`(?:https?:\/\/${LOCAL}(?::\d+)?|${LOCAL}:\d+)[^\s"'<>)\]]*`, 'gi');
const SETTLE = 1200;
/** Commands sent to a tab whose shell is not up yet: they are watched once it starts. */
const queuedRuns = new Map<string, string>();

/** The line the cursor is on (joined with the lines it wraps over); null in a full-screen program. */
function cursorLine(term: Terminal) {
  const buffer = term.buffer.active;
  if (buffer.type === 'alternate') return null;
  let y = buffer.baseY + buffer.cursorY;
  let text = '';
  for (;;) {
    const line = buffer.getLine(y);
    if (!line) break;
    text = line.translateToString(true) + text;
    if (!line.isWrapped || y === 0) break;
    y -= 1;
  }
  return text;
}

/** The last local address printed since line `from` (the text of the buffer has no colour codes). */
function localUrlSince(term: Terminal, from: number) {
  const buffer = term.buffer.active;
  let found: string | null = null;
  let text = '';
  const scan = () => {
    for (const match of text.matchAll(LOCAL_URL)) found = match[0];
    text = '';
  };
  for (let y = Math.max(from, buffer.length - 400, 0); y < buffer.length; y++) {
    const line = buffer.getLine(y);
    if (!line) continue;
    if (!line.isWrapped) scan();
    text += line.translateToString(true);
  }
  scan();
  if (found === null) return null;
  return (found as string)
    .replace(/[.,;:]+$/, '')
    .replace(/^(?!https?:\/\/)/i, 'http://')
    .replace('//0.0.0.0', '//localhost');
}

function report(id: string, session: Live) {
  if (session.run) runWatcher?.(id, { runId: session.run.runId, command: session.run.command, cwd: session.run.cwd ?? session.cwd, startedAt: session.run.startedAt, url: session.url });
}

function endRun(id: string, session: Live) {
  clearTimeout(session.timer);
  session.timer = 0;
  if (!session.run) return;
  session.run = undefined;
  session.url = null;
  runWatcher?.(id, null);
}

function schedule(id: string, session: Live, delay: number) {
  if (session.timer) return;
  session.timer = window.setTimeout(() => evaluate(id, session), delay);
}

/** Looks once at a watched command: still running (and on which address), or the prompt is back. */
function evaluate(id: string, session: Live) {
  session.timer = 0;
  const run = session.run;
  if (!run || live.get(id) !== session) return;
  if (session.exited) return endRun(id, session);
  const wait = run.startedAt + SETTLE - Date.now();
  if (wait > 0) return schedule(id, session, wait);
  const line = cursorLine(session.term);
  if (line !== null && PROMPT.test(line)) return endRun(id, session);
  session.url = localUrlSince(session.term, run.line);
  report(id, session);
}

let runCounter = 0;
function beginRun(id: string, session: Live, command: string) {
  const sent = command.split(/\r?\n/).find((line) => line.trim())?.trim();
  if (!sent) return;
  // The Run button moves into the project's folder first: the entry shows the user's command and that folder.
  const { command: first, cwd } = stripLocationPrefix(sent);
  if (!first) return;
  endRun(id, session);
  const buffer = session.term.buffer.active;
  // Every run has its own id, so the same command run again in the same tab is a new entry.
  session.run = { runId: ++runCounter, command: first, cwd, startedAt: Date.now(), line: buffer.baseY + buffer.cursorY };
  schedule(id, session, SETTLE);
}

/** A command sent to tab `id` from outside (a chat's Run button): watched as if it had been typed there. */
export function noteCommand(id: string, command: string) {
  const session = live.get(id);
  if (session && session.up && !session.exited && !waiting.has(id)) beginRun(id, session, command);
  else queuedRuns.set(id, command);
}

/**
 * Takes over a shell the Rust side kept running while the page was away (a reload): its kept output is written into
 * the terminal and the output that arrived meanwhile follows. False when there is no such shell.
 * While the replay is written, the terminal's answers to what a program asked of it back then are not typed into the shell.
 */
async function adopt(id: string, session: Live): Promise<boolean> {
  session.replaying = true;
  session.held = [];
  try {
    const attached = await call<{ name: string; replay: string; total: number } | null>('pty_attach', { id, cols: session.term.cols, rows: session.term.rows });
    if (!attached) return false;
    await new Promise<void>((done) => session.term.write(decode(attached.replay), done));
    for (const chunk of session.held) if (chunk.end > attached.total) session.term.write(chunk.bytes);
    session.hooks.title(attached.name);
    return true;
  } finally {
    session.held = [];
    session.replaying = false;
  }
}

function start(id: string, session: Live) {
  const done = begin(id, session);
  session.ready = done;
  return done;
}

async function begin(id: string, session: Live) {
  session.exited = false;
  session.up = false;
  session.hooks.status('starting');
  try {
    let name: string | null = null;
    if (!(await adopt(id, session))) name = await call<string>('pty_open', { id, cwd: session.cwd ?? null, cols: session.term.cols, rows: session.term.rows });
    if (name !== null) session.hooks.title(name);
    session.up = true;
    session.hooks.status('running');
    const queued = waiting.get(id);
    waiting.delete(id);
    if (queued) void call('pty_write', { id, data: queued }).catch(() => {});
    const command = queuedRuns.get(id);
    queuedRuns.delete(id);
    if (command) beginRun(id, session, command);
  } catch (error) {
    session.exited = true;
    session.up = false;
    session.hooks.status('exited');
    session.term.write(`\r\n\x1b[31m${String(error)}\x1b[0m\r\nPress Enter to try again.\r\n`);
  }
}

function create(id: string, cwd: string | undefined, hooks: SessionHooks): Live {
  const windows = navigator.userAgent.includes('Windows');
  const term = new Terminal({
    allowProposedApi: true,
    cursorBlink: true,
    // A thin bar, like an editor's cursor, instead of the block that covers the letter.
    cursorStyle: 'bar',
    cursorWidth: 2,
    cursorInactiveStyle: 'bar',
    fontFamily: '"Cascadia Code", "Cascadia Mono", Consolas, ui-monospace, monospace',
    fontSize: 12,
    lineHeight: 1.2,
    scrollback: 10000,
    theme: theme(),
    windowsPty: windows ? { backend: 'conpty' } : undefined,
  });
  const fit = new FitAddon();
  const search = new SearchAddon();
  term.loadAddon(fit);
  term.loadAddon(search);
  term.loadAddon(
    new WebLinksAddon((_event, uri) => {
      void call('open_external', { url: uri }).catch(() => {});
    }),
  );
  const session: Live = { term, fit, search, cwd, hooks, exited: false, url: null, timer: 0, replaying: false, held: [], up: false, fitFrame: 0, watchers: new Set(), pushTimer: 0, reportScroll: () => {} };

  term.onData((data) => {
    if (session.replaying) return;
    if (session.exited) {
      if (data === '\r') void start(id, session);
      return;
    }
    if (data.includes('\r')) {
      // Enter: the cursor line still holds the prompt and what was typed after it.
      const line = cursorLine(term);
      const match = line === null ? null : PROMPT.exec(line);
      const command = line !== null && match ? line.slice(match[0].length).trim() : '';
      if (command) beginRun(id, session, command);
    }
    void call('pty_write', { id, data }).catch(() => {});
  });
  // The scroll arrows need to know when the view is at either end (coalesced per frame).
  let reporting = 0;
  const reportScroll = () => {
    cancelAnimationFrame(reporting);
    reporting = requestAnimationFrame(() => {
      const buffer = term.buffer.active;
      session.hooks.scrolled?.(buffer.viewportY <= 0, buffer.viewportY >= buffer.baseY);
    });
  };
  term.onScroll(reportScroll);
  term.onWriteParsed(reportScroll);
  session.reportScroll = reportScroll;
  term.onResize(({ cols, rows }) => {
    if (!session.exited) void call('pty_resize', { id, cols, rows }).catch(() => {});
  });
  term.onTitleChange((title) => {
    // PowerShell titles the window with the path of its program: the last part is the name.
    const name = title.split(/[\\/]/).filter(Boolean).at(-1)?.replace(/\.exe$/i, '');
    if (name) hooks.title(name.length > 24 ? `${name.slice(0, 23)}…` : name);
  });
  term.attachCustomKeyEventHandler((event) => {
    if (event.type !== 'keydown') return true;
    const mod = event.ctrlKey || event.metaKey;
    if (!mod) return true;
    const key = event.key.toLowerCase();
    // The page's own shortcuts (show or hide, new, close and switch terminals): not for the shell.
    if (key === '`' || (key === 'j' && !event.shiftKey) || (key === 'w' && event.shiftKey) || key === 'pageup' || key === 'pagedown') return false;
    // Ctrl+Shift+C copies the selection; Ctrl+C does too while text is selected, else it is the interrupt.
    if (key === 'c' && (event.shiftKey || term.hasSelection())) {
      event.preventDefault();
      void copySelection(id);
      return false;
    }
    // Ctrl+Shift+V pastes (plain Ctrl+V is left to the browser's own paste, which xterm handles).
    if (key === 'v' && event.shiftKey) {
      event.preventDefault();
      void pasteInto(id);
      return false;
    }
    if (key === 'f' && !event.shiftKey) {
      event.preventDefault();
      hooks.find();
      return false;
    }
    return true;
  });
  live.set(id, session);
  return session;
}

/**
 * Whether the panel is still sliding in or out. While it does, its box grows or shrinks every frame (down to nothing),
 * and a terminal fitted to it would be cut to a row or two, and the shell with it: the output on screen would go up
 * into the scrollback and the view would be empty. A terminal is only fitted once the box has its real size.
 */
function sliding(element: Element) {
  const panel = element.closest('[data-terminal]') ?? element;
  return panel.getAnimations({ subtree: true }).some((animation) => animation.playState === 'running' && animation.effect?.getTiming().iterations !== Infinity);
}

/** Resolves once `container` has its real size (not sliding, not collapsed), or when it was taken away or it took too long. */
function settled(container: HTMLElement) {
  return new Promise<void>((done) => {
    let frames = 0;
    let last = '';
    const look = () => {
      const size = `${container.clientWidth}x${container.clientHeight}`;
      // The same size two frames in a row (a neighbouring panel may still be sliding), and a real one.
      const steady = !sliding(container) && size === last && container.clientHeight > 40 && container.clientWidth > 200;
      last = size;
      if (!container.isConnected || steady || ++frames > 120) return done();
      requestAnimationFrame(look);
    };
    look();
  });
}

/** Shows the terminal of tab `id` in `container`, starting its shell the first time. */
export function attachSession(id: string, container: HTMLElement, cwd: string | undefined, hooks: SessionHooks) {
  if (!terminalAvailable()) return;
  watchTheme();
  void listenToShells();
  const existing = live.get(id);
  if (existing) {
    // The panel was hidden and is back: the same terminal (its buffer, its scrollback, its shell) moves into the new box.
    existing.hooks = hooks;
    const viewport = existing.term.buffer.active.viewportY;
    if (existing.term.element) container.append(existing.term.element);
    else existing.term.open(container);
    // A box that left the page forgot where its scrollbar was.
    existing.term.scrollToLine(viewport);
    existing.hooks.status(existing.exited ? 'exited' : 'running');
    existing.reportScroll();
    fitSession(id);
    existing.term.refresh(0, existing.term.rows - 1);
    return;
  }
  const session = create(id, cwd, hooks);
  session.term.open(container);
  try {
    const gl = new WebglAddon();
    gl.onContextLoss(() => gl.dispose());
    session.term.loadAddon(gl);
  } catch {
    // No WebGL: the canvas-less DOM renderer stays.
  }
  // The shell starts at the size of the box once it has settled, not at the size of a panel that is still opening.
  session.ready = settled(container).then(() => {
    if (live.get(id) !== session) return;
    if (container.isConnected) fitNow(session);
    return start(id, session);
  });
}

/**
 * After a reload: finds again the command a tab was running. The tab's shell is taken over (without showing the
 * tab), and when its cursor line is not a prompt the command counts as running again, from its original start; the
 * run watcher is told as for any run. Returns what became of it: 'running', 'ok' (back at the prompt) or 'stopped'
 * (the shell is gone).
 */
export async function resumeRun(id: string, hooks: SessionHooks, saved: { command: string; cwd?: string; startedAt: number }): Promise<'running' | 'ok' | 'stopped'> {
  if (!terminalAvailable()) return 'stopped';
  await listenToShells();
  let session = live.get(id);
  if (!session) {
    session = create(id, saved.cwd, hooks);
    session.ready = adopt(id, session);
    if (!(await session.ready.catch(() => false))) {
      live.delete(id);
      session.term.dispose();
      return 'stopped';
    }
    session.up = true;
    hooks.status('running');
  } else {
    await session.ready?.catch(() => {});
    if (live.get(id) !== session || session.exited) return 'stopped';
  }
  if (session.run) return 'running';
  const line = cursorLine(session.term);
  if (line !== null && PROMPT.test(line)) return 'ok';
  // The replay is old output, not a command that was just typed: it is watched from the original start.
  session.run = { runId: ++runCounter, command: saved.command, cwd: saved.cwd, startedAt: saved.startedAt, line: 0 };
  session.url = localUrlSince(session.term, 0);
  report(id, session);
  schedule(id, session, 800);
  return 'running';
}

function fitNow(session: Live) {
  try {
    const size = session.fit.proposeDimensions();
    // Never to a sliver: a box with no real size yet (or a window squeezed to nothing) leaves the terminal as it is.
    if (!size || size.rows < 2 || size.cols < 20) return;
    session.fit.fit();
  } catch {
    // Not laid out yet.
  }
}

/** Fits the terminal to its container (when it is on screen and settled) and tells the shell the new size. */
export function fitSession(id: string) {
  const session = live.get(id);
  const element = session?.term.element;
  if (!session || !element || !element.offsetParent) return;
  if (sliding(element)) {
    // Not yet: look again next frame, once.
    if (!session.fitFrame) {
      session.fitFrame = requestAnimationFrame(() => {
        session.fitFrame = 0;
        fitSession(id);
      });
    }
    return;
  }
  fitNow(session);
}

/** The last `lines` lines of a terminal's buffer as plain text (no colours), for the Tasks panel; null when it is gone. */
export function sessionTail(id: string, lines: number): string | null {
  const session = live.get(id);
  if (!session) return null;
  const buffer = session.term.buffer.active;
  const first = Math.max(0, buffer.length - lines);
  const out: string[] = [];
  for (let y = first; y < buffer.length; y++) {
    const line = buffer.getLine(y);
    if (!line) continue;
    const text = line.translateToString(true);
    if (line.isWrapped && out.length > 0) out[out.length - 1] += text;
    else out.push(text);
  }
  while (out.length > 0 && !out[out.length - 1].trim()) out.pop();
  return (first > 0 ? '(Earlier output not shown)\n' : '') + out.join('\n');
}

/** How soon after output arrives the watchers are told (several chunks in that time are one push). */
const PUSH_EVERY = 120;

/** Tells the watchers of a tab the tail of its output, at most once every `PUSH_EVERY` ms. */
function pushOutput(id: string, session: Live) {
  if (session.pushTimer || session.watchers.size === 0) return;
  session.pushTimer = window.setTimeout(() => {
    session.pushTimer = 0;
    if (live.get(id) !== session || session.watchers.size === 0) return;
    const text = sessionTail(id, 200);
    if (text !== null) for (const watcher of session.watchers) watcher(text);
  }, PUSH_EVERY);
}

/**
 * Calls `watcher` with the tail of tab `id`'s output now and again whenever the shell prints (throttled). Nothing runs
 * while there is no watcher. Returns the function that stops watching.
 */
export function watchOutput(id: string, watcher: (text: string) => void): () => void {
  const session = live.get(id);
  if (!session) return () => {};
  session.watchers.add(watcher);
  const text = sessionTail(id, 200);
  if (text !== null) watcher(text);
  return () => {
    session.watchers.delete(watcher);
    if (session.watchers.size === 0) {
      clearTimeout(session.pushTimer);
      session.pushTimer = 0;
    }
  };
}

export const focusSession = (id: string) => live.get(id)?.term.focus();

export function findInSession(id: string, query: string, backwards = false) {
  const session = live.get(id);
  if (!session) return false;
  if (!query) {
    session.search.clearDecorations();
    return false;
  }
  return backwards ? session.search.findPrevious(query) : session.search.findNext(query);
}

export const clearSession = (id: string) => live.get(id)?.term.clear();

export const hasSelection = (id: string) => live.get(id)?.term.hasSelection() ?? false;

/** Copies the selected text (if any) to the clipboard. */
export async function copySelection(id: string) {
  const term = live.get(id)?.term;
  if (!term?.hasSelection()) return;
  await navigator.clipboard.writeText(term.getSelection()).catch(() => {});
  term.clearSelection();
}

/** Types the clipboard's text into the shell (as a paste: bracketed when the program asked for it). */
export async function pasteInto(id: string) {
  const session = live.get(id);
  if (!session) return;
  const text = await navigator.clipboard.readText().catch(() => '');
  if (text) session.term.paste(text);
}

/** Scrolls tab `id`'s view by `lines` (negative: up), for the scroll arrows. */
export const scrollSession = (id: string, lines: number) => live.get(id)?.term.scrollLines(lines);

export const selectAllIn = (id: string) => live.get(id)?.term.selectAll();

/** Ends the shell of a closed tab and drops its terminal. */
export function disposeSession(id: string) {
  const session = live.get(id);
  if (!session) return;
  endRun(id, session);
  live.delete(id);
  queuedRuns.delete(id);
  session.term.dispose();
  void call('pty_close', { id }).catch(() => {});
}
