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

export type SessionStatus = 'starting' | 'running' | 'exited';

/** What a terminal tells the tab that shows it. */
export type SessionHooks = {
  title: (name: string) => void;
  status: (status: SessionStatus) => void;
  /** Ctrl+F in the terminal. */
  find: () => void;
};

type Live = {
  term: Terminal;
  fit: FitAddon;
  search: SearchAddon;
  cwd?: string;
  hooks: SessionHooks;
  exited: boolean;
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
    await listen<{ id: string; data: string }>('pty-data', ({ payload }) => {
      const session = live.get(payload.id);
      if (!session) return;
      const text = atob(payload.data);
      const bytes = new Uint8Array(text.length);
      for (let index = 0; index < text.length; index++) bytes[index] = text.charCodeAt(index);
      session.term.write(bytes);
    });
    await listen<{ id: string; code: number | null }>('pty-exit', ({ payload }) => {
      const session = live.get(payload.id);
      if (!session) return;
      session.exited = true;
      session.hooks.status('exited');
      const how = payload.code === null || payload.code === 0 ? 'exited' : `exited with code ${payload.code}`;
      session.term.write(`\r\n\x1b[2m[shell ${how}. Press Enter to start a new one.]\x1b[0m\r\n`);
    });
  })();
  return listening;
}

async function start(id: string, session: Live) {
  session.exited = false;
  session.hooks.status('starting');
  try {
    const name = await call<string>('pty_open', { id, cwd: session.cwd ?? null, cols: session.term.cols, rows: session.term.rows });
    session.hooks.title(name);
    session.hooks.status('running');
  } catch (error) {
    session.exited = true;
    session.hooks.status('exited');
    session.term.write(`\r\n\x1b[31m${String(error)}\x1b[0m\r\nPress Enter to try again.\r\n`);
  }
}

function create(id: string, cwd: string | undefined, hooks: SessionHooks): Live {
  const windows = navigator.userAgent.includes('Windows');
  const term = new Terminal({
    allowProposedApi: true,
    cursorBlink: true,
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
  const session: Live = { term, fit, search, cwd, hooks, exited: false };

  term.onData((data) => {
    if (session.exited) {
      if (data === '\r') void start(id, session);
      return;
    }
    void call('pty_write', { id, data }).catch(() => {});
  });
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
    // Ctrl+Shift+C always copies; Ctrl+C copies only while text is selected, else it is the interrupt.
    if (key === 'c' && (event.shiftKey || term.hasSelection())) {
      event.preventDefault();
      void navigator.clipboard.writeText(term.getSelection()).catch(() => {});
      term.clearSelection();
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

/** Shows the terminal of tab `id` in `container`, starting its shell the first time. */
export function attachSession(id: string, container: HTMLElement, cwd: string | undefined, hooks: SessionHooks) {
  if (!terminalAvailable()) return;
  watchTheme();
  void listenToShells();
  const existing = live.get(id);
  if (existing) {
    existing.hooks = hooks;
    if (existing.term.element) container.append(existing.term.element);
    else existing.term.open(container);
    existing.hooks.status(existing.exited ? 'exited' : 'running');
    fitSession(id);
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
  session.fit.fit();
  void start(id, session);
}

/** Fits the terminal to its container (when it is on screen) and tells the shell the new size. */
export function fitSession(id: string) {
  const session = live.get(id);
  const element = session?.term.element;
  if (!session || !element || !element.offsetParent) return;
  try {
    session.fit.fit();
  } catch {
    // Not laid out yet.
  }
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

/** Copies the selection, or pastes the clipboard when nothing is selected (the right mouse button). */
export async function copyOrPaste(id: string) {
  const session = live.get(id);
  if (!session) return;
  if (session.term.hasSelection()) {
    await navigator.clipboard.writeText(session.term.getSelection()).catch(() => {});
    session.term.clearSelection();
    return;
  }
  const text = await navigator.clipboard.readText().catch(() => '');
  if (text) session.term.paste(text);
}

/** Ends the shell of a closed tab and drops its terminal. */
export function disposeSession(id: string) {
  const session = live.get(id);
  if (!session) return;
  live.delete(id);
  session.term.dispose();
  void call('pty_close', { id }).catch(() => {});
}
