// State and requests of the AgentDeck browser (see src-tauri/src/browser.rs). Every tab is its own webview; a script
// injected into each page reports console, network and navigation, and answers DOM / screenshot / element requests.
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
export type NetworkLine = { method: string; url: string; status: number; ms: number; at: number };
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
  /** Comments pinned to elements of the page, waiting to be sent to the chat. */
  comments: [] as PageComment[],
  devtools: false,
  devtoolsHeight: 220,
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
      browser.network.push({ method: String(payload.method), url: String(payload.url), status: Number(payload.status), ms: Number(payload.ms), at: Number(payload.at) });
      if (browser.network.length > LIMIT) browser.network.splice(0, browser.network.length - LIMIT);
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
  const tab = blankTab(url);
  browser.tabs.push(tab);
  browser.activeId = tab.id;
  return tab;
}

export async function closeTab(id: string) {
  const index = browser.tabs.findIndex((tab) => tab.id === id);
  if (index === -1) return;
  const [tab] = browser.tabs.splice(index, 1);
  if (tab.opened && inTauri()) await call('browser_close', { tab: id }).catch(() => {});
  if (browser.tabs.length === 0) browser.tabs.push(blankTab());
  if (browser.activeId === id) browser.activeId = browser.tabs[Math.max(0, index - 1)].id;
}

export function selectTab(id: string) {
  if (browser.tabs.some((tab) => tab.id === id)) browser.activeId = id;
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
export const reload = () => run('location.reload()');
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
export function openInPreview(url: string) {
  const tab = activeTab();
  if (tab.url && tab.url !== url) newTab(url);
  else tab.url = url;
  browser.autostart = url;
  layout.activeTool = 'Preview';
  layout.toolsOpen = true;
}

/** The right-click menu of a link, shown by LinkMenu. */
export const linkMenu = $state({ open: false, url: '', x: 0, y: 0 });
