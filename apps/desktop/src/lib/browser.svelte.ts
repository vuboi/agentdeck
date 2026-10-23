// State and requests of the AgentDeck browser (see src-tauri/src/browser.rs). Every tab is its own webview; a script
// injected into each page reports console, network and navigation, and answers DOM / screenshot / element requests.
import { listenToNetwork } from './devtools-net.svelte';
import { inTauri } from './history';
import { layout } from './layout.svelte';

export type Device = 'desktop' | 'tablet' | 'mobile';
export const DEVICES: Record<Device, { label: string; name: string; w: number; h: number; ua?: string }> = {
  desktop: { label: 'Desktop', name: 'Desktop', w: 0, h: 0 }, // fills the panel
  tablet: {
    label: 'Tablet',
    name: 'iPad Air',
    w: 820,
    h: 1180,
    ua: 'Mozilla/5.0 (iPad; CPU OS 17_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Mobile/15E148 Safari/604.1',
  },
  mobile: {
    label: 'Mobile',
    name: 'iPhone 14',
    w: 390,
    h: 844,
    ua: 'Mozilla/5.0 (iPhone; CPU iPhone OS 17_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Mobile/15E148 Safari/604.1',
  },
};

export type ConsoleLine = { level: string; text: string; at: number };
export type NetworkLine = { method: string; url: string; status: number; ms: number; at: number; /** The response text, when the request returned text or JSON. */ body?: string; /** What was sent. */ request?: string };
export type PickedElement = {
  selector: string;
  tag: string;
  text: string;
  html: string;
  box: { x: number; y: number; w: number; h: number };
  style: Record<string, string>;
};
export type PageComment = { id: string; selector: string; html: string; box: PickedElement['box']; note: string };
export type PageSnapshot = { url: string; title: string; viewport: { w: number; h: number; dpr: number }; html: string };

/** One browser tab. `opened` is true once its webview exists; `url` follows the page as it navigates. */
export type BrowserTab = { id: string; url: string; title: string; favicon: string; loading: boolean; opened: boolean };

const LIMIT = 300;
let counter = 0;
const newTabId = () => `t${++counter}${Date.now().toString(36)}`;
const blankTab = (url = ''): BrowserTab => ({ id: newTabId(), url, title: '', favicon: '', loading: false, opened: false });

export type AnnotateTool = 'pick' | 'arrow' | 'box' | 'note';
/** One mark on the page: where it is (page pixels), what was written, and for an element which one. */
export type AnnotationNote = {
  n: number;
  type: string;
  text: string;
  x: number;
  y: number;
  /** For a mark on an element: which one, with its markup and look. */
  element?: { tag: string; text: string; selector: string; html: string; box: { x: number; y: number; w: number; h: number }; style: Record<string, string> };
};

export const browser = $state({
  tabs: [blankTab()] as BrowserTab[],
  activeId: '' as string,
  device: 'desktop' as Device,
  landscape: false,
  picking: false,
  picked: null as PickedElement | null,
  console: [] as ConsoleLine[],
  network: [] as NetworkLine[],
  error: '',
  /** Open the page at once when the browser view mounts (developer hook, links). */
  autostart: '' as string,
  /** The project command being started from the Preview (cwd|command), '' when none: it outlives the panel. */
  starting: '',
  /** Comments pinned to elements of the page, waiting to be sent to the chat. */
  comments: [] as PageComment[],
  devtools: false,
  devtoolsHeight: 220,
  /** Marking up the live page: comment on an element, draw an arrow or a box, or pin a note. */
  annotating: false,
  annotateTool: 'arrow' as AnnotateTool,
  /** How many notes are on the page: with some, the toolbar offers Add to chat. */
  annotateCount: 0,
  /** Whether the marks have something to undo or to redo. */
  annotateUndo: false,
  annotateRedo: false,
  /** A note on the page is selected: the delete button then removes that one, not all. */
  annotateSelected: false,
});
browser.activeId = browser.tabs[0].id;

export const activeTab = () => browser.tabs.find((tab) => tab.id === browser.activeId) ?? browser.tabs[0];

async function call<T = void>(command: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<T>(command, args);
}

// ---- reports from the pages ----
type Report = { kind: string; tab?: string; id?: string; [key: string]: unknown };
const waiting = new Map<string, (report: Report) => void>();
let listening = false;

