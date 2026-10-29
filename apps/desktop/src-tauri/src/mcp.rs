//! The AgentDeck browser as an MCP server (streamable HTTP, JSON answers), served by the bridge at `/mcp`.
//!
//! The agent CLI is started with it in `--mcp-config`, so the browser shows up among its own tools
//! (`mcp__agentdeck__browser_click`, ...) instead of as curl commands described in the prompt. Each tool is a
//! bridge route: the work itself stays in `bridge.rs`.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};
use tokio::sync::oneshot;

struct Tool {
    name: &'static str,
    route: &'static str,
    description: &'static str,
    /// (name, JSON type, description, required)
    args: &'static [(&'static str, &'static str, &'static str, bool)],
}

const TOOLS: &[Tool] = &[
    Tool { name: "browser_record_start", route: "/guide-record/start", description: "Starts an automatic continuous recording of the current Preview at up to 4 fps and 1.5× capture scale. Call before the walkthrough, operate the page, then call browser_record_stop to save a .webm in docs/user-guides/ under the project root.", args: &[("root", "string", "absolute path of the current project folder", true), ("file_name", "string", "output filename ending in .webm; defaults to a timestamped name", false)] },
    Tool { name: "browser_record_stop", route: "/guide-record/stop", description: "Stops the active guide recording and saves its .webm file in the project. Call after the walkthrough actions are complete.", args: &[] },
    Tool { name: "browser_zoom", route: "/zoom", description: "Changes the Preview page zoom for clear guide shots. Use a factor such as 1.25 to zoom in or 0.8 to zoom out; valid range is 0.25–3.0.", args: &[ ("factor", "number", "page zoom, where 1.0 is 100%", true) ] },
    Tool { name: "browser_page", route: "/page", description: "Address and title of the page open in AgentDeck's Preview panel (the browser the user sees).", args: &[] },
    Tool { name: "browser_screenshot", route: "/shot", description: "A picture of the Preview page as the user sees it now, menus included. Look before and after every action. With save_as, the PNG is also written to that file (for a guide, a document or a slide), and the answer says where.", args: &[("save_as", "string", "absolute path of a .png file to write the picture to; its folder is created", false)] },
    Tool { name: "ui_audit", route: "/eval", description: "Checks the page open in the Preview for common UI faults and answers each with a selector: horizontal overflow and elements past the edge, text with low contrast (WCAG), text under 12px, click targets under 24px, images without alt, buttons and links without a name, and colours or font sizes that are not part of the design tokens (too many one-off values). Run it after building or changing UI, fix what it reports, then run it again; use device mobile with browser_device to audit the phone layout too.", args: &[("selector", "string", "audit only inside this element (default: the whole page)", false), ("max", "number", "most issues listed (default 40)", false)] },
    Tool { name: "browser_annotate", route: "/eval", description: "Draws a callout on the page for a guide screenshot: a solid or dashed outline around an element (by number from browser_elements, visible text, or CSS selector) or around a rectangle, with an optional step number badge and a short label. The marks stay until clear is true; they are fixed to the screen, so draw them right before browser_screenshot and clear them before scrolling or clicking.", args: &[("index", "number", "number from browser_elements", false), ("text", "string", "visible text or label of the element", false), ("selector", "string", "CSS selector", false), ("x", "number", "rectangle left, CSS pixels (instead of an element)", false), ("y", "number", "rectangle top", false), ("w", "number", "rectangle width", false), ("h", "number", "rectangle height", false), ("label", "string", "short caption shown next to the outline", false), ("number", "number", "step number shown in a badge", false), ("style", "string", "solid (default: the control to use) or dashed (an area to look at)", false), ("color", "string", "CSS colour, default #ef4444", false), ("clear", "boolean", "remove every mark drawn so far (other arguments are ignored)", false)] },
    Tool { name: "browser_click", route: "/click", description: "Clicks at a point of the page, in CSS pixels of the screenshot. Real mouse events; reaches iframes.", args: &[("x", "number", "from the left of the page", true), ("y", "number", "from the top of the page", true)] },
    Tool { name: "browser_hover", route: "/hover", description: "Moves the mouse over a point (hover menus, tooltips).", args: &[("x", "number", "", true), ("y", "number", "", true)] },
    Tool { name: "browser_scroll", route: "/scroll", description: "Scrolls at a point, like a mouse wheel. dy > 0 scrolls down.", args: &[("x", "number", "", true), ("y", "number", "", true), ("dy", "number", "pixels down (negative: up)", false), ("dx", "number", "pixels right", false)] },
    Tool { name: "browser_type", route: "/type", description: "Types text into the focused field, like a keyboard. Click the field first.", args: &[("text", "string", "", true)] },
    Tool { name: "browser_key", route: "/key", description: "Presses a key: Enter, Tab, Escape, Backspace, Delete, ArrowUp/Down/Left/Right, Home, End, PageUp, PageDown, Space.", args: &[("key", "string", "", true)] },
    Tool { name: "browser_fill", route: "/fill", description: "Sets the value of the field matching a CSS selector and fires input and change.", args: &[("selector", "string", "", true), ("value", "string", "", true)] },
    Tool { name: "browser_device", route: "/device", description: "Switches the Preview between desktop, tablet and mobile (a phone or tablet user agent and screen size), to check a responsive layout. Answers with the viewport after the switch.", args: &[("mode", "string", "desktop, tablet or mobile", true), ("landscape", "boolean", "rotate the device", false)] },
    Tool { name: "browser_serve", route: "/serve", description: "Starts a local dev server that AgentDeck owns, waits until its address answers, then opens it in the Preview panel. Use it for every dev server: one started in your own shell dies when your command or turn ends. If the address already answers, it only opens it. The start command may ask questions (a menu, a port): AgentDeck answers them and moves to a free port if the address is taken. A failed start returns the end of the server's log.", args: &[("command", "string", "the start command, for example pnpm start:svelte", true), ("cwd", "string", "absolute path of the project folder", true), ("url", "string", "the address the server answers at, for example http://localhost:4301/", true), ("answers", "string", "answers to the questions the start command asks (a menu of projects, a port), in the order they come, separated by |: a number, or words from the option the user asked for (for example \"shop|4301\"). Left out: AgentDeck answers by itself (the first option, a free port, the default).", false), ("new_tab", "boolean", "true: open it in a tab of its own, leaving the current tab as it is; false: use the current tab. Left out: the current tab, unless another chat is driving it.", false)] },
    Tool { name: "browser_open", route: "/open", description: "Opens an address in AgentDeck's Preview panel (a tab of its own when another page is open) and waits until the page is ready. Use it right after starting a dev server, then screenshot and act in the same turn.", args: &[("url", "string", "full address, for example http://localhost:4301/", true), ("new_tab", "boolean", "true: open it in a tab of its own, leaving the current tab as it is (use it when another chat is driving the current tab, to compare, to debug side by side, or to look something up). false: use the current tab. Left out: the current tab, unless another chat is driving it.", false)] },
    Tool { name: "browser_drag", route: "/drag", description: "Drags one element onto another, as HTML5 drag and drop (dragstart, dragover, drop). Both are CSS selectors.", args: &[("from", "string", "CSS selector of the element to drag", true), ("to", "string", "CSS selector of the element to drop on", true)] },
    Tool { name: "browser_upload", route: "/upload", description: "Uploads a file from disk: sets it on an <input type=file>, or drops it on a drop zone. Give the absolute file path.", args: &[("selector", "string", "CSS selector of the file input or drop zone", true), ("path", "string", "absolute path of the file", true)] },
    Tool { name: "browser_eval", route: "/eval", description: "Runs JavaScript in the page (or in a frame, see browser_frames) as in the DevTools console and returns its value as JSON. Several statements are fine, top-level await works, and the value of the last expression is returned (write the expression, not `return`). Use it to read state, find an element's position with getBoundingClientRect(), or run a short scripted interaction and measure it (click, then sample the page over time with setTimeout and collect the numbers). Return small values: JSON.stringify a summary rather than whole objects or DOM nodes.", args: &[("script", "string", "JavaScript; statements and top-level await allowed, the last expression is the answer", true), ("frame", "string", "frame index or part of its address", false), ("timeout", "number", "seconds the script may run (default 20, at most 60)", false)] },
    Tool { name: "browser_text", route: "/eval", description: "The visible text of the page (its main or article part first, else the whole body), to read what it says without a screenshot.", args: &[("max_chars", "number", "characters kept (default 20000)", false)] },
    Tool { name: "browser_find", route: "/eval", description: "Finds elements of the page whose text, label, placeholder, title or role contains a query (case does not matter), anywhere on the page, not only in view. Each comes with a number, what it is and its position; use the number with browser_click_element.", args: &[("query", "string", "text to look for, for example Sign in or search", true)] },
    Tool { name: "browser_dom", route: "/dom", description: "HTML of the page, or of the elements matching a CSS selector.", args: &[("selector", "string", "", false), ("frame", "string", "", false)] },
    Tool { name: "browser_console", route: "/console", description: "Console lines and uncaught JavaScript errors of the page.", args: &[("level", "string", "error, warn or info", false), ("limit", "number", "", false)] },
    Tool { name: "browser_network", route: "/network", description: "Requests of the page; failed ones carry the start of the response body. With bodies, every request carries what was sent and the response (JSON, text), to write a DTO or an interface from a real response, or to see why a call failed.", args: &[("failed", "boolean", "only failed requests (status 0 or >= 400)", false), ("ok", "boolean", "only successful requests", false), ("url", "string", "only requests whose address contains this text, for example /api/users", false), ("bodies", "boolean", "include the request body and the response body", false), ("max_body", "number", "characters kept per body (default 2000, at most 20000)", false), ("limit", "number", "", false)] },
    Tool { name: "browser_frames", route: "/frames", description: "Frames of other origins in the page (sign-in, payment, embeds).", args: &[] },
    Tool { name: "browser_elements", route: "/eval", description: "Lists what can be clicked or filled in the visible part of the page (buttons, links, fields, menus), each with a number, its visible text and its position. Use the number with browser_click_element. Scroll with browser_scroll and call it again to see more. This is the reliable way to find things: prefer it to guessing coordinates from a screenshot.", args: &[] },
    Tool { name: "browser_click_element", route: "/click", description: "Clicks an element: by the number from browser_elements, or by its visible text, or by a CSS selector. It scrolls the element into view and clicks its centre with real mouse events. For a field, click it first, then browser_type.", args: &[("index", "number", "number from browser_elements", false), ("text", "string", "visible text or label of the element", false), ("selector", "string", "CSS selector", false)] },
];

