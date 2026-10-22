// State and requests of the AgentDeck browser (see src-tauri/src/browser.rs). The page runs in a child webview;
// a script injected into it reports console, network and navigation, and answers DOM / screenshot / pick requests.
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
export type PageSnapshot = { url: string; title: string; viewport: { w: number; h: number; dpr: number }; html: string };

const LIMIT = 300;
export const browser = $state({
  url: 'http://localhost:3000',
  title: '',
  device: 'desktop' as Device,
  landscape: false,
  opened: false,
  picking: false,
  picked: null as PickedElement | null,
  console: [] as ConsoleLine[],
  network: [] as NetworkLine[],
  error: '',
  /** Open the page at once when the browser view mounts (used by the developer hook). */
  autostart: false,
});

async function call<T = void>(command: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<T>(command, args);
}

// ---- reports from the page ----
type Report = { kind: string; id?: string; [key: string]: unknown };
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
    if (payload.kind === 'console') {
      browser.console.push({ level: String(payload.level), text: String(payload.text), at: Number(payload.at) });
      if (browser.console.length > LIMIT) browser.console.splice(0, browser.console.length - LIMIT);
    } else if (payload.kind === 'network') {
      browser.network.push({ method: String(payload.method), url: String(payload.url), status: Number(payload.status), ms: Number(payload.ms), at: Number(payload.at) });
      if (browser.network.length > LIMIT) browser.network.splice(0, browser.network.length - LIMIT);
    } else if (payload.kind === 'nav') {
      browser.url = String(payload.url);
      browser.title = String(payload.title ?? '');
    }
  });
}

/** Asks the page's injected script for something and waits for its answer. */
function ask<T extends Report>(kind: string, timeout = 15000): Promise<T> {
  const id = crypto.randomUUID();
  return new Promise<T>((resolve, reject) => {
    const timer = setTimeout(() => {
      waiting.delete(id);
      reject(new Error(`The page did not answer (${kind}). Is a page loaded?`));
    }, timeout);
    waiting.set(id, (report) => {
      clearTimeout(timer);
      resolve(report as T);
    });
    call('browser_eval', { script: `window.__agentdeck && window.__agentdeck.run(${JSON.stringify(id)}, ${JSON.stringify(kind)})` }).catch((error: unknown) => {
      clearTimeout(timer);
      waiting.delete(id);
      reject(new Error(String(error)));
    });
  });
}

// ---- commands ----
export type Rect = { x: number; y: number; w: number; h: number };

export async function showBrowser(rect: Rect) {
  await call('browser_show', { url: browser.url, userAgent: DEVICES[browser.device].ua ?? null, x: rect.x, y: rect.y, w: rect.w, h: rect.h });
  browser.opened = true;
}
export const moveBrowser = (rect: Rect) => call('browser_bounds', { x: rect.x, y: rect.y, w: rect.w, h: rect.h });
export const hideBrowser = () => call('browser_hide');
export const zoomBrowser = (factor: number) => call('browser_zoom', { factor });
export async function goTo(url: string) {
  browser.url = url;
  if (browser.opened) await call('browser_navigate', { url });
}
export const back = () => call('browser_eval', { script: 'history.back()' });
export const forward = () => call('browser_eval', { script: 'history.forward()' });
export const reload = () => call('browser_eval', { script: 'location.reload()' });

export async function snapshot(): Promise<PageSnapshot> {
  const report = await ask<Report & PageSnapshot>('dom');
  return { url: report.url, title: report.title, viewport: report.viewport, html: report.html };
}

/** A screenshot of what the page shows now, as a PNG data URL, with the size it was taken at. */
export async function screenshot(): Promise<{ data: string; w: number; h: number }> {
  const report = await ask<Report & { data?: string; w: number; h: number; error?: string }>('shot', 30000);
  if (!report.data) throw new Error(report.error ?? 'screenshot failed');
  return { data: report.data, w: report.w, h: report.h };
}

/** Lets the user click an element in the page; resolves with it, or null when cancelled. */
export async function pickElement(): Promise<PickedElement | null> {
  browser.picking = true;
  try {
    const report = await ask<Report & { element: PickedElement | null }>('pick', 120000);
    browser.picked = report.element;
    return report.element;
  } finally {
    browser.picking = false;
  }
}
export const cancelPick = () => call('browser_eval', { script: "window.__agentdeck && window.__agentdeck.run('', 'unpick')" });

/** A local address (a dev server) is opened in the AgentDeck browser instead of leaving the app. */
export const isLocalUrl = (url: string) => /^https?:\/\/(localhost|127\.0\.0\.1|\[::1\])(:\d+)?(\/|$)/i.test(url);

export function openInPreview(url: string) {
  browser.url = url;
  browser.autostart = true;
  layout.activeTool = 'Preview';
  layout.toolsOpen = true;
}

/** The page is created again with the new device's user agent (it cannot change on a live page), at the same address. */
export async function recreateBrowser() {
  if (!browser.opened) return;
  await call('browser_close');
  browser.opened = false;
}
