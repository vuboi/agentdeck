// Runs inside every page of the AgentDeck browser (the child webview). It watches the page (console, network,
// navigation) and answers requests from the app (DOM, screenshot, element picker) through the `agentdeck` URL
// scheme. Nothing here talks to a server: every message goes to the app on this machine.
(() => {
  if (window.__agentdeck || window.top !== window) return;
  const BASE = '__BASE__';
  const TAB = '__TAB__';
  const post = (message) => {
    try {
      fetch(BASE + 'r', { method: 'POST', body: JSON.stringify({ tab: TAB, ...message }), mode: 'no-cors', keepalive: true }).catch(() => {});
    } catch {
      // A page policy may block it; the app then simply sees nothing from this page.
    }
  };
  const cut = (text, max) => (text.length > max ? text.slice(0, max) + '…' : text);
  const show = (value) => {
    try {
      if (typeof value === 'string') return value;
      if (value instanceof Error) return value.stack || value.message;
      return cut(JSON.stringify(value), 800);
    } catch {
      return String(value);
    }
  };

  // ---- console ----
  let quiet = false; // while the app takes a screenshot, the library's own warnings are not the page's
  for (const level of ['log', 'info', 'warn', 'error', 'debug']) {
    const original = console[level].bind(console);
    console[level] = (...args) => {
      original(...args);
      if (quiet) return;
      post({ kind: 'console', level, text: cut(args.map(show).join(' '), 2000), at: Date.now() });
    };
  }
  window.addEventListener('error', (event) =>
    post({ kind: 'console', level: 'error', text: cut(`${event.message} (${event.filename}:${event.lineno})`, 2000), at: Date.now() }),
  );
  window.addEventListener('unhandledrejection', (event) =>
    post({ kind: 'console', level: 'error', text: cut('Unhandled rejection: ' + show(event.reason), 2000), at: Date.now() }),
  );

  // ---- network ----
  const record = (method, url, status, started, body) =>
    post({ kind: 'network', method, url: cut(String(url), 400), status, ms: Math.round(performance.now() - started), at: Date.now(), body: body ? cut(String(body), 1500) : undefined });
  const originalFetch = window.fetch.bind(window);
  window.fetch = (input, init) => {
    const url = typeof input === 'string' ? input : input.url;
    if (String(url).startsWith(BASE)) return originalFetch(input, init);
    const started = performance.now();
    const method = (init && init.method) || (typeof input !== 'string' && input.method) || 'GET';
    return originalFetch(input, init).then(
      (response) => {
        if (response.status >= 400) {
          response.clone().text().then((text) => record(method, url, response.status, started, text), () => record(method, url, response.status, started));
        } else {
          record(method, url, response.status, started);
        }
        return response;
      },
      (error) => (record(method, url, 0, started), Promise.reject(error)),
    );
  };
  const open = XMLHttpRequest.prototype.open;
  XMLHttpRequest.prototype.open = function (method, url, ...rest) {
    const started = performance.now();
    this.addEventListener('loadend', () => {
      let text;
      try {
        if (this.status >= 400 && (this.responseType === '' || this.responseType === 'text')) text = this.responseText;
      } catch {
        // The body is not readable.
      }
      record(method, url, this.status, started, text);
    });
    return open.call(this, method, url, ...rest);
  };

  // ---- navigation ----
  let last = '';
  const announce = () => {
    if (location.href === last) return;
    last = location.href;
    const icon = document.querySelector('link[rel~="icon"]');
    post({ kind: 'nav', url: location.href, title: document.title, favicon: icon ? icon.href : '' });
  };
  for (const name of ['pushState', 'replaceState']) {
    const original = history[name];
    history[name] = function (...args) {
      const result = original.apply(this, args);
      setTimeout(announce, 0);
      return result;
    };
  }
  window.addEventListener('popstate', announce);
  window.addEventListener('hashchange', announce);
  window.addEventListener('load', () => { last = ''; announce(); });
  // The title can change after load (single page apps); the document does not exist yet when this script starts.
  document.addEventListener('DOMContentLoaded', () => {
    const target = document.querySelector('title') || document.head || document.documentElement;
    if (target) new MutationObserver(() => post({ kind: 'title', title: document.title })).observe(target, { subtree: true, childList: true, characterData: true });
  });
  document.addEventListener('DOMContentLoaded', announce);

  // ---- DOM, selectors, picking ----
  const selectorOf = (element) => {
    const parts = [];
    for (let node = element; node && node.nodeType === 1 && parts.length < 6; node = node.parentElement) {
      if (node.id) {
        parts.unshift('#' + CSS.escape(node.id));
        break;
      }
      let part = node.tagName.toLowerCase();
      const classes = [...node.classList].slice(0, 2).map((name) => '.' + CSS.escape(name)).join('');
      part += classes;
      const siblings = node.parentElement ? [...node.parentElement.children].filter((child) => child.tagName === node.tagName) : [];
      if (siblings.length > 1) part += `:nth-of-type(${siblings.indexOf(node) + 1})`;
      parts.unshift(part);
    }
    return parts.join(' > ');
  };
  const describe = (element) => {
    const rect = element.getBoundingClientRect();
    const style = getComputedStyle(element);
    const pick = (names) => Object.fromEntries(names.map((name) => [name, style.getPropertyValue(name)]));
    return {
      selector: selectorOf(element),
      tag: element.tagName.toLowerCase(),
      text: cut((element.innerText || '').trim().replace(/\s+/g, ' '), 300),
      html: cut(element.outerHTML, 3000),
      box: { x: Math.round(rect.x), y: Math.round(rect.y), w: Math.round(rect.width), h: Math.round(rect.height) },
      style: pick(['color', 'background-color', 'font-family', 'font-size', 'font-weight', 'display', 'position', 'margin', 'padding', 'border']),
    };
  };

  // The inspector: the element under the pointer is outlined live with its tag and size; a click picks it.
  let picker = null;
  const stopPicking = () => {
    if (!picker) return;
    picker.overlay.remove();
    document.removeEventListener('mousemove', picker.move, true);
    document.removeEventListener('click', picker.click, true);
    document.removeEventListener('keydown', picker.key, true);
    picker = null;
  };
  const startPicking = (id) => {
    stopPicking();
    const overlay = document.createElement('div');
    overlay.style.cssText = 'position:fixed;z-index:2147483647;pointer-events:none;border:2px solid #7c5cff;background:rgba(124,92,255,.15);border-radius:2px;transition:all .05s';
    const tag = document.createElement('span');
    tag.style.cssText = 'position:absolute;left:-2px;bottom:100%;margin-bottom:2px;padding:1px 6px;border-radius:4px;background:#7c5cff;color:#fff;font:11px ui-monospace,monospace;white-space:nowrap';
    overlay.appendChild(tag);
    document.documentElement.appendChild(overlay);
    let target = null;
    const move = (event) => {
      target = document.elementFromPoint(event.clientX, event.clientY);
      if (!target || target === overlay) return;
      const rect = target.getBoundingClientRect();
      Object.assign(overlay.style, { left: rect.x + 'px', top: Math.max(0, rect.y) + 'px', width: rect.width + 'px', height: rect.height + 'px' });
      tag.textContent = `${target.tagName.toLowerCase()} · ${Math.round(rect.width)}×${Math.round(rect.height)}`;
    };
    const click = (event) => {
      event.preventDefault();
      event.stopPropagation();
      if (target) post({ kind: 'pick', id, element: describe(target), url: location.href, title: document.title });
      stopPicking();
    };
    const key = (event) => {
      if (event.key === 'Escape') {
        post({ kind: 'pick', id, element: null });
        stopPicking();
      }
    };
    document.addEventListener('mousemove', move, true);
    document.addEventListener('click', click, true);
    document.addEventListener('keydown', key, true);
    picker = { overlay, move, click, key };
  };

  // A phone or tablet has no desktop scrollbars and no hover cursor; show the page that way.
  if (/iPhone|iPad/.test(navigator.userAgent)) {
    document.addEventListener('DOMContentLoaded', () => {
      const style = document.createElement('style');
      style.textContent = '::-webkit-scrollbar{display:none!important}html{scrollbar-width:none!important}*{-webkit-tap-highlight-color:transparent}';
      (document.head || document.documentElement).appendChild(style);
    });
  }

  // ---- requests from the app ----
  window.__agentdeck = {
    run(id, kind, arg) {
      if (kind === 'dom') {
        let html;
        try {
          html = typeof arg === 'string' && arg ? [...document.querySelectorAll(arg)].slice(0, 10).map((node) => node.outerHTML).join('\n') : document.documentElement.outerHTML;
        } catch (error) {
          html = 'Invalid selector: ' + error.message;
        }
        post({
          kind: 'dom',
          id,
          url: location.href,
          title: document.title,
          viewport: { w: innerWidth, h: innerHeight, dpr: devicePixelRatio },
          html: cut(html, 60000),
        });
      } else if (kind === 'eval') {
        Promise.resolve()
          .then(() => (0, eval)(String(arg)))
          .then(
            (value) => post({ kind: 'eval', id, result: cut(show(value), 20000) }),
            (error) => post({ kind: 'eval', id, error: String((error && error.stack) || error) }),
          );
      } else if (kind === 'shot') {
        const lib = window.htmlToImage;
        if (!lib) return post({ kind: 'shot', id, error: 'screenshot library missing' });
        quiet = true;
        const options = {
          cacheBust: true,
          skipFonts: true,
          imagePlaceholder: 'data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7',
          pixelRatio: 1,
          width: innerWidth,
          height: innerHeight,
          style: { transform: `translate(${-scrollX}px, ${-scrollY}px)`, transformOrigin: 'top left' },
        };
        const media = ['IFRAME', 'VIDEO', 'SCRIPT'];
        // First try with pictures; a page whose images cannot be read still gets a layout-only capture.
        lib
          .toPng(document.documentElement, { ...options, filter: (node) => !media.includes(node.tagName) })
          .catch(() => lib.toPng(document.documentElement, { ...options, filter: (node) => ![...media, 'IMG', 'PICTURE', 'CANVAS', 'SVG', 'LINK'].includes(String(node.tagName).toUpperCase()) }))
          .then((data) => post({ kind: 'shot', id, data, w: innerWidth, h: innerHeight }))
          .catch((error) => post({ kind: 'shot', id, error: error && error.message ? error.message : 'the page could not be captured (' + (error && error.type ? error.type : 'unknown') + ')' }))
          .finally(() => (quiet = false));
      } else if (kind === 'at') {
        const element = document.elementFromPoint(arg.x, arg.y);
        post({ kind: 'at', id, element: element ? describe(element) : null });
      } else if (kind === 'pick') {
        startPicking(id);
      } else if (kind === 'unpick') {
        stopPicking();
      }
    },
  };
})();
