// State and requests of the AgentDeck browser (see src-tauri/src/browser.rs). Every tab is its own webview; a script
// injected into each page reports console, network and navigation, and answers DOM / screenshot / element requests.
import { listenToNetwork } from './devtools-net.svelte';
import { inTauri, isPreviewCopy } from '../app/platform/history';
import { stripAnsi } from '../app/text/ansi';
import { layout, setToolsOpen } from '../app/ui/layout.svelte';

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
  /** Its place among its neighbours, as text (see `around` in the injected script). */
  around?: string;
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

export type AnnotateTool = 'pick' | 'arrow' | 'box' | 'note' | 'region';
/** One mark on the page: where it is (page pixels), what was written, and for an element which one. */
export type AnnotationNote = {
  n: number;
  type: string;
  text: string;
  x: number;
  y: number;
  /** For a mark on an element: which one, with its markup and look. */
  element?: { tag: string; text: string; selector: string; html: string; around?: string; box: { x: number; y: number; w: number; h: number }; style: Record<string, string> };
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
  /** The address of the front tab could not be opened (an invalid address, nothing answering): shown in the page's place until it changes. */
  navError: '',
  /** A project command from the Run list that did not bring a page up: shown until dismissed. */
  problem: null as RunProblem | null,
  /** Open the page at once when the browser view mounts (developer hook, links). */
  autostart: '' as string,
  /** The project commands being started from the Preview (cwd|command), none when empty: they outlive the panel, and several can start at once. */
  starting: [] as string[],
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

export const activeTab = () => browser.tabs.find((tab) => tab.id === browser.activeId) ?? browser.tabs[0] ?? blankTab();

async function call<T = void>(command: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<T>(command, args);
}

// ---- reports from the pages ----
type Report = { kind: string; tab?: string; id?: string; [key: string]: unknown };
const waiting = new Map<string, (report: Report) => void>();
let listening = false;
let regionSelectionResolve: ((selection: { box: Rect; viewport: { w: number; h: number } } | null) => void) | null = null;

export async function listenToPage() {
  if (listening || !inTauri() || isPreviewCopy()) return;
  listening = true;
  const { listen } = await import('../app/platform/tauri-event');
  void listenToNetwork();
  // The agent asked to open an address (browser_open): the same as a link opened in AgentDeck.
  await listen<{ url: string; newTab?: boolean; tab?: string | null }>('agent-open', ({ payload }) => openForAgent(payload.url, payload.newTab === true, payload.tab ?? undefined));
  // The agent switched the device (browser_device): pages are created again with that device's user agent.
  await listen<{ device: Device; landscape: boolean }>('agent-device', ({ payload }) => {
    if (!(payload.device in DEVICES)) return;
    browser.device = payload.device;
    browser.landscape = payload.landscape === true;
    layout.activeTool = 'Preview';
    setToolsOpen(true);
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
    } else if (payload.kind === 'capture-region' && payload.tab === browser.activeId) {
      browser.annotating = false;
      browser.annotateCount = 0;
      regionSelectionResolve?.({ box: payload.box as Rect, viewport: payload.viewport as { w: number; h: number } });
      regionSelectionResolve = null;
    } else if (payload.kind === 'capture-region-cancel' && payload.tab === browser.activeId) {
      browser.annotating = false;
      browser.annotateCount = 0;
      regionSelectionResolve?.(null);
      regionSelectionResolve = null;
    } else if (payload.kind === 'nav' && tab && !isErrorUrl(String(payload.url))) {
      tab.url = String(payload.url);
      tab.title = String(payload.title ?? '');
      void loadIcon(tab, String(payload.favicon ?? ''));
      recordVisit(tab.url, tab.title);
    } else if (payload.kind === 'focus-address') {
      // The page has the keyboard: bring it back to the app's window, then to the address bar.
      void import('@tauri-apps/api/webview')
        .then(({ getCurrentWebview }) => getCurrentWebview().setFocus())
        .catch(() => {})
        .finally(() => window.dispatchEvent(new Event('agentdeck:focus-address')));
    } else if (payload.kind === 'title' && tab) {
      tab.title = String(payload.title ?? '');
      recordVisit(tab.url, tab.title);
    }
  });
  // The page started or finished loading: drives the spinner on the tab and the reload button.
  await listen<{ label: string; event: 'started' | 'finished'; url: string }>('browser-load', ({ payload }) => {
    const tab = browser.tabs.find((item) => `browser-${item.id}` === payload.label);
    if (!tab) return;
    tab.loading = payload.event === 'started';
    if (payload.event === 'started') {
      if (!isErrorUrl(payload.url)) tab.url = payload.url;
      if (tab.id === browser.activeId) {
        // A new page takes the marks and the picker with it: leave both modes.
        dropPageModes();
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

/** The most tabs the browser keeps open: every tab is a live webview. */
export const MAX_TABS = 8;
export const tabsFull = () => browser.tabs.length >= MAX_TABS;

export function newTab(url = '') {
  // At the limit the front tab is used instead: a link or the agent still gets its page.
  if (tabsFull()) {
    const front = activeTab();
    if (url) front.url = url;
    return front;
  }
  if (browser.annotating) void stopAnnotating();
  browser.error = '';
  browser.navError = '';
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
  browser.navError = '';
  const [tab] = browser.tabs.splice(index, 1);
  if (tab.opened && inTauri()) await call('browser_close', { tab: id }).catch(() => {});
  if (browser.tabs.length === 0) browser.tabs.push(blankTab());
  if (browser.activeId === id) browser.activeId = browser.tabs[Math.max(0, index - 1)].id;
}

export function selectTab(id: string) {
  if (!browser.tabs.some((tab) => tab.id === id)) return;
  // An error belongs to the page it came from.
  if (id !== browser.activeId) {
    browser.error = '';
    browser.navError = '';
  }
  // The marks and the picker belong to the page they are on: leaving it ends them there (before the switch, so the
  // old page is the one told).
  if (browser.annotating && id !== browser.activeId) void stopAnnotating();
  if (browser.picking && id !== browser.activeId) void cancelPick();
  browser.activeId = id;
}

// ---- the icon of a tab ----
// The page reports the address of its icon; the app's content policy only allows images from itself, so the picture is
// fetched by the app (browser_favicon) and kept by site, so a site's icon is asked for once.
const icons = new Map<string, string>();
const iconsAsked = new Map<string, Promise<string>>();
const originOf = (url: string) => {
  try {
    return /^https?:$/.test(new URL(url).protocol) ? new URL(url).origin : '';
  } catch {
    return '';
  }
};

async function loadIcon(tab: BrowserTab, href: string) {
  const page = tab.url;
  const origin = originOf(page);
  if (!origin || !inTauri()) {
    tab.favicon = '';
    return;
  }
  const key = `${origin}|${href}`;
  const known = icons.get(key);
  if (known !== undefined) {
    tab.favicon = known;
    return;
  }
  let asked = iconsAsked.get(key);
  if (!asked) {
    asked = call<string>('browser_favicon', { page, icon: href || null })
      .catch(() => '')
      .finally(() => iconsAsked.delete(key));
    iconsAsked.set(key, asked);
  }
  const data = await asked;
  icons.set(key, data);
  if (originOf(tab.url) === origin) tab.favicon = data;
}

// ---- what was visited: the address bar suggests from it ----
export type Visit = { url: string; title: string; visits: number; last: number };
const HISTORY_KEY = 'agentdeck.browser.history';
const HISTORY_MAX = 400;
function readHistory(): Visit[] {
  try {
    const list = JSON.parse(localStorage.getItem(HISTORY_KEY) ?? '[]') as Visit[];
    return Array.isArray(list) ? list.filter((item) => item && typeof item.url === 'string') : [];
  } catch {
    return [];
  }
}
export const visited = $state<{ list: Visit[] }>({ list: readHistory() });

/** The browser's own error page or AgentDeck's: never an address of a tab, never kept in the history. */
export const isErrorUrl = (url: string) => /^chrome-error:|^about:neterror|^https?:\/\/agentdeck\.localhost\/|^agentdeck:\/\/|agentdeck-error/i.test(url);

/** One entry per address (the part after # does not make another); pages that are not web pages are left out. */
export function recordVisit(url: string, title: string) {
  if (!originOf(url) || isErrorUrl(url)) return;
  const clean = url.split('#')[0];
  const found = visited.list.find((item) => item.url === clean);
  const now = Date.now();
  if (found) {
    // The title arrives a moment after the address: only a new address counts as a visit.
    if (now - found.last > 3000) found.visits += 1;
    found.last = now;
    if (title) found.title = title;
  } else {
    visited.list.push({ url: clean, title, visits: 1, last: now });
    if (visited.list.length > HISTORY_MAX) {
      visited.list.sort((a, b) => b.last - a.last);
      visited.list.length = HISTORY_MAX;
    }
  }
  try {
    localStorage.setItem(HISTORY_KEY, JSON.stringify(visited.list));
  } catch {
    // Not kept: the list still works for this session.
  }
}

/** History entries matching what was typed, the best first: how often and how recently they were visited decide. */
export function matchHistory(query: string, limit = 5): Visit[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (!words.length) return [];
  const now = Date.now();
  return visited.list
    .map((item) => {
      const url = item.url.replace(/^https?:\/\/(www\.)?/, '').toLowerCase();
      const title = item.title.toLowerCase();
      if (!words.every((word) => url.includes(word) || title.includes(word))) return { item, score: -1 };
      const days = (now - item.last) / 86400000;
      const prefix = words.length === 1 && url.startsWith(words[0]) ? 2 : 1;
      return { item, score: prefix * (1 + Math.log2(item.visits)) / (1 + days / 7) };
    })
    .filter((entry) => entry.score >= 0)
    .sort((a, b) => b.score - a.score)
    .slice(0, limit)
    .map((entry) => entry.item);
}

export function forgetVisit(url: string) {
  const index = visited.list.findIndex((item) => item.url === url);
  if (index === -1) return;
  visited.list.splice(index, 1);
  try {
    localStorage.setItem(HISTORY_KEY, JSON.stringify(visited.list));
  } catch {
    // Not kept.
  }
}

/** What Google suggests for a search being typed (empty when offline). */
export const searchSuggestions = (query: string) => (inTauri() ? call<string[]>('browser_suggest', { query }).catch(() => [] as string[]) : Promise.resolve([] as string[]));

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
  try {
    await call('browser_show', { tab: tab.id, url: tab.url, userAgent: DEVICES[browser.device].ua ?? null, x: rect.x, y: rect.y, w: rect.w, h: rect.h });
    tab.opened = true;
  } catch (error) {
    tab.loading = false;
    browser.navError = String(error);
    throw error;
  }
}
export const moveBrowser = (rect: Rect) => call('browser_bounds', { tab: activeTab().id, x: rect.x, y: rect.y, w: rect.w, h: rect.h });
export const hideBrowser = () => call('browser_hide');
/** The panel is not on screen: the front page keeps running off-screen (a hidden page stops drawing and cannot be photographed). */
export const parkBrowser = () => call('browser_park').catch(() => {});
/** At start-up (also after F5): closes the pages a previous load of the window left running above it. */
// Not from the Preview's own copy of the app: it would close the very pages it is shown in.
export const resetBrowser = () => (inTauri() && !isPreviewCopy() ? call('browser_reset').catch(() => {}) : Promise.resolve());
/** A picture of the front page taken by the engine (PNG data URL). */
export const capturePage = (scale = 1, fullPage = false) => call<string>('browser_capture', { tab: activeTab().id, scale, fullPage });
export const zoomBrowser = (factor: number) => call('browser_zoom', { tab: activeTab().id, factor });

/** The address as it came: colour codes copied from a server's log and the spaces around it are not part of it. */
export const cleanAddress = (url: string) => stripAnsi(url).trim();

export async function goTo(address: string) {
  const url = cleanAddress(address);
  const tab = activeTab();
  tab.url = url;
  browser.navError = '';
  if (tab.opened) {
    tab.loading = true;
    try {
      await call('browser_navigate', { tab: tab.id, url });
    } catch (error) {
      tab.loading = false;
      browser.navError = String(error);
      throw error;
    }
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
export const stop = () => {
  const tab = activeTab();
  tab.loading = false;
  if (!tab.opened) return Promise.resolve();
  return call('browser_eval', { tab: tab.id, script: 'window.stop()' });
};

/** The pages are created again with the new device's user agent (it cannot change on a live page), at the same address. */
export async function recreateBrowser() {
  dropPageModes();
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

/** A PNG of one element of a tab, cut from the page at its box. Undefined when the page cannot be captured (or does not answer in time): the element is then added without a picture. */
export async function captureElement(tab: string, box: PickedElement['box']): Promise<string | undefined> {
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    const timeout = new Promise<undefined>((resolve) => setTimeout(() => resolve(undefined), 5000));
    return await Promise.race([invoke<string>('browser_element_capture', { tab, x: box.x, y: box.y, w: box.w, h: box.h }), timeout]);
  } catch {
    return undefined;
  }
}

/** A PNG of the whole page as it is now, or undefined when it cannot be captured (or does not answer in time). */
export async function capturePageIfPossible(scale = 1, fullPage = false): Promise<string | undefined> {
  try {
    const timeout = new Promise<undefined>((resolve) => setTimeout(() => resolve(undefined), fullPage ? 45000 : scale > 1 ? 15000 : 5000));
    return await Promise.race([capturePage(scale, fullPage), timeout]);
  } catch {
    return undefined;
  }
}

/** A WebP file of one element of a tab (its path, see browser_element_capture_file), or undefined: no base64 is made. */
export async function captureElementFile(tab: string, box: PickedElement['box']): Promise<string | undefined> {
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    const timeout = new Promise<undefined>((resolve) => setTimeout(() => resolve(undefined), 5000));
    return await Promise.race([invoke<string>('browser_element_capture_file', { tab, x: box.x, y: box.y, w: box.w, h: box.h }), timeout]);
  } catch {
    return undefined;
  }
}

/** A WebP file of the whole page as it is now (its path, see browser_capture_file), or undefined when it cannot be captured. */
export async function capturePageFileIfPossible(): Promise<string | undefined> {
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    const timeout = new Promise<undefined>((resolve) => setTimeout(() => resolve(undefined), 5000));
    return await Promise.race([invoke<string>('browser_capture_file', { tab: activeTab().id, scale: 1, fullPage: false }), timeout]);
  } catch {
    return undefined;
  }
}

/** Starts a drag on the page and returns the selected rectangle in CSS pixels. Escape cancels the selection. */
export async function selectCaptureRegion(): Promise<{ box: Rect; viewport: { w: number; h: number } } | null> {
  if (browser.annotating) await stopAnnotating();
  if (browser.picking) await cancelPick();
  browser.annotating = true;
  browser.annotateTool = 'region';
  const selected = new Promise<{ box: Rect; viewport: { w: number; h: number } } | null>((resolve) => {
    regionSelectionResolve = resolve;
  });
  try {
    await ask('ann', { tool: 'region' });
    return await selected;
  } catch (error) {
    regionSelectionResolve = null;
    browser.annotating = false;
    browser.annotateCount = 0;
    throw error;
  }
}

/** Ends the pick that is waiting, here in the app, whether or not its page can still answer. */
let abortPick: (() => void) | null = null;

/** Lets the user click an element in the live page; resolves with it (and where it is), or null when cancelled. */
export async function pickElement(): Promise<{ element: PickedElement; url: string; title: string } | null> {
  browser.picking = true;
  // The page may go away mid-pick (another device recreates it, a reload, another tab): the pick then ends from here,
  // instead of waiting minutes for an answer that cannot come.
  const aborted = new Promise<null>((resolve) => (abortPick = () => resolve(null)));
  try {
    const report = await Promise.race([ask<Report & { element: PickedElement | null; url?: string; title?: string }>('pick', null, 300000), aborted]);
    return report?.element ? { element: report.element, url: report.url ?? '', title: report.title ?? '' } : null;
  } finally {
    abortPick = null;
    browser.picking = false;
  }
}

/** Ends inspecting: the wait in the app at once, then the page's own picker if the page is still there. */
export async function cancelPick() {
  abortPick?.();
  await run("window.__agentdeck && window.__agentdeck.run('', 'unpick')").catch(() => {});
}

/** The page is gone or replaced: the pick and the marks went with it, so both modes end here without asking it. */
function dropPageModes() {
  abortPick?.();
  regionSelectionResolve?.(null);
  regionSelectionResolve = null;
  browser.annotating = false;
  browser.annotateCount = 0;
  browser.annotateUndo = false;
  browser.annotateRedo = false;
  browser.annotateSelected = false;
}
/** Starts marking up the live page with a tool, or switches the tool. */
export async function annotate(tool: AnnotateTool) {
  browser.annotating = true;
  browser.annotateTool = tool;
  try {
    await ask('ann', { tool });
  } catch (error) {
    // A page can navigate or lose its injected bridge between enabling the toolbar and receiving the reply.
    // Do not leave the app showing an active markup mode when the page never started it.
    browser.annotating = false;
    browser.annotateCount = 0;
    browser.annotateUndo = false;
    browser.annotateRedo = false;
    browser.annotateSelected = false;
    throw error;
  }
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
  regionSelectionResolve?.(null);
  regionSelectionResolve = null;
  browser.annotating = false;
  browser.annotateCount = 0;
  browser.annotateUndo = false;
  browser.annotateRedo = false;
  browser.annotateSelected = false;
  await ask('ann', { clear: true }).catch(() => {});
}


/** Pins a note to an element. */
export function addComment(element: Pick<PickedElement, 'selector' | 'html' | 'box'>, note: string) {
  if (!note.trim()) return;
  browser.comments.push({ id: crypto.randomUUID(), selector: element.selector, html: element.html, box: element.box, note: note.trim() });
}

export const openExternal = (url: string) => call('open_external', { url });

/** Adds an unpacked Chrome extension to the persistent Windows Browser Preview profile. */
export async function addBrowserExtension(tab: string): Promise<string | null> {
  const path = await call<string | null>('browser_pick_extension');
  if (!path) return null;
  return call<string>('browser_add_extension', { tab, path });
}

/** An extension of the Preview profile (src-tauri/src/browser_extensions.rs). `id` is empty while no page is open. */
export type BrowserExtension = { id: string; name: string; enabled: boolean; popup: string | null; icon: string | null };
export const listBrowserExtensions = (tab: string) => call<BrowserExtension[]>('browser_extensions', { tab });
export const enableBrowserExtension = (tab: string, id: string, enabled: boolean) => call('browser_extension_enable', { tab, id, enabled });
export const reloadBrowserExtension = (tab: string, id: string) => call('browser_extension_reload', { tab, id });
export const removeBrowserExtension = (tab: string, id: string) => call('browser_extension_remove', { tab, id });

/** A local address (a dev server) is opened in the AgentDeck browser instead of leaving the app. */
export const isLocalUrl = (url: string) => /^https?:\/\/(localhost|127\.0\.0\.1|\[::1\])(:\d+)?(\/|$)/i.test(url);

/** Shows `url` in the browser: in the front tab when that is blank, in a new tab otherwise. */
/**
 * `auto` (a link): a page already open in a tab is shown as it is; any other address gets a tab of its own, never the
 * open page's. `current` (the agent): the front tab goes to the address. `new`: a tab of its own.
 */
export function openInPreview(address: string, mode: 'auto' | 'current' | 'new' = 'auto') {
  const url = cleanAddress(address);
  if (mode === 'new') newTab(url);
  else if (mode === 'auto') {
    const shown = browser.tabs.find((tab) => tab.url === url);
    if (shown) browser.activeId = shown.id;
    else if (activeTab().url) newTab(url);
    else activeTab().url = url;
  }
  browser.autostart = url;
  layout.activeTool = 'Preview';
  setToolsOpen(true);
}

/**
 * The agent (browser_open, browser_serve) opens an address. With the Preview on screen it opens there, as a link would.
 * Otherwise the Preview panel is left alone: the page is created off-screen, where it keeps drawing, so the agent can
 * still look at it, click, read the console and take screenshots; the user finds it in the Preview when they open it.
 */
function openForAgent(address: string, inNewTab: boolean, ownTab?: string) {
  const url = cleanAddress(address);
  // A chat that drives a tab of its own opens the page there and leaves the tab in front as it is.
  const own = ownTab ? browser.tabs.find((tab) => tab.id === ownTab) : undefined;
  if (own) {
    own.url = url;
    if (own.opened) void call('browser_navigate', { tab: own.id, url }).catch(() => {});
    return;
  }
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
export type RunTarget = { name: string; command: string; cwd: string; url: string | null; source: string; /** The script file it runs (`node scripts/dev.mjs`). */ wrapper: string | null; /** That script asks questions: it needs a terminal. */ prompts: boolean };

/** Why a start did not bring a page up (see `RunFailure` in detect.rs), with what was started. */
export type RunFailure = { kind: string; message: string; log: string; hint: string | null; interactive: boolean; wrapper: string | null };
export type RunProblem = RunFailure & { name: string; command: string; cwd: string };

/** The failure a rejected start carries (a plain error text is not one). */
export function asRunFailure(error: unknown): RunFailure | null {
  if (!error || typeof error !== 'object') return null;
  const found = error as Partial<RunFailure>;
  return typeof found.message === 'string' && typeof found.kind === 'string' ? { kind: found.kind, message: found.message, log: found.log ?? '', hint: found.hint ?? null, interactive: found.interactive === true, wrapper: found.wrapper ?? null } : null;
}

/** The failure as one text, for the Audit log. */
export const failureText = (failure: RunFailure) => [failure.message, failure.hint, failure.log && `Last lines of its log:\n${failure.log}`].filter(Boolean).join('\n');

/** A command for the Terminal panel's shell: first into the folder it belongs to. */
export function terminalCommand(command: string, cwd: string): string {
  if (!cwd) return command;
  return /windows/i.test(navigator.userAgent) ? `Set-Location -LiteralPath '${cwd.replaceAll("'", "''")}'; ${command}` : `cd '${cwd.replaceAll("'", "'\\''")}' && ${command}`;
}

/** The ways the project in `cwd` can be run, read from its own files (scripts, launch.json, manage.py). */
export const detectTargets = (cwd: string) => (inTauri() && cwd ? call<RunTarget[]>('detect_run_targets', { cwd }) : Promise.resolve([] as RunTarget[]));

/** Starts the project as a server AgentDeck owns and returns the address once it answers. */
export const runTarget = (target: RunTarget) => call<string>('run_target', { command: target.command, cwd: target.cwd, url: target.url });

// ---- the servers running in the background ----
export type ServerInfo = { key: number; command: string; cwd: string; url: string | null; pid: number; age: number; running: boolean };
/** The dev servers AgentDeck started (from the Preview or by an agent), kept in step by `refreshServers`. */
/** `at` is when the list was read: a running server's age grows from there between two readings. */
export const servers = $state<{ list: ServerInfo[]; at: number }>({ list: [], at: Date.now() });

export async function refreshServers() {
  if (!inTauri()) return;
  try {
    servers.list = await call<ServerInfo[]>('servers_list');
    servers.at = Date.now();
  } catch {
    // The list stays as it was.
  }
}

/** Stops one server (its key), or all of them, with every process they started. */
export async function stopServers(key: number | null) {
  await call('server_stop', { key });
  await refreshServers();
}