export async function listenToPage() {
  if (listening || !inTauri()) return;
  listening = true;
  const { listen } = await import('@tauri-apps/api/event');
  void listenToNetwork();
  // The agent asked to open an address (browser_open): the same as a link opened in AgentDeck.
  await listen<{ url: string; newTab?: boolean }>('agent-open', ({ payload }) => openForAgent(payload.url, payload.newTab === true));
  // The agent switched the device (browser_device): pages are created again with that device's user agent.
  await listen<{ device: Device; landscape: boolean }>('agent-device', ({ payload }) => {
    if (!(payload.device in DEVICES)) return;
    browser.device = payload.device;
    browser.landscape = payload.landscape === true;
    layout.activeTool = 'Preview';
    layout.toolsOpen = true;
  });
  await listen<Report>('browser-report', ({ payload }) => {
    if (payload.id && waiting.has(payload.id)) {
      waiting.get(payload.id)?.(payload);
      waiting.delete(payload.id);
    }
    const tab = browser.tabs.find((item) => item.id === payload.tab);
    if (payload.kind === 'console' && payload.tab === browser.activeId) {
      browser.console.push({ level: String(payload.level), text: String(payload.text), at: Number(payload.at) });
      if (browser.console.length > LIMIT) browser.console.splice(0, browser.console.length - LIMIT);
    } else if (payload.kind === 'network' && payload.tab === browser.activeId) {
      browser.network.push({ method: String(payload.method), url: String(payload.url), status: Number(payload.status), ms: Number(payload.ms), at: Number(payload.at), body: typeof payload.body === 'string' ? payload.body : undefined, request: typeof payload.request === 'string' ? payload.request : undefined });
      if (browser.network.length > LIMIT) browser.network.splice(0, browser.network.length - LIMIT);
    } else if (payload.kind === 'ann-count' && payload.tab === browser.activeId) {
      browser.annotateCount = Number(payload.count) || 0;
      browser.annotateUndo = payload.undo === true;
      browser.annotateRedo = payload.redo === true;
      browser.annotateSelected = payload.selected === true;
    } else if (payload.kind === 'nav' && tab) {
      tab.url = String(payload.url);
      tab.title = String(payload.title ?? '');
      tab.favicon = String(payload.favicon ?? '');
    } else if (payload.kind === 'title' && tab) {
      tab.title = String(payload.title ?? '');
    }
  });
  // The page started or finished loading: drives the spinner on the tab and the reload button.
  await listen<{ label: string; event: 'started' | 'finished'; url: string }>('browser-load', ({ payload }) => {
    const tab = browser.tabs.find((item) => `browser-${item.id}` === payload.label);
    if (!tab) return;
    tab.loading = payload.event === 'started';
    if (payload.event === 'started') {
      tab.url = payload.url;
      if (tab.id === browser.activeId) {
        // A new page takes the marks with it: leave the mode.
        browser.annotating = false;
        browser.annotateCount = 0;
        browser.console = [];
        browser.network = [];
      }
    }
  });
}

/** Asks the page of the front tab for something and waits for its answer. */
function ask<T extends Report>(kind: string, arg: unknown = null, timeout = 15000): Promise<T> {
  const id = crypto.randomUUID();
  const tab = activeTab().id;
  return new Promise<T>((resolve, reject) => {
    const timer = setTimeout(() => {
      waiting.delete(id);
      reject(new Error(`The page did not answer (${kind}). Is a page loaded?`));
    }, timeout);
    waiting.set(id, (report) => {
      clearTimeout(timer);
      resolve(report as T);
    });
    call('browser_eval', { tab, script: `window.__agentdeck && window.__agentdeck.run(${JSON.stringify(id)}, ${JSON.stringify(kind)}, ${JSON.stringify(arg)})` }).catch((error: unknown) => {
      clearTimeout(timer);
      waiting.delete(id);
      reject(new Error(String(error)));
    });
  });
}

// ---- tabs ----
export type Rect = { x: number; y: number; w: number; h: number };

export function newTab(url = '') {
  if (browser.annotating) void stopAnnotating();
  browser.error = '';
  const tab = blankTab(url);
  browser.tabs.push(tab);
  browser.activeId = tab.id;
  return tab;
}

export async function closeTab(id: string) {
  const index = browser.tabs.findIndex((tab) => tab.id === id);
  if (index === -1) return;
  if (browser.annotating && id === browser.activeId) {
    browser.annotating = false;
    browser.annotateCount = 0;
  }
  browser.error = '';
  const [tab] = browser.tabs.splice(index, 1);
  if (tab.opened && inTauri()) await call('browser_close', { tab: id }).catch(() => {});
  if (browser.tabs.length === 0) browser.tabs.push(blankTab());
  if (browser.activeId === id) browser.activeId = browser.tabs[Math.max(0, index - 1)].id;
}

