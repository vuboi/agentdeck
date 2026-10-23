// Menus that open over a browser page. The page is a native layer above the app, so HTML can never be drawn on
// top of it. Such a menu is mirrored into a transparent webview stacked above the page (see overlay.rs): the same
// markup with the app's styles, at the same place. The real menu stays in the DOM, invisible, and receives the
// clicks made on the mirror. The page underneath stays live.

import { activeTab } from './browser.svelte';
import { inTauri } from './history';

const MENUS = '[data-z-popover]:not(.z-tooltip-host)';
/**
 * Dialogs mirrored the same way, with a dimmed backdrop over the page: the search palette, a modal, a confirm. The
 * page stays shown and live under them, so opening one never makes it blink. A full-screen viewer (the gallery)
 * is not one of them: the page gives way to a picture of it instead (BrowserView).
 */
export const MIRRORED_DIALOGS = '.z-overlay-pane[data-kind="modal"] > .z-overlay-container > [aria-modal="true"]:not(.z-gallery-preview)';
const PICK = 'button,a,[role=menuitem],[role=menuitemradio],[role=menuitemcheckbox],[role=option],[role=radio],[role=checkbox]';
/** Room around the menu for its shadow. */
const PAD = 16;

async function call(command: string, args?: Record<string, unknown>) {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke(command, args);
}
const run = (script: string) => call('overlay_eval', { script });

let styled = false;
let reloaded = false;
/** The app's styles, sent once: the mirror then looks exactly like the menu. */
function sendStyles() {
  if (styled) return;
  styled = true;
  const css = [...document.styleSheets]
    .map((sheet) => {
      try {
        return [...sheet.cssRules].map((rule) => rule.cssText).join('\n');
      } catch {
        return '';
      }
    })
    .join('\n');
  void run(`window.__overlay && window.__overlay.css(${JSON.stringify(css)})`);
}

function attributes(element: Element) {
  return Object.fromEntries(element.getAttributeNames().map((name) => [name, element.getAttribute(name) ?? '']));
}

/** The markup of `element` with what markup leaves out: what is typed in its fields, how far its lists are scrolled. */
function copyOf(element: HTMLElement): string {
  const copy = element.cloneNode(true) as HTMLElement;
  const from = [element, ...element.querySelectorAll<HTMLElement>('*')];
  const to = [copy, ...copy.querySelectorAll<HTMLElement>('*')];
  from.forEach((node, index) => {
    const twin = to[index];
    if (node instanceof HTMLTextAreaElement) twin.textContent = node.value;
    else if (node instanceof HTMLInputElement) twin.setAttribute('value', node.value);
    if (node.scrollTop > 0) twin.setAttribute('data-mirror-scroll', String(Math.round(node.scrollTop)));
  });
  return copy.outerHTML;
}

type Backdrop = { color: string; filter: string };

/** How a dialog's backdrop looks (its colour and blur), to draw the same over the page. */
function backdropOf(dialog: HTMLElement): Backdrop {
  const backdrop = dialog.closest('.z-overlay-pane')?.querySelector<HTMLElement>('.z-overlay-backdrop');
  const style = backdrop ? getComputedStyle(backdrop) : null;
  const color = style?.backgroundColor ?? '';
  return {
    color: color && color !== 'rgba(0, 0, 0, 0)' ? color : 'rgba(0, 0, 0, 0.4)',
    filter: style && style.backdropFilter !== 'none' ? style.backdropFilter : '',
  };
}

/** The tab whose page carries a dialog's backdrop. */
let shadedTab = '';
/**
 * Draws a dialog's backdrop inside the page (null: takes it away). An overlay webview is transparent but cannot blur
 * what is under it; the page itself can. It fades like the app's backdrop and takes the presses meant for the page:
 * they close the dialog, as a press on the backdrop does.
 */
function shadePage(backdrop: Backdrop | null) {
  const tab = backdrop ? activeTab().id : shadedTab;
  if (!tab) return;
  shadedTab = backdrop ? tab : '';
  const look = backdrop ? JSON.stringify(`background:${backdrop.color};backdrop-filter:${backdrop.filter || 'none'};-webkit-backdrop-filter:${backdrop.filter || 'none'}`) : 'null';
  // The page's script draws it (inject.js): a one-line call, run as soon as the page gets it.
  void call('browser_eval', { tab, script: `window.__agentdeck && window.__agentdeck.shade && window.__agentdeck.shade(${look})` }).catch(() => {});
}

/**
 * Keeps menus that overlap `area()` mirrored above the page. `area` returns the page's rectangle, or null when no
 * page is on screen. Returns a stop function.
 */