/// Page script shared by the element tools: finds the open dialog, drawer or modal (if any), and names elements.
const PAGE_HELPERS: &str = r#"
const adShown = (e) => { const r = e.getBoundingClientRect(); const cs = getComputedStyle(e); return r.width > 20 && r.height > 20 && cs.visibility !== 'hidden' && cs.display !== 'none' && +cs.opacity > 0.05; };
const adOverlay = () => {
  const found = [...document.querySelectorAll('[role=dialog],[role=alertdialog],[aria-modal=true],dialog[open],[class*=drawer],[class*=modal],[class*=sheet],[class*=overlay],[class*=popover],[class*=dropdown-menu],[class*=menu-panel]')]
    .filter((e) => adShown(e) && /^(fixed|absolute|sticky)$/.test(getComputedStyle(e).position))
    .filter((e) => { const r = e.getBoundingClientRect(); return r.bottom > 0 && r.top < innerHeight && r.right > 0 && r.left < innerWidth; });
  const z = (e) => { let n = 0; for (let p = e; p && p !== document.body; p = p.parentElement) { const v = parseInt(getComputedStyle(p).zIndex, 10); if (!isNaN(v)) n = Math.max(n, v); } return n; };
  const roots = found.filter((e) => !found.some((o) => o !== e && o.contains(e)));
  roots.sort((a, b) => z(b) - z(a));
  return roots.find((e) => e.matches('[role=dialog],[role=alertdialog],[aria-modal=true],dialog[open]')) || roots[0] || null;
};
const adName = (el) => {
  const text = (el.getAttribute('aria-label') || el.innerText || el.value || el.placeholder || el.title || el.alt || el.name || el.querySelector('svg title')?.textContent || '').replace(/\s+/g, ' ').trim().slice(0, 70);
  if (text) return text;
  const hint = (el.className + ' ' + el.id + ' ' + (el.querySelector('svg')?.getAttribute('class') || '') + ' ' + (el.querySelector('[class*=icon]')?.className || '')).toString().match(/close|dismiss|cancel|back|menu|search|delete|trash|edit|plus|add|save|settings|more|check|copy|download|upload|chevron|arrow/i);
  return hint ? '(icon: ' + hint[0].toLowerCase() + ')' : '(icon button)';
};
const adDescribe = (ov) => ov ? (ov.getAttribute('aria-label') || ov.querySelector('h1,h2,h3,[class*=title]')?.innerText || ov.tagName.toLowerCase()).replace(/\s+/g, ' ').trim().slice(0, 60) : '';
"#;

