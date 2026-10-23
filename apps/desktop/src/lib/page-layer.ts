// The Preview page and what the app opens over it.
//
// The page is a native webview: HTML of the app can never be drawn above it. So the app's own webview is see-through
// where the page is (every background above the page leaves a hole there), and whenever something of the app opens
// over the page (a modal, the search palette, a menu, a tooltip, a toast, a tool window) the page goes below the
// app. That thing is then drawn by the app itself, over the live page, with its own animations, exactly as
// anywhere else. Only the stacking order changes: instant, nothing is copied or captured.

import { activeTab } from './browser.svelte';

async function call(command: string, args?: Record<string, unknown>) {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke(command, args);
}

/** What the app opens above everything: modal and drawer panes, menus, tooltips, dialogs, toasts. */
const OVER = '.z-overlay-pane, [data-z-popover], [role="dialog"], [role="alertdialog"], [role="tooltip"], [role="region"][aria-live] > * > *';

/** Whether something of the app lies over `page` (window coordinates). `stage` holds the page itself. */
export function overPage(stage: HTMLElement, page: DOMRect): boolean {
  for (const element of document.querySelectorAll<HTMLElement>(OVER)) {
    // The window that holds the Preview, and anything inside the Preview itself, are not over it.
    if (element.contains(stage) || stage.parentElement?.contains(element)) continue;
    // A modal or drawer pane covers the whole app with its backdrop.
    if (element.classList.contains('z-overlay-pane')) {
      if (element.childElementCount > 0) return true;
      continue;
    }
    const r = element.getBoundingClientRect();
    if (r.width > 0 && r.height > 0 && r.right > page.left && r.left < page.right && r.bottom > page.top && r.top < page.bottom) return true;
  }
  return false;
}

const BACKGROUND = ['backgroundColor', 'backgroundImage', 'backgroundRepeat', 'backgroundPosition', 'backgroundSize'] as const;
/** The elements given a hole: the colour they paint, and their own inline background, given back after. */
const holed = new Map<HTMLElement, { color: string; inline: string[] }>();

/**
 * Leaves a hole in every background above the page, at `rect` (window coordinates), so the page shows through the
 * app when it is below it; null fills them again (no page: the app must not be see-through). A background is
 * redrawn as four bands around the hole, not masked: what is drawn over the page (a menu, a modal) is untouched.
 */
export function cutHole(stage: HTMLElement | null, rect: { x: number; y: number; w: number; h: number } | null) {
  const keep = new Set<HTMLElement>();
  if (stage && rect) {
    for (let element: HTMLElement | null = stage; element; element = element.parentElement) {
      const color = holed.get(element)?.color ?? getComputedStyle(element).backgroundColor;
      if (!color || color === 'transparent' || color === 'rgba(0, 0, 0, 0)') continue;
      if (!holed.has(element)) holed.set(element, { color, inline: BACKGROUND.map((name) => element.style[name]) });
      keep.add(element);
      const box = element.getBoundingClientRect();
      const x = Math.max(0, rect.x - box.left - element.clientLeft);
      const y = Math.max(0, rect.y - box.top - element.clientTop);
      const right = x + rect.w;
      const bottom = y + rect.h;
      const fill = `linear-gradient(${color}, ${color})`;
      element.style.backgroundColor = 'transparent';
      element.style.backgroundImage = [fill, fill, fill, fill].join(', ');
      element.style.backgroundRepeat = 'no-repeat';
      element.style.backgroundPosition = `0 0, 0 ${bottom}px, 0 ${y}px, ${right}px ${y}px`;
      element.style.backgroundSize = `100% ${y}px, 100% calc(100% - ${bottom}px), ${x}px ${rect.h}px, calc(100% - ${right}px) ${rect.h}px`;
    }
  }
  for (const [element, { inline }] of [...holed]) {
    if (keep.has(element)) continue;
    holed.delete(element);
    BACKGROUND.forEach((name, index) => (element.style[name] = inline[index]));
  }
}

let below = false;
/** Puts the pages below the app (something is open over the page) or back above it. */
export function setBelow(value: boolean) {
  if (value === below) return;
  below = value;
  void call('browser_layer', { below: value }).catch(() => {});
}

/** The blur of an open modal's backdrop, if it has one: the app cannot blur the page under it, the page can. */
export function modalBlur(): string {
  for (const pane of document.querySelectorAll<HTMLElement>('.z-overlay-pane')) {
    const backdrop = pane.querySelector<HTMLElement>('.z-overlay-backdrop');
    const filter = backdrop ? getComputedStyle(backdrop).backdropFilter : 'none';
    if (filter && filter !== 'none') return filter;
  }
  return '';
}

let shaded = { tab: '', filter: '' };
/** Blurs the page like the open modal blurs the app (its dim is the app's own, drawn over the page). */
export function blurPage(filter: string) {
  const tab = filter ? activeTab().id : shaded.tab;
  if (!tab || (filter === shaded.filter && tab === shaded.tab)) return;
  shaded = { tab: filter ? tab : '', filter };
  const look = filter ? JSON.stringify(`background:transparent;backdrop-filter:${filter};-webkit-backdrop-filter:${filter}`) : 'null';
  void call('browser_eval', { tab, script: `window.__agentdeck && window.__agentdeck.shade && window.__agentdeck.shade(${look})` }).catch(() => {});
}
