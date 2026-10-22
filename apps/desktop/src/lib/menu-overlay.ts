// Menus that open over a browser page. The page is a native layer above the app, so HTML can never be drawn on
// top of it. Such a menu is mirrored into a transparent webview stacked above the page (see overlay.rs): the same
// markup with the app's styles, at the same place. The real menu stays in the DOM, invisible, and receives the
// clicks made on the mirror. The page underneath stays live.

import { inTauri } from './history';

const MENUS = '[data-z-popover]:not(.z-tooltip-host)';
const PICK = 'button,a,[role=menuitem],[role=menuitemradio],[role=menuitemcheckbox],[role=option],[role=radio],[role=checkbox]';
/** Room around the menu for its shadow. */
const PAD = 16;

async function call(command: string, args?: Record<string, unknown>) {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke(command, args);
}
const run = (script: string) => call('overlay_eval', { script });

let styled = false;
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

/**
 * Keeps menus that overlap `area()` mirrored above the page. `area` returns the page's rectangle, or null when no
 * page is on screen. Returns a stop function.
 */
export function mirrorMenus(area: () => DOMRect | null): () => void {
  if (!inTauri()) return () => {};
  let current: HTMLElement | null = null;
  let lastKey = '';
  let shown = false;
  let frame = 0;
  /** Where the overlay sits in the window. */
  let at = { x: 0, y: 0 };

  let fresh = false;
  const hide = () => {
    // A closing menu stays unseen while it fades out; the mirror plays the closing instead.
    if (!current?.inert) current?.removeAttribute('data-mirrored');
    current = null;
    lastKey = '';
    // Emptied before it is hidden: shown again later, it must not flash the previous menu.
    if (shown) void call('overlay_hide').catch(() => {});
    shown = false;
  };

  const tick = () => {
    frame = requestAnimationFrame(tick);
    const page = area();
    const menu = page
      ? [...document.querySelectorAll<HTMLElement>(MENUS)].find((item) => {
          // Svelte marks a menu that is playing its closing animation `inert`: it is closed already.
          if (item.inert) return false;
          const r = item.getBoundingClientRect();
          return r.width > 0 && r.right > page.left && r.left < page.right && r.bottom > page.top && r.top < page.bottom;
        })
      : undefined;
    if (!menu) {
      if (current) hide();
      return;
    }
    if (menu !== current) {
      current?.removeAttribute('data-mirrored');
      current = menu;
      fresh = true;
      // z-hover-glide makes its highlight box on the first hover; make it now so the mirror has it to move.
      for (const host of menu.querySelectorAll<HTMLElement>('[data-z-hover-glide]')) {
        const item = host.querySelector('[data-glide-item]:not([aria-checked="true"])');
        if (!item || host.querySelector(':scope > [data-z-hover-glide-highlight]')) continue;
        item.dispatchEvent(new PointerEvent('pointerover', { bubbles: true }));
        item.dispatchEvent(new PointerEvent('pointerout', { bubbles: true, relatedTarget: document.body }));
      }
      sendStyles();
    }
    menu.setAttribute('data-mirrored', '');
    // Where the menu ends up, not where its opening animation has it now: the mirror plays its own animation.
    const r = menu.getBoundingClientRect();
    const w = menu.offsetWidth || r.width;
    const h = menu.offsetHeight || r.height;
    const left = parseFloat(menu.style.left);
    const top = parseFloat(menu.style.top);
    const x = Number.isFinite(left) ? left : r.left + r.width / 2 - w / 2;
    const y = Number.isFinite(top) ? top : r.top + r.height / 2 - h / 2;
    // The glide highlight moves on its own in the mirror; its position here is left out of the copy.
    const html = menu.outerHTML.replace(/(data-z-hover-glide-highlight=""[^>]*?) style="[^"]*"/g, '$1');
    const key = `${Math.round(x)},${Math.round(y)},${w},${h}|${document.documentElement.className}|${html}`;
    if (key === lastKey) return;
    const moved = !lastKey.startsWith(`${Math.round(x)},${Math.round(y)},${w},${h}|`);
    lastKey = key;
    const attrs = { html: attributes(document.documentElement), body: document.body.className };
    const opening = fresh;
    fresh = false;
    const origin = menu.style.transformOrigin || 'center top';
    const script = `window.__overlay && window.__overlay.render(${JSON.stringify(html)}, ${JSON.stringify(attrs)}, ${PAD}, ${PAD}, ${w}, ${opening ? JSON.stringify(origin) : 'null'})`;
    if (moved || !shown) {
      shown = true;
      at = { x: x - PAD, y: y - PAD };
      void call('overlay_show', { script, x: x - PAD, y: y - PAD, w: w + PAD * 2, h: h + PAD * 2 }).catch(() => {});
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
        // A press on the page is a press outside the menu: it closes as it would on any outside click.
        if (current) document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, composed: true }));
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
    hide();
    unlisten?.();
  };
}