/// Runs in the page: numbers the visible interactive elements and keeps them in `window.__adEls`.
/// When a dialog, drawer or menu is open, only what is inside it is listed: the page behind cannot be clicked.
const ELEMENTS_JS: &str = r#"(() => {
  __HELPERS__
  const sel = 'a[href],button,input,select,textarea,summary,[role=button],[role=link],[role=tab],[role=menuitem],[role=option],[role=checkbox],[role=switch],[onclick],[tabindex]:not([tabindex="-1"]),[contenteditable=true]';
  const ov = adOverlay();
  const scope = ov || document;
  window.__adEls = [];
  const out = [];
  const seen = new Set();
  for (const el of scope.querySelectorAll(sel)) {
    if (seen.has(el)) continue;
    seen.add(el);
    const r = el.getBoundingClientRect();
    const cs = getComputedStyle(el);
    if (r.width < 2 || r.height < 2 || cs.visibility === 'hidden' || cs.display === 'none' || +cs.opacity === 0 || el.disabled) continue;
    if (r.bottom < 0 || r.top > innerHeight || r.right < 0 || r.left > innerWidth) continue;
    const cx = Math.min(Math.max(r.left + r.width / 2, 1), innerWidth - 1), cy = Math.min(Math.max(r.top + r.height / 2, 1), innerHeight - 1);
    const top = document.elementFromPoint(cx, cy);
    if (top && top !== el && !el.contains(top) && !top.contains(el)) continue;
    const kind = el.tagName.toLowerCase() + (el.tagName === 'INPUT' ? '[' + (el.type || 'text') + ']' : '') + (el.getAttribute('role') ? '(' + el.getAttribute('role') + ')' : '');
    out.push('[' + window.__adEls.length + '] ' + kind + ' "' + adName(el) + '" at (' + Math.round(cx) + ',' + Math.round(cy) + ')');
    window.__adEls.push(el);
    if (out.length >= 80) break;
  }
  const doc = document.documentElement;
  const head = 'viewport ' + innerWidth + 'x' + innerHeight + ', scrolled ' + Math.round(scrollY) + ' of ' + Math.max(0, doc.scrollHeight - innerHeight) + ' px';
  const note = ov ? '\nOPEN OVERLAY: "' + adDescribe(ov) + '" (dialog, drawer or menu). The page behind it cannot be clicked. Only its own elements are listed below. To get back to the page, close it first: click its close/cancel button, or press Escape with browser_key, then call browser_elements again.' : '';
  return head + note + '\n' + (out.join('\n') || '(nothing to click in view)');
})()"#;

/// Runs in the page after a click: says whether a dialog, drawer or menu is open now.
const OVERLAY_NOTE_JS: &str = r#"(() => {
  __HELPERS__
  const ov = adOverlay();
  return ov ? 'A dialog, drawer or menu is open: "' + adDescribe(ov) + '". Close it (its close button, or Escape) before using the page behind it.' : 'No dialog, drawer or menu is open.';
})()"#;