export function mirrorMenus(area: () => DOMRect | null): () => void {
  if (!inTauri()) return () => {};
  // The overlay lives as long as the app process, across reloads of the app: load its page again so it is the one
  // this code goes with, and send the styles again.
  if (!reloaded) {
    reloaded = true;
    void run('location.reload()').catch(() => {});
  }
  let current: HTMLElement | null = null;
  /** The mirrored markup, where the overlay was put and the theme it was drawn in: what the overlay has now. */
  let html = '';
  let lastPlace = '';
  let lastTheme = '';
  let shown = false;
  /** The menu changed since it was last copied: a mutation, typing in its field, scrolling its list. */
  let dirty = true;
  const markDirty = () => (dirty = true);
  let watcher: MutationObserver | null = null;
  const watch = (menu: HTMLElement) => {
    dirty = true;
    watcher = new MutationObserver(markDirty);
    watcher.observe(menu, { subtree: true, childList: true, attributes: true, characterData: true });
    menu.addEventListener('input', markDirty, true);
    menu.addEventListener('scroll', markDirty, true);
  };
  const unwatch = () => {
    watcher?.disconnect();
    watcher = null;
    current?.removeEventListener('input', markDirty, true);
    current?.removeEventListener('scroll', markDirty, true);
  };
  let frame = 0;
  /** Where the overlay sits in the window. */
  let at = { x: 0, y: 0 };

  let fresh = false;
  /** The dialog whose backdrop is drawn in the page. */
  let opened: HTMLElement | null = null;
  const hide = () => {
    if (opened) {
      opened = null;
      shadePage(null);
    }
    // A closing menu stays unseen while it fades out; the mirror plays the closing instead.
    if (!current?.inert) current?.removeAttribute('data-mirrored');
    unwatch();
    current = null;
    lastPlace = '';
    html = '';
    // Emptied before it is hidden: shown again later, it must not flash the previous menu.
    if (shown) void call('overlay_hide').catch(() => {});
    shown = false;
  };

  const tick = () => {
    frame = requestAnimationFrame(tick);
    update();
  };
  // A dialog or menu put in the DOM is mirrored at once, not on the next frame.
  // Cheap while nothing is open (a chat streaming in changes the DOM all the time): only a new one is looked at.
  const added = new MutationObserver(() => {
    if (!current && document.querySelector(`${MIRRORED_DIALOGS}, ${MENUS}`)) update();
  });
  added.observe(document.body, { childList: true, subtree: true });

  const update = () => {
    const page = area();
    // An open dialog dims the whole app, the page included, whether or not it lies over the page.
    const dialog = page ? [...document.querySelectorAll<HTMLElement>(MIRRORED_DIALOGS)].findLast((item) => !item.inert) : undefined;
    const menu =
      dialog ??
      (page
        ? [...document.querySelectorAll<HTMLElement>(MENUS)].find((item) => {
            // Svelte marks a menu that is playing its closing animation `inert`: it is closed already.
            if (item.inert) return false;
            const r = item.getBoundingClientRect();
            return r.width > 0 && r.right > page.left && r.left < page.right && r.bottom > page.top && r.top < page.bottom;
          })
        : undefined);
    if (!menu) {
      if (current) hide();
      return;
    }
    if (menu !== current) {
      current?.removeAttribute('data-mirrored');
      unwatch();
      current = menu;
      fresh = true;
      watch(menu);
      // z-hover-glide makes its highlight box on the first hover; make it now so the mirror has it to move.
      for (const host of menu.querySelectorAll<HTMLElement>('[data-z-hover-glide]')) {
        const item = host.querySelector('[data-glide-item]:not([aria-checked="true"])');
        if (!item || host.querySelector(':scope > [data-z-hover-glide-highlight]')) continue;
        item.dispatchEvent(new PointerEvent('pointerover', { bubbles: true }));
        item.dispatchEvent(new PointerEvent('pointerout', { bubbles: true, relatedTarget: document.body }));
      }
      sendStyles();
    }
    if (!menu.hasAttribute('data-mirrored')) menu.setAttribute('data-mirrored', '');
    // Where the menu ends up, not where its opening animation has it now: the mirror plays its own animation.
    const r = menu.getBoundingClientRect();
    const w = menu.offsetWidth || r.width;
    const h = menu.offsetHeight || r.height;
    let x: number;
    let y: number;
    const holder = menu.offsetParent?.getBoundingClientRect();
    if (dialog && holder) {
      // A dialog is laid out in its container (its own top / left are offsets, 0): its place without the opening
      // transform is the container's plus its offset.
      x = holder.left + menu.offsetLeft;
      y = holder.top + menu.offsetTop;
    } else {
      const left = parseFloat(menu.style.left);
      const top = parseFloat(menu.style.top);
      x = Number.isFinite(left) ? left : r.left + r.width / 2 - w / 2;
      y = Number.isFinite(top) ? top : r.top + r.height / 2 - h / 2;
    }
    // Copied again only when it changed: copying a big dialog every frame is wasted work. The glide highlight
    // moves on its own in the mirror; its position here is left out of the copy.
    let changed = false;
    if (dirty) {
      dirty = false;
      const next = copyOf(menu).replace(/(data-z-hover-glide-highlight=""[^>]*?) style="[^"]*"/g, '$1');
      if (next !== html) {
        html = next;
        changed = true;
      }
    }
    // The overlay holds the menu with room for its shadow.
    const box = { x: x - PAD, y: y - PAD, w: w + PAD * 2, h: h + PAD * 2 };
    // A dialog's backdrop dims and blurs the page too: drawn inside the page, the only place that can blur it.
    if (dialog && opened !== dialog) {
      opened = dialog;
      shadePage(backdropOf(dialog));
    }
    const place = `${Math.round(box.x)},${Math.round(box.y)},${Math.round(box.w)},${Math.round(box.h)}`;
    const theme = document.documentElement.className;
    const moved = place !== lastPlace;
    if (!changed && !moved && theme === lastTheme) return;
    lastPlace = place;
    lastTheme = theme;
    const attrs = { html: attributes(document.documentElement), body: document.body.className };
    const opening = fresh;
    fresh = false;
    const origin = menu.style.transformOrigin || (dialog ? 'center center' : 'center top');
    const script = `window.__overlay && window.__overlay.render(${JSON.stringify(html)}, ${JSON.stringify(attrs)}, ${PAD}, ${PAD}, ${w}, ${opening ? JSON.stringify(origin) : 'null'})`;
    if (moved || !shown) {
      shown = true;
      at = { x: box.x, y: box.y };
      void call('overlay_show', { script, ...box }).catch(() => {});
    } else {
      void run(script).catch(() => {});
    }
  };
  frame = requestAnimationFrame(tick);

  // What happens on the mirror happens on the real menu.
  let unlisten: (() => void) | undefined;
  void import('@tauri-apps/api/event').then(({ listen }) =>
    listen<{ kind: string; index?: number; key?: string }>('browser-report', ({ payload }) => {
      if (payload.kind === 'page-pointer') {
        // A press on the page is a press outside the menu: it closes as it would on any outside click. For a dialog
        // it is a press on its backdrop (the container around the panel).
        const container = opened?.closest<HTMLElement>('.z-overlay-container');
        if (container) {
          container.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, composed: true }));
          container.click();
        } else if (current) document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, composed: true }));
        return;
      }
      if (payload.kind === 'overlay-pass') {
        // A press on the mirror's clear margin (room for its shadow) belongs to what is under it in the app:
        // the menu's own button most often, which then toggles the menu shut.
        const { x = 0, y = 0 } = payload as { x?: number; y?: number };
        const under = document.elementFromPoint(at.x + x, at.y + y);
        if (under instanceof HTMLElement) {
          under.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, composed: true }));
          (under.closest<HTMLElement>('button,a,[role=button],input') ?? under).click();
        }
        return;
      }
      if (payload.kind === 'overlay-ready') {
        // The overlay (re)loaded: its styles go now, so the first menu opens as fast as the next ones.
        styled = false;
        sendStyles();
        return;
      }
      if (payload.kind === 'overlay-hover') {
        const row = current && typeof payload.index === 'number' ? current.querySelectorAll<HTMLElement>(PICK)[payload.index] : undefined;
        row?.dispatchEvent(new PointerEvent('pointermove', { bubbles: true }));
        row?.dispatchEvent(new PointerEvent('pointerover', { bubbles: true }));
        return;
      }
      if (payload.kind === 'overlay-press') {
        // The overlay took the keyboard with that press: typing must go on in the real field.
        void call('overlay_return_focus').catch(() => {});
        return;
      }
      if (payload.kind === 'overlay-wheel') {
        const { x = 0, y = 0, dx = 0, dy = 0 } = payload as { x?: number; y?: number; dx?: number; dy?: number };
        let node = document.elementFromPoint(at.x + x, at.y + y) as HTMLElement | null;
        while (node && !(node.scrollHeight > node.clientHeight && /auto|scroll/.test(getComputedStyle(node).overflowY))) node = node.parentElement;
        node?.scrollBy({ left: dx, top: dy });
        return;
      }
      if (!current) return;
      const target = typeof payload.index === 'number' ? current.querySelectorAll<HTMLElement>(PICK)[payload.index] : undefined;
      if (payload.kind === 'overlay-click') target?.click();
      else if (payload.kind === 'overlay-key' && payload.key) {
        document.dispatchEvent(new KeyboardEvent('keydown', { key: payload.key, bubbles: true }));
      }
    }).then((stop) => (unlisten = stop)),
  );

  return () => {
    cancelAnimationFrame(frame);
    added.disconnect();
    hide();
    unlisten?.();
  };
}