export function selectTab(id: string) {
  if (!browser.tabs.some((tab) => tab.id === id)) return;
  // An error belongs to the page it came from.
  if (id !== browser.activeId) browser.error = '';
  // The marks belong to the page they were drawn on: leaving it ends the mode there.
  if (browser.annotating && id !== browser.activeId) void stopAnnotating();
  browser.activeId = id;
}

let showing: Promise<void> | null = null;
/** Shows the front tab's page in `rect`, creating it the first time (or after a device change). */
export function showBrowser(rect: Rect): Promise<void> {
  // One open at a time: a resize and a navigation arriving together must not both create the page.
  showing ??= openBrowser(rect).finally(() => (showing = null));
  return showing;
}
async function openBrowser(rect: Rect) {
  const tab = activeTab();
  if (!tab.url) return;
  await call('browser_show', { tab: tab.id, url: tab.url, userAgent: DEVICES[browser.device].ua ?? null, x: rect.x, y: rect.y, w: rect.w, h: rect.h });
  tab.opened = true;
}
export const moveBrowser = (rect: Rect) => call('browser_bounds', { tab: activeTab().id, x: rect.x, y: rect.y, w: rect.w, h: rect.h });
export const hideBrowser = () => call('browser_hide');
/** The panel is not on screen: the front page keeps running off-screen (a hidden page stops drawing and cannot be photographed). */
export const parkBrowser = () => call('browser_park').catch(() => {});
/** At start-up (also after F5): closes the pages a previous load of the window left running above it. */
export const resetBrowser = () => (inTauri() ? call('browser_reset').catch(() => {}) : Promise.resolve());
/** A picture of the front page taken by the engine (PNG data URL). */
export const capturePage = () => call<string>('browser_capture', { tab: activeTab().id });
export const zoomBrowser = (factor: number) => call('browser_zoom', { tab: activeTab().id, factor });

export async function goTo(url: string) {
  const tab = activeTab();
  tab.url = url;
  if (tab.opened) {
    tab.loading = true;
    await call('browser_navigate', { tab: tab.id, url });
  }
}
const run = (script: string) => call('browser_eval', { tab: activeTab().id, script });
export const back = () => run('history.back()');
export const forward = () => run('history.forward()');
// Reload asks for the address again, so an address that does not answer gets AgentDeck's own error page, never the
// browser's chrome-error page (a reload of a page the browser cannot reach would show that).
export const reload = () => {
  const tab = activeTab();
  return tab.url ? goTo(tab.url) : Promise.resolve();
};
export const stop = () => run('window.stop()');

/** The pages are created again with the new device's user agent (it cannot change on a live page), at the same address. */
export async function recreateBrowser() {
  for (const tab of browser.tabs) {
    // Marked closed first: a placement running meanwhile must not reach a page that is going away.
    const was = tab.opened;
    tab.opened = false;
    if (was && inTauri()) await call('browser_close', { tab: tab.id }).catch(() => {});
  }
}

export async function snapshot(): Promise<PageSnapshot> {
  const report = await ask<Report & PageSnapshot>('dom');
  return { url: report.url, title: report.title, viewport: report.viewport, html: report.html };
}

/** A screenshot of what the page shows now, as a PNG data URL, with the size it was taken at. */
export async function screenshot(): Promise<{ data: string; w: number; h: number }> {
  const report = await ask<Report & { data?: string; w: number; h: number; error?: string }>('shot', null, 30000);
  if (!report.data) throw new Error(report.error ?? 'screenshot failed');
  return { data: report.data, w: report.w, h: report.h };
}

/** The element at a point of the page (CSS pixels, as in the screenshot). */
export async function elementAt(x: number, y: number): Promise<PickedElement | null> {
  const report = await ask<Report & { element: PickedElement | null }>('at', { x, y });
  return report.element;
}

/** Lets the user click an element in the live page; resolves with it (and where it is), or null when cancelled. */
export async function pickElement(): Promise<{ element: PickedElement; url: string; title: string } | null> {
  browser.picking = true;
  try {
    const report = await ask<Report & { element: PickedElement | null; url?: string; title?: string }>('pick', null, 300000);
    return report.element ? { element: report.element, url: report.url ?? '', title: report.title ?? '' } : null;
  } finally {
    browser.picking = false;
  }
}
/** Starts marking up the live page with a tool, or switches the tool. */
export async function annotate(tool: AnnotateTool) {
  browser.annotating = true;
  browser.annotateTool = tool;
  await ask('ann', { tool });
}