/// Runs in the page: common UI faults with a selector each (see the `ui_audit` tool).
fn ui_audit_js(args: &Value) -> String {
    format!(
        r#"((arg) => {{
  const scope = (arg.selector && document.querySelector(arg.selector)) || document.body;
  const max = Math.max(5, Math.min(200, +arg.max || 40));
  const issues = [];
  const add = (el, problem) => {{ if (issues.length < max * 3) issues.push({{ at: sel(el), problem }}); }};
  // A short selector: the element and at most two parents (an id ends the walk), enough to find it in the code.
  const sel = (el, depth = 0) => {{
    if (!el || el === document.body) return 'body';
    if (el.id) return el.tagName.toLowerCase() + '#' + el.id;
    const cls = [...el.classList].filter((c) => !/[:[\]/]/.test(c)).slice(0, 2).map((c) => '.' + c).join('');
    const parent = el.parentElement;
    const nth = parent ? [...parent.children].filter((c) => c.tagName === el.tagName).indexOf(el) + 1 : 1;
    const own = el.tagName.toLowerCase() + cls + (nth > 1 ? ':nth-of-type(' + nth + ')' : '');
    return parent && parent !== document.body && depth < 2 ? sel(parent, depth + 1) + ' > ' + own : own;
  }};
  const shown = (el) => {{ const r = el.getBoundingClientRect(); const cs = getComputedStyle(el); return r.width > 0 && r.height > 0 && cs.visibility !== 'hidden' && cs.display !== 'none' && +cs.opacity > 0.05; }};
  const rgb = (text) => {{ const m = /rgba?\(([^)]+)\)/.exec(text || ''); if (!m) return null; const p = m[1].split(/[ ,/]+/).filter(Boolean).map(Number); return {{ r: p[0], g: p[1], b: p[2], a: p.length > 3 ? p[3] : 1 }}; }};
  const lum = (c) => {{ const f = (v) => {{ v /= 255; return v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4); }}; return 0.2126 * f(c.r) + 0.7152 * f(c.g) + 0.0722 * f(c.b); }};
  const background = (el) => {{ for (let e = el; e; e = e.parentElement) {{ const c = rgb(getComputedStyle(e).backgroundColor); if (c && c.a > 0.5) return c; if (getComputedStyle(e).backgroundImage !== 'none') return null; }} return {{ r: 255, g: 255, b: 255, a: 1 }}; }};
  const hex = (c) => '#' + [c.r, c.g, c.b].map((v) => Math.round(v).toString(16).padStart(2, '0')).join('');

  // Overflow: the page scrolls sideways, or an element runs past the right edge.
  const doc = document.documentElement;
  if (doc.scrollWidth > innerWidth + 1) issues.push({{ at: 'html', problem: 'page scrolls sideways: content is ' + doc.scrollWidth + 'px wide in a ' + innerWidth + 'px viewport' }});
  const all = [...scope.querySelectorAll('*')].filter((el) => !el.closest('#__ad-guide-marks')).slice(0, 6000);
  const usedColors = new Map();
  const usedSizes = new Map();
  for (const el of all) {{
    if (!shown(el)) continue;
    const r = el.getBoundingClientRect();
    const cs = getComputedStyle(el);
    if (r.right > innerWidth + 2 && r.width < innerWidth * 2 && cs.position !== 'fixed' && !el.closest('[style*="overflow"],[class*="scroll"],[class*="overflow"]')) add(el, 'runs ' + Math.round(r.right - innerWidth) + 'px past the right edge');
    const ownText = [...el.childNodes].some((n) => n.nodeType === 3 && n.textContent.trim().length > 1);
    if (ownText) {{
      const size = parseFloat(cs.fontSize);
      usedSizes.set(cs.fontSize, (usedSizes.get(cs.fontSize) || 0) + 1);
      usedColors.set(cs.color, (usedColors.get(cs.color) || 0) + 1);
      if (size < 12) add(el, 'text is ' + size + 'px: too small to read (12px at least)');
      const fg = rgb(cs.color);
      const bg = background(el);
      if (fg && bg && fg.a > 0.5) {{
        const ratio = (Math.max(lum(fg), lum(bg)) + 0.05) / (Math.min(lum(fg), lum(bg)) + 0.05);
        const large = size >= 24 || (size >= 18.6 && +cs.fontWeight >= 700);
        const need = large ? 3 : 4.5;
        if (ratio < need) add(el, 'low contrast ' + ratio.toFixed(2) + ':1 (' + hex(fg) + ' on ' + hex(bg) + ', needs ' + need + ':1)');
      }}
    }}
    const bgc = rgb(cs.backgroundColor);
    if (bgc && bgc.a > 0.5) usedColors.set(cs.backgroundColor, (usedColors.get(cs.backgroundColor) || 0) + 1);
    const interactive = el.matches('a[href],button,input:not([type=hidden]),select,textarea,[role=button],[role=link],[role=tab],[role=menuitem],[onclick]');
    if (interactive) {{
      if ((r.width < 24 || r.height < 24) && !el.closest('p,li,td') ) add(el, 'click target ' + Math.round(r.width) + 'x' + Math.round(r.height) + 'px: under 24px');
      const name = (el.getAttribute('aria-label') || el.getAttribute('title') || el.innerText || el.value || el.getAttribute('placeholder') || (el.querySelector('img[alt]') || {{}}).alt || '').trim();
      if (!name && !el.matches('input[type=checkbox],input[type=radio]')) add(el, el.tagName.toLowerCase() + ' has no accessible name (text, aria-label or title)');
    }}
    if (el.tagName === 'IMG' && !el.hasAttribute('alt')) add(el, 'image without alt text');
  }}

  // One-off values: colours and font sizes that are not design tokens.
  const tokenColors = new Set();
  const probe = document.createElement('span');
  document.body.appendChild(probe);
  const rootStyle = getComputedStyle(document.documentElement);
  for (const sheet of [...document.styleSheets]) {{
    let rules = [];
    try {{ rules = [...sheet.cssRules]; }} catch {{ continue; }}
    for (const rule of rules) {{
      if (!rule.style) continue;
      for (const prop of [...rule.style]) {{
        if (!prop.startsWith('--')) continue;
        const value = rootStyle.getPropertyValue(prop).trim();
        if (!value || !/^(#|rgb|hsl|oklch|oklab|color)/.test(value)) continue;
        probe.style.color = value;
        tokenColors.add(getComputedStyle(probe).color);
      }}
    }}
  }}
  probe.remove();
  const offToken = [...usedColors.entries()].filter(([c]) => tokenColors.size > 0 && !tokenColors.has(c) && !/rgba?\(0, 0, 0, 0\)/.test(c)).sort((a, b) => b[1] - a[1]);
  const summary = {{
    viewport: innerWidth + 'x' + innerHeight,
    elements: all.length,
    fontSizes: usedSizes.size,
    colors: usedColors.size,
    tokenColors: tokenColors.size,
  }};
  if (usedSizes.size > 8) issues.push({{ at: 'page', problem: usedSizes.size + ' different font sizes in use (' + [...usedSizes.keys()].slice(0, 12).join(', ') + '): keep to the type scale of the project' }});
  if (offToken.length > 6) issues.push({{ at: 'page', problem: offToken.length + ' colours are not design tokens, most used: ' + offToken.slice(0, 6).map(([c, n]) => c + ' x' + n).join('; ') }});
  // One line per problem kind and place; the worst kinds first.
  const order = (p) => /sideways|past the right/.test(p) ? 0 : /contrast/.test(p) ? 1 : /accessible name|alt text/.test(p) ? 2 : /click target/.test(p) ? 3 : 4;
  const seen = new Set();
  const unique = issues.filter((i) => {{ const k = i.at + '|' + i.problem.replace(/[\d.]+/g, '#'); if (seen.has(k)) return false; seen.add(k); return true; }});
  // A kind of problem found many times is one line with a few examples, so it does not hide the other kinds.
  const kind = (p) => p.replace(/[\d.]+(px|:1)?/g, '#').replace(/\(.*\)/, '').trim();
  const byKind = new Map();
  for (const i of unique) {{ const k = kind(i.problem); byKind.set(k, [...(byKind.get(k) || []), i]); }}
  const grouped = [...byKind.values()].flatMap((group) => group.length > 4 ? [...group.slice(0, 3), {{ at: '(' + (group.length - 3) + ' more)', problem: group.length + ' elements in all: ' + group[0].problem }}] : group);
  const list = grouped.sort((a, b) => order(a.problem) - order(b.problem)).slice(0, max);
  return JSON.stringify({{ summary, issues: list, verdict: list.length ? list.length + ' issue(s): fix them and run ui_audit again' : 'No issue found' }});
}})({})"#,
        args
    )
}

/// Runs in the page: draws (or clears) a guide callout, an outline with a badge and a label, in a top layer of its own.
fn annotate_js(args: &Value) -> String {
    format!(
        r#"((arg) => {{
  const ROOT = '__ad-guide-marks';
  if (arg.clear) {{ document.getElementById(ROOT)?.remove(); return 'Marks cleared.'; }}
  let rect = null;
  if (arg.w > 0 && arg.h > 0) rect = {{ left: +arg.x || 0, top: +arg.y || 0, width: +arg.w, height: +arg.h }};
  else {{
    let el = null;
    if (arg.index !== undefined && arg.index !== null) el = (window.__adEls || [])[arg.index];
    else if (arg.selector) el = document.querySelector(arg.selector);
    else if (arg.text) {{
      const want = String(arg.text).trim().toLowerCase();
      const label = (e) => (e.getAttribute('aria-label') || e.innerText || e.value || e.placeholder || e.title || '').replace(/\s+/g, ' ').trim().toLowerCase();
      const shown = (e) => {{ const r = e.getBoundingClientRect(); const cs = getComputedStyle(e); return r.width > 1 && r.height > 1 && cs.visibility !== 'hidden' && cs.display !== 'none'; }};
      const all = [...document.querySelectorAll('a,button,input,select,textarea,summary,label,li,h1,h2,h3,h4,p,span,div,[role]')].filter((e) => shown(e) && label(e).includes(want));
      all.sort((a, b) => (label(a) === want ? 0 : 1) - (label(b) === want ? 0 : 1) || label(a).length - label(b).length);
      el = all[0];
    }}
    if (!el || !el.isConnected) return {{ error: 'element not found: call browser_elements and use a number from it, or give x, y, w, h' }};
    const first = el.getBoundingClientRect();
    if (first.top < 0 || first.bottom > innerHeight) el.scrollIntoView({{ block: 'center', inline: 'nearest' }});
    rect = el.getBoundingClientRect();
  }}
  let root = document.getElementById(ROOT);
  if (!root) {{
    root = document.createElement('div');
    root.id = ROOT;
    root.style.cssText = 'position:fixed;inset:0;pointer-events:none;z-index:2147483647;font:600 13px/1.35 system-ui,-apple-system,Segoe UI,sans-serif';
    document.documentElement.appendChild(root);
  }}
  const color = arg.color || '#ef4444';
  const pad = 4;
  const left = rect.left - pad, top = rect.top - pad, width = rect.width + pad * 2, height = rect.height + pad * 2;
  const box = document.createElement('div');
  box.style.cssText = 'position:absolute;left:' + left + 'px;top:' + top + 'px;width:' + width + 'px;height:' + height + 'px;box-sizing:border-box;border:3px ' + (arg.style === 'dashed' ? 'dashed' : 'solid') + ' ' + color + ';border-radius:6px;box-shadow:0 0 0 2px rgba(255,255,255,.85),0 4px 14px rgba(0,0,0,.3)';
  root.appendChild(box);
  if (arg.number !== undefined && arg.number !== null && arg.number !== '') {{
    const badge = document.createElement('div');
    badge.textContent = String(arg.number);
    badge.style.cssText = 'position:absolute;left:' + Math.max(2, left - 13) + 'px;top:' + Math.max(2, top - 13) + 'px;min-width:26px;height:26px;padding:0 6px;box-sizing:border-box;border-radius:13px;background:' + color + ';color:#fff;border:2px solid #fff;display:flex;align-items:center;justify-content:center;box-shadow:0 2px 8px rgba(0,0,0,.35)';
    root.appendChild(badge);
  }}
  if (arg.label) {{
    const tag = document.createElement('div');
    tag.textContent = String(arg.label);
    const below = top + height + 40 < innerHeight;
    tag.style.cssText = 'position:absolute;left:' + Math.max(4, Math.min(left, innerWidth - 324)) + 'px;' + (below ? 'top:' + (top + height + 8) : 'bottom:' + (innerHeight - top + 8)) + 'px;max-width:320px;padding:5px 9px;border-radius:6px;background:' + color + ';color:#fff;box-shadow:0 2px 10px rgba(0,0,0,.3)';
    root.appendChild(tag);
  }}
  return {{ marked: {{ x: Math.round(rect.left), y: Math.round(rect.top), w: Math.round(rect.width), h: Math.round(rect.height) }}, marks: root.childElementCount }};
}})({})"#,
        args
    )
}

/// Runs in the page with the target (`index`, `text` or `selector`): scrolls it into view and answers its centre.
fn locate_js(args: &Value) -> String {
    format!(
        r#"((arg) => {{
  let el = null;
  if (arg.index !== undefined && arg.index !== null) el = (window.__adEls || [])[arg.index];
  else if (arg.selector) el = document.querySelector(arg.selector);
  else if (arg.text) {{
    const want = String(arg.text).trim().toLowerCase();
    const label = (e) => (e.getAttribute('aria-label') || e.innerText || e.value || e.placeholder || e.title || '').replace(/\s+/g, ' ').trim().toLowerCase();
    const shown = (e) => {{ const r = e.getBoundingClientRect(); const cs = getComputedStyle(e); return r.width > 1 && r.height > 1 && cs.visibility !== 'hidden' && cs.display !== 'none'; }};
    const all = [...document.querySelectorAll('a,button,input,select,textarea,summary,label,li,h1,h2,h3,h4,p,span,div,[role]')].filter((e) => shown(e) && label(e).includes(want));
    all.sort((a, b) => (label(a) === want ? 0 : 1) - (label(b) === want ? 0 : 1) || label(a).length - label(b).length);
    el = all[0];
  }}
  if (!el || !el.isConnected) return {{ error: 'element not found: call browser_elements again and use a number from it' }};
  el.scrollIntoView({{ block: 'center', inline: 'center' }});
  const r = el.getBoundingClientRect();
  return {{ x: Math.round(r.left + r.width / 2), y: Math.round(r.top + r.height / 2), tag: el.tagName.toLowerCase(), label: (el.innerText || el.value || el.getAttribute('aria-label') || '').trim().slice(0, 60) }};
}})({})"#,
        args
    )
}

/// Runs in the page: its visible text, main part first, cut at `max` characters.
fn text_js(max: u64) -> String {
    format!(
        r#"(() => {{
  const root = document.querySelector('main, article, [role=main]') || document.body;
  const text = (root?.innerText || '').replace(/\n{{3,}}/g, '\n\n').trim();
  const max = {max};
  return text.length > max ? text.slice(0, max) + '\n… (' + (text.length - max) + ' more characters)' : text || '(the page has no text)';
}})()"#
    )
}

/// Runs in the page: the elements matching `query`, numbered into `window.__adEls` like browser_elements.
fn find_js(query: &str) -> String {
    format!(
        r#"((query) => {{
  __HELPERS__
  const want = String(query).trim().toLowerCase();
  if (!want) return 'give a query';
  const line = (e) => [e.getAttribute('aria-label'), e.getAttribute('role'), e.placeholder, e.title, e.alt, e.value, e.innerText].filter(Boolean).join(' ').replace(/\s+/g, ' ').toLowerCase();
  const sel = 'a[href],button,input,select,textarea,summary,label,[role],[aria-label],[placeholder],[title],[onclick],[tabindex],h1,h2,h3,h4,h5,h6,p,li,td,th,span,div';
  const hits = [...document.querySelectorAll(sel)].filter((e) => {{
    const r = e.getBoundingClientRect(); const cs = getComputedStyle(e);
    return r.width > 1 && r.height > 1 && cs.visibility !== 'hidden' && cs.display !== 'none' && line(e).includes(want);
  }});
  // The innermost match: a wrapper whose child also matches says nothing more.
  const tight = hits.filter((e) => !hits.some((o) => o !== e && e.contains(o)));
  window.__adEls = [];
  const out = tight.slice(0, 20).map((el) => {{
    const r = el.getBoundingClientRect();
    const kind = el.tagName.toLowerCase() + (el.tagName === 'INPUT' ? '[' + (el.type || 'text') + ']' : '') + (el.getAttribute('role') ? '(' + el.getAttribute('role') + ')' : '');
    const where = r.bottom < 0 || r.top > innerHeight ? ' (out of view)' : '';
    window.__adEls.push(el);
    return '[' + (window.__adEls.length - 1) + '] ' + kind + ' "' + adName(el) + '" at (' + Math.round(r.left + r.width / 2) + ',' + Math.round(r.top + r.height / 2) + ')' + where;
  }});
  return out.length ? out.join('\n') + (tight.length > 20 ? '\n… ' + (tight.length - 20) + ' more: use a longer query' : '') : 'nothing matches "' + query + '"';
}})({})"#,
        Value::String(query.to_string())
    )
    .replace("__HELPERS__", PAGE_HELPERS)
}

/// Stores the elements the last `browser_elements` / `browser_find` numbered (`window.__adEls`), with their selectors.
async fn remember_seen(app: &AppHandle, params: &HashMap<String, String>) {
    let Ok(answer) = crate::bridge::answer(app, "/eval", params, crate::page_memory::MEMO_JS).await else {
        return;
    };
    let Some(text) = answer["result"].as_str() else {
        return;
    };
    if let Ok(seen) = serde_json::from_str::<Value>(text) {
        if seen["elements"].as_array().is_some_and(|list| !list.is_empty()) {
            crate::page_memory::store(app, seen);
        }
    }
}