/** Undo or redo the last change to the marks, or take them all off (which can be undone). */
export const markHistory = (action: 'undo' | 'redo' | 'remove') => ask('ann', { [action]: true });

/** The marks on the page, to send with its picture. */
export async function annotationNotes(): Promise<AnnotationNote[]> {
  const report = await ask<Report & { result: { notes: AnnotationNote[] } }>('ann', { state: true });
  return report.result.notes;
}

/** Ends the mode and takes every mark off the page. */
export async function stopAnnotating() {
  browser.annotating = false;
  browser.annotateCount = 0;
  browser.annotateUndo = false;
  browser.annotateRedo = false;
  browser.annotateSelected = false;
  await ask('ann', { clear: true }).catch(() => {});
}

export const cancelPick = () => run("window.__agentdeck && window.__agentdeck.run('', 'unpick')");

/** Pins a note to an element. */
export function addComment(element: Pick<PickedElement, 'selector' | 'html' | 'box'>, note: string) {
  if (!note.trim()) return;
  browser.comments.push({ id: crypto.randomUUID(), selector: element.selector, html: element.html, box: element.box, note: note.trim() });
}

export const openExternal = (url: string) => call('open_external', { url });

/** A local address (a dev server) is opened in the AgentDeck browser instead of leaving the app. */
export const isLocalUrl = (url: string) => /^https?:\/\/(localhost|127\.0\.0\.1|\[::1\])(:\d+)?(\/|$)/i.test(url);

/** Shows `url` in the browser: in the front tab when that is blank, in a new tab otherwise. */
/**
 * `auto` (a link): a page already open in a tab is shown as it is; any other address gets a tab of its own, never the
 * open page's. `current` (the agent): the front tab goes to the address. `new`: a tab of its own.
 */
export function openInPreview(url: string, mode: 'auto' | 'current' | 'new' = 'auto') {
  if (mode === 'new') newTab(url);
  else if (mode === 'auto') {
    const shown = browser.tabs.find((tab) => tab.url === url);
    if (shown) browser.activeId = shown.id;
    else if (activeTab().url) newTab(url);
    else activeTab().url = url;
  }
  browser.autostart = url;
  layout.activeTool = 'Preview';
  layout.toolsOpen = true;
}

/**
 * The agent (browser_open, browser_serve) opens an address. With the Preview on screen it opens there, as a link would.
 * Otherwise the Preview panel is left alone: the page is created off-screen, where it keeps drawing, so the agent can
 * still look at it, click, read the console and take screenshots; the user finds it in the Preview when they open it.
 */
function openForAgent(url: string, inNewTab: boolean) {
  if (!inTauri() || (layout.toolsOpen && layout.activeTool === 'Preview')) return openInPreview(url, inNewTab ? 'new' : 'current');
  const front = activeTab();
  const tab = inNewTab && front.url ? newTab(url) : front;
  tab.url = url;
  if (tab.opened) void call('browser_navigate', { tab: tab.id, url }).catch(() => {});
  else void showBrowser({ x: -20000, y: -20000, w: 1100, h: 760 }).catch(() => {});
}

/** The right-click menu of a link, shown by LinkMenu. */
/** The right-click menu of a link. `file` is set when the link is a file in the project: then `root` is its folder. */
export const linkMenu = $state({ open: false, url: '', x: 0, y: 0, file: '', root: '' });

// ---- running the project from the Preview ----
export type RunTarget = { name: string; command: string; cwd: string; url: string | null; source: string };

/** The ways the project in `cwd` can be run, read from its own files (scripts, launch.json, manage.py). */
export const detectTargets = (cwd: string) => (inTauri() && cwd ? call<RunTarget[]>('detect_run_targets', { cwd }) : Promise.resolve([] as RunTarget[]));

/** Starts the project as a server AgentDeck owns and returns the address once it answers. */
export const runTarget = (target: RunTarget) => call<string>('run_target', { command: target.command, cwd: target.cwd, url: target.url });

// ---- the servers running in the background ----
export type ServerInfo = { key: number; command: string; cwd: string; url: string | null; pid: number; age: number; running: boolean };
/** The dev servers AgentDeck started (from the Preview or by an agent), kept in step by `refreshServers`. */
export const servers = $state<{ list: ServerInfo[] }>({ list: [] });

export async function refreshServers() {
  if (!inTauri()) return;
  try {
    servers.list = await call<ServerInfo[]>('servers_list');
  } catch {
    // The list stays as it was.
  }
}

/** Stops one server (its key), or all of them, with every process they started. */
export async function stopServers(key: number | null) {
  await call('server_stop', { key });
  await refreshServers();
}