/// `browser_recall` and `browser_remember`.
async fn memory_tool(app: &AppHandle, name: &str, args: &Value, chat: &str) -> Value {
    let mut params = HashMap::new();
    if !chat.is_empty() {
        params.insert("chat".to_string(), chat.to_string());
    }
    let url = match crate::bridge::answer(app, "/eval", &params, "location.href").await {
        Ok(answer) => answer["result"].as_str().unwrap_or_default().trim_matches('"').to_owned(),
        Err(error) => return error_reply(error),
    };
    if !url.starts_with("http") {
        return error_reply("open a web page in the Preview first (browser_open)".into());
    }
    if name == "browser_remember" {
        let goal = args["goal"].as_str().unwrap_or_default().trim();
        let steps: Vec<&str> = args["steps"]
            .as_str()
            .unwrap_or_default()
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect();
        if goal.is_empty() || steps.is_empty() {
            return error_reply("give a goal and at least one step".into());
        }
        return match crate::page_memory::ask(app, "remember", json!({ "url": url, "goal": goal, "steps": steps })).await {
            Ok(_) => text_reply(format!("Remembered {} steps for \"{goal}\".", steps.len())),
            Err(error) => error_reply(error),
        };
    }
    let query = args["query"].as_str().unwrap_or_default().trim();
    if query.is_empty() {
        return error_reply("say what you are looking for".into());
    }
    let limit = args["limit"].as_u64().unwrap_or(8).clamp(1, 20);
    let found = match crate::page_memory::ask(app, "recall", json!({ "url": url, "query": query, "limit": limit })).await {
        Ok(text) => serde_json::from_str::<Value>(&text).unwrap_or(Value::Null),
        Err(error) => return error_reply(error),
    };
    let selectors: Vec<String> = found["hits"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|hit| hit["selector"].as_str().map(str::to_owned))
        .collect();
    let live = if selectors.is_empty() {
        Vec::new()
    } else {
        match crate::bridge::answer(app, "/eval", &params, &crate::page_memory::verify_js(&selectors)).await {
            Ok(answer) => answer["result"]
                .as_str()
                .and_then(|text| serde_json::from_str::<Vec<Value>>(text).ok())
                .unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    };
    text_reply(crate::page_memory::format_recall(&found, &live))
}

fn text_reply(text: String) -> Value {
    json!({ "content": [{ "type": "text", "text": text }] })
}

fn error_reply(text: String) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": true })
}

/// The tools that are a few steps of page script and clicks, not one bridge route.
async fn element_tool(app: &AppHandle, name: &str, args: &Value, chat: &str) -> Value {
    let mut params = HashMap::new();
    if !chat.is_empty() {
        params.insert("chat".to_string(), chat.to_string());
    }
    // The page's answer is the script's value, possibly as text that holds JSON.
    let value_of = |answer: Value| match &answer["result"] {
        Value::String(text) => {
            serde_json::from_str::<Value>(text).unwrap_or_else(|_| Value::String(text.clone()))
        }
        other => other.clone(),
    };
    let script = match name {
        "browser_elements" => Some(ELEMENTS_JS.replace("__HELPERS__", PAGE_HELPERS)),
        "browser_text" => Some(text_js(
            args["max_chars"]
                .as_u64()
                .unwrap_or(20_000)
                .clamp(500, 200_000),
        )),
        "browser_find" => Some(find_js(args["query"].as_str().unwrap_or_default())),
        "browser_annotate" => Some(annotate_js(args)),
        "ui_audit" => Some(ui_audit_js(args)),
        _ => None,
    };
    if let Some(script) = script {
        let remember = matches!(name, "browser_elements" | "browser_find");
        return match crate::bridge::answer(app, "/eval", &params, &script).await {
            Ok(answer) => match value_of(answer) {
                Value::String(text) => {
                    // What the agent was just shown is kept for the next visit (browser_recall).
                    if remember {
                        remember_seen(app, &params).await;
                    }
                    text_reply(text)
                }
                other if other["error"].is_string() => {
                    error_reply(other["error"].as_str().unwrap_or_default().to_string())
                }
                other => text_reply(other.to_string()),
            },
            Err(error) => error_reply(error),
        };
    }
    let target = match crate::bridge::answer(app, "/eval", &params, &locate_js(args)).await {
        Ok(answer) => value_of(answer),
        Err(error) => return error_reply(error),
    };
    if let Some(error) = target["error"].as_str() {
        return error_reply(error.to_string());
    }
    let (Some(x), Some(y)) = (target["x"].as_f64(), target["y"].as_f64()) else {
        return error_reply(format!("could not locate the element: {target}"));
    };
    params.insert("x".into(), x.to_string());
    params.insert("y".into(), y.to_string());
    match crate::bridge::answer(app, "/click", &params, "").await {
        Ok(_) => {
            // Let the page react (a drawer slides in, a menu opens), then say what is on top now.
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            let note = match crate::bridge::answer(
                app,
                "/eval",
                &params,
                &OVERLAY_NOTE_JS.replace("__HELPERS__", PAGE_HELPERS),
            )
            .await
            {
                Ok(answer) => value_of(answer).as_str().unwrap_or_default().to_owned(),
                Err(_) => String::new(),
            };
            text_reply(format!(
                "Clicked <{}> \"{}\" at ({x}, {y}). {note}",
                target["tag"].as_str().unwrap_or("element"),
                target["label"].as_str().unwrap_or_default()
            ))
        }
        Err(error) => error_reply(error),
    }
}

pub(crate) fn tool_list() -> Value {
    Value::Array(
        TOOLS
            .iter()
            .map(|tool| {
                let properties: serde_json::Map<String, Value> = tool
                    .args
                    .iter()
                    .map(|(name, kind, about, _)| (name.to_string(), json!({ "type": kind, "description": about })))
                    .collect();
                let required: Vec<&str> = tool.args.iter().filter(|arg| arg.3).map(|arg| arg.0).collect();
                json!({
                    "name": tool.name,
                    "description": tool.description,
                    "inputSchema": { "type": "object", "properties": properties, "required": required },
                })
            })
            .chain(crate::index::tool_defs())
            .chain(crate::devtools::tool_defs())
            .chain(crate::delegate::tool_defs())
            .chain(crate::page_memory::tool_defs())
            .chain([
                json!({"name":"computer_screen_size","description":"Read the size of the Windows primary display.","inputSchema":{"type":"object","properties":{}}}),
                json!({"name":"computer_screenshot","description":"Capture the Windows primary display. Computer Use is on by default; disable it in AgentDeck Settings to block this call.","inputSchema":{"type":"object","properties":{}}}),
                json!({"name":"computer_click","description":"Click a coordinate on the Windows primary display. Computer Use is on by default; disable it in AgentDeck Settings to block this call.","inputSchema":{"type":"object","properties":{"x":{"type":"integer"},"y":{"type":"integer"}},"required":["x","y"]}}),
                json!({"name":"computer_type","description":"Type text into the focused Windows app. Computer Use is on by default; disable it in AgentDeck Settings to block this call.","inputSchema":{"type":"object","properties":{"text":{"type":"string"}},"required":["text"]}}),
                json!({"name":"computer_press_key","description":"Press a keyboard key in the focused Windows app. Computer Use is on by default; disable it in AgentDeck Settings to block this call.","inputSchema":{"type":"object","properties":{"key":{"type":"string"}},"required":["key"]}}),
                json!({"name":"computer_scroll","description":"Scroll the Windows mouse wheel. Positive delta scrolls up; negative scrolls down. Computer Use is on by default; disable it in AgentDeck Settings to block this call.","inputSchema":{"type":"object","properties":{"delta":{"type":"integer"}},"required":["delta"]}}),
            ])
            .collect(),
    )
}

/// Writes a screenshot where the agent asked (an absolute .png path), creating its folder.
fn save_shot(path: &str, bytes: &[u8]) -> Result<String, String> {
    let path = std::path::Path::new(path);
    if !path.is_absolute() {
        return Err("save_as must be an absolute path".into());
    }
    if path.extension().and_then(|ext| ext.to_str()).map(|ext| ext.eq_ignore_ascii_case("png")) != Some(true) {
        return Err("save_as must end in .png".into());
    }
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder).map_err(|error| error.to_string())?;
    }
    std::fs::write(path, bytes).map_err(|error| error.to_string())?;
    Ok(path.display().to_string())
}

async fn call_tool(app: &AppHandle, name: &str, args: &Value, chat: &str) -> Value {
    if name == "browser_record_start" {
        let Some(root) = args["root"].as_str().filter(|root| !root.trim().is_empty()) else {
            return error_reply("root must be the absolute path of the project folder".into());
        };
        if !std::path::Path::new(root).is_absolute() || !std::path::Path::new(root).is_dir() {
            return error_reply("root must be an existing absolute project folder".into());
        }
        let file_name = args["file_name"].as_str().unwrap_or_default();
        if !file_name.is_empty()
            && (std::path::Path::new(file_name).components().count() != 1
                || std::path::Path::new(file_name).file_name().map(|name| name != file_name).unwrap_or(true)
                || !file_name.ends_with(".webm"))
        {
            return error_reply("file_name must be a simple filename ending in .webm".into());
        }
        return match control_guide_recording(app, chat, "start", root, file_name).await {
            Ok(text) => text_reply(text),
            Err(error) => error_reply(error),
        };
    }
    if name == "browser_record_stop" {
        return match control_guide_recording(app, chat, "stop", "", "").await {
            Ok(text) => text_reply(text),
            Err(error) => error_reply(error),
        };
    }
    if name == "browser_zoom" {
        let Some(factor) = args["factor"].as_f64() else {
            return error_reply("factor must be a number".into());
        };
        let tab = crate::bridge::active_tab();
        return match crate::browser::browser_zoom(app.clone(), tab, factor) {
            Ok(()) => text_reply(format!("Preview zoom set to {:.0}%", factor.clamp(0.25, 3.0) * 100.0)),
            Err(error) => error_reply(error),
        };
    }
    if crate::page_memory::is_tool(name) {
        return memory_tool(app, name, args, chat).await;
    }
    if name.starts_with("computer_") {
        return computer_tool(app, name, args, chat).await;
    }
    if crate::delegate::is_tool(name) {
        return crate::delegate::call(app, name, args, chat).await;
    }
    if crate::devtools::is_tool(name) {
        return match crate::devtools::call(name, args, chat).await {
            Ok(text) => text_reply(text),
            Err(error) => error_reply(error),
        };
    }
    if crate::index::is_tool(name) {
        return match crate::index::call(name, args, chat).await {
            Ok(text) => text_reply(text),
            Err(error) => error_reply(error),
        };
    }
    if matches!(
        name,
        "browser_elements"
            | "browser_click_element"
            | "browser_text"
            | "browser_find"
            | "browser_annotate"
            | "ui_audit"
    ) {
        return element_tool(app, name, args, chat).await;
    }
    let Some(tool) = TOOLS.iter().find(|tool| tool.name == name) else {
        return json!({ "content": [{ "type": "text", "text": format!("unknown tool {name}") }], "isError": true });
    };
    let mut params = HashMap::new();
    if !chat.is_empty() {
        params.insert("chat".to_string(), chat.to_string());
    }
    let mut body = String::new();
    if let Some(map) = args.as_object() {
        for (key, value) in map {
            let text = match value {
                Value::String(text) => text.clone(),
                Value::Bool(true) => "1".into(),
                Value::Bool(false) => "0".into(),
                other => other.to_string(),
            };
            if key == "script" {
                body = text;
            } else if key == "save_as" {
                continue;
            } else {
                params.insert(key.clone(), text);
            }
        }
    }
    match crate::bridge::answer(app, tool.route, &params, &body).await {
        Ok(value) if tool.route == "/shot" => match value["png"].as_str().map(std::fs::read) {
            Some(Ok(bytes)) => {
                use base64::Engine;
                let image = json!({ "type": "image", "mimeType": "image/png", "data": base64::engine::general_purpose::STANDARD.encode(&bytes) });
                match args["save_as"].as_str().filter(|path| !path.trim().is_empty()) {
                    Some(path) => match save_shot(path, &bytes) {
                        Ok(saved) => json!({ "content": [image, { "type": "text", "text": format!("Saved to {saved}") }] }),
                        Err(error) => json!({ "content": [image, { "type": "text", "text": format!("Not saved: {error}") }], "isError": true }),
                    },
                    None => json!({ "content": [image] }),
                }
            }
            _ => json!({ "content": [{ "type": "text", "text": value.to_string() }] }),
        },
        Ok(value) => {
            let text = match value {
                Value::String(text) => text,
                other => serde_json::to_string_pretty(&other).unwrap_or_default(),
            };
            json!({ "content": [{ "type": "text", "text": text }] })
        }
        Err(error) => json!({ "content": [{ "type": "text", "text": error }], "isError": true }),
    }
}

type GuideRecordingReplies = Mutex<HashMap<String, oneshot::Sender<Result<String, String>>>>;

static GUIDE_RECORDING_REPLIES: LazyLock<GuideRecordingReplies> = LazyLock::new(Default::default);

/// Sends a start/stop request to the app-shell recorder and waits for its saved-path result.
async fn control_guide_recording(
    app: &AppHandle,
    chat: &str,
    action: &str,
    root: &str,
    file_name: &str,
) -> Result<String, String> {
    let request_id = uuid::Uuid::new_v4().to_string();
    let (sender, receiver) = oneshot::channel();
    GUIDE_RECORDING_REPLIES
        .lock()
        .map_err(|_| "guide recorder is busy".to_string())?
        .insert(request_id.clone(), sender);
    let event = json!({
        "requestId": request_id,
        "action": action,
        "root": root,
        "fileName": file_name,
        "chat": chat,
    });
    if let Err(error) = app.emit("agentdeck:guide-record-control", event) {
        GUIDE_RECORDING_REPLIES.lock().ok().and_then(|mut replies| replies.remove(&request_id));
        return Err(error.to_string());
    }
    match tokio::time::timeout(Duration::from_secs(90), receiver).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => Err("the guide recorder closed before replying".into()),
        Err(_) => {
            GUIDE_RECORDING_REPLIES.lock().ok().and_then(|mut replies| replies.remove(&request_id));
            Err("the guide recorder did not respond within 90 seconds".into())
        }
    }
}

/// Completes one pending browser_record_start or browser_record_stop tool call.
pub(crate) fn guide_recording_reply(request_id: String, ok: bool, message: String) -> Result<(), String> {
    let sender = GUIDE_RECORDING_REPLIES
        .lock()
        .map_err(|_| "guide recorder is busy".to_string())?
        .remove(&request_id)
        .ok_or_else(|| "the recording request expired".to_string())?;
    let result = if ok { Ok(message) } else { Err(message) };
    sender.send(result).map_err(|_| "the browser tool already ended".to_string())
}

async fn computer_tool(app: &AppHandle, name: &str, args: &Value, chat: &str) -> Value {
    if !crate::computer::enabled() {
        return error_reply(
            "Computer use is disabled. Enable it in AgentDeck Settings > Computer use.".into(),
        );
    }
    match name {
        "computer_screen_size" => {
            let (width, height) = crate::computer::screen_size();
            text_reply(format!("Windows primary display: {width}×{height}"))
        }
        "computer_screenshot" => {
            if let Err(error) = crate::computer::approve(
                app,
                chat,
                name,
                "Share a screenshot of the Windows desktop with this agent".into(),
            )
            .await
            {
                return error_reply(error);
            }
            match crate::computer::screenshot() {
                Ok(bytes) => {
                    use base64::Engine;
                    json!({"content":[{"type":"image","mimeType":"image/png","data":base64::engine::general_purpose::STANDARD.encode(bytes)}]})
                }
                Err(error) => error_reply(error),
            }
        }
        "computer_click" => {
            let (Some(x), Some(y)) = (args["x"].as_i64(), args["y"].as_i64()) else {
                return error_reply("x and y must be integers".into());
            };
            let (width, height) = crate::computer::screen_size();
            if x < 0 || y < 0 || x >= i64::from(width) || y >= i64::from(height) {
                return error_reply(format!(
                    "Coordinates must be inside the primary display ({width}×{height})."
                ));
            }
            if let Err(error) = crate::computer::approve(
                app,
                chat,
                name,
                format!("Click Windows screen position ({x}, {y})"),
            )
            .await
            {
                return error_reply(error);
            }
            match crate::computer::click(x as i32, y as i32) {
                Ok(()) => text_reply(format!("Clicked ({x}, {y}).")),
                Err(error) => error_reply(error),
            }
        }
        "computer_type" => {
            let Some(text) = args["text"].as_str() else {
                return error_reply("text must be a string".into());
            };
            if text.chars().count() > 10_000 {
                return error_reply(
                    "Text input is limited to 10,000 characters per approved action.".into(),
                );
            }
            let preview: String = text.chars().take(160).collect();
            if let Err(error) = crate::computer::approve(
                app,
                chat,
                name,
                format!("Type into the focused Windows app: {preview}"),
            )
            .await
            {
                return error_reply(error);
            }
            match crate::computer::type_text(text) {
                Ok(()) => text_reply("Text typed.".into()),
                Err(error) => error_reply(error),
            }
        }
        "computer_press_key" => {
            let Some(key) = args["key"].as_str() else {
                return error_reply("key must be a string".into());
            };
            if let Err(error) = crate::computer::approve(
                app,
                chat,
                name,
                format!("Press the {key} key in the focused Windows app"),
            )
            .await
            {
                return error_reply(error);
            }
            match crate::computer::press_key(key) {
                Ok(()) => text_reply(format!("Pressed {key}.")),
                Err(error) => error_reply(error),
            }
        }
        "computer_scroll" => {
            let Some(delta) = args["delta"].as_i64() else {
                return error_reply("delta must be an integer".into());
            };
            if let Err(error) = crate::computer::approve(
                app,
                chat,
                name,
                format!("Scroll the Windows mouse wheel by {delta}"),
            )
            .await
            {
                return error_reply(error);
            }
            match crate::computer::scroll(delta.clamp(-1200, 1200) as i32) {
                Ok(()) => text_reply("Scrolled.".into()),
                Err(error) => error_reply(error),
            }
        }
        _ => error_reply(format!("unknown computer-use tool {name}")),
    }
}

/// One JSON-RPC message. `None` for a notification (the HTTP answer is then 202 with no body).
pub async fn handle(app: &AppHandle, body: &str, chat: &str) -> Option<String> {
    let request: Value = serde_json::from_str(body).ok()?;
    let id = request.get("id").cloned()?;
    let params = &request["params"];
    let result = match request["method"].as_str().unwrap_or_default() {
        "initialize" => json!({
            "protocolVersion": params["protocolVersion"].as_str().unwrap_or("2025-06-18"),
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "agentdeck", "version": env!("CARGO_PKG_VERSION") },
            "instructions": "The page open in AgentDeck's Preview panel, the browser the user is looking at. Use these tools to see it and act on it as a user would: screenshot, then click / type / scroll, then screenshot again and check browser_console and browser_network for errors. For a requested video guide, call browser_record_start with the absolute project root before operating the page, then browser_record_stop when finished; it saves the WebM under docs/user-guides. Computer Use is on by default: computer_screenshot, computer_click, computer_type, computer_press_key and computer_scroll operate the user's Windows desktop immediately; the user can disable these calls in AgentDeck Settings. The code_* tools search an index of the project (symbols, outlines, definitions, references, imports): use them before grepping the tree. delegate_task routes work to another AgentDeck provider; use native sub-agents for internal orchestration when your CLI provides them. Keep ownership of integration and the final answer.",
        }),
        "ping" => json!({}),
        "tools/list" => json!({ "tools": tool_list() }),
        "tools/call" => {
            call_tool(
                app,
                params["name"].as_str().unwrap_or_default(),
                &params["arguments"],
                chat,
            )
            .await
        }
        method => {
            return Some(json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32601, "message": format!("unknown method {method}") } }).to_string());
        }
    };
    Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string())
}
