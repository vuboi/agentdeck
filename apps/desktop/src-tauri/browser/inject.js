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

  // ---- presses ----
  // The app cannot see clicks on the page; a menu of the app open over it closes on this.
  addEventListener('pointerdown', () => post({ kind: 'page-pointer' }), true);

  // ---- network ----
  // The response of every text request is kept (up to 20 000 characters), and what was sent: that is what
  // browser_network gives back to write a DTO or to see why a call failed.
  const record = (method, url, status, started, body, request) =>
    post({
      kind: 'network',
      method,
      url: cut(String(url), 400),
      status,
      ms: Math.round(performance.now() - started),
      at: Date.now(),
      body: body ? cut(String(body), 20000) : undefined,
      request: typeof request === 'string' && request ? cut(request, 4000) : undefined,
    });
  const textual = (type) => /json|text|xml|javascript|html|x-www-form/i.test(type || '');
  const originalFetch = window.fetch.bind(window);
  window.fetch = (input, init) => {
    const url = typeof input === 'string' ? input : input.url;
    if (String(url).startsWith(BASE)) return originalFetch(input, init);
    const started = performance.now();
    const method = (init && init.method) || (typeof input !== 'string' && input.method) || 'GET';
    return originalFetch(input, init).then(
      (response) => {
        const sent = init && init.body;
        if (response.status >= 400 || textual(response.headers.get('content-type'))) {
          response.clone().text().then((text) => record(method, url, response.status, started, text, sent), () => record(method, url, response.status, started, undefined, sent));
        } else {
          record(method, url, response.status, started, undefined, sent);
        }
        return response;
      },
      (error) => (record(method, url, 0, started), Promise.reject(error)),
    );
  };
  const open = XMLHttpRequest.prototype.open;
  const send = XMLHttpRequest.prototype.send;
  XMLHttpRequest.prototype.send = function (body) {
    this.__sent = typeof body === 'string' ? body : undefined;
    return send.call(this, body);
  };
  XMLHttpRequest.prototype.open = function (method, url, ...rest) {
    const started = performance.now();
    this.addEventListener('loadend', () => {
      let text;
      try {
        if ((this.status >= 400 || textual(this.getResponseHeader('content-type'))) && (this.responseType === '' || this.responseType === 'text')) text = this.responseText;
      } catch {
        // The body is not readable.
      }
      record(method, url, this.status, started, text, this.__sent);
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
  // Where the element sits among its neighbours: what comes before and after it in its parent. A model that has to put
  // something "next to" or "after" an element needs this; the element's own markup does not say it.
  const around = (element) => {
    const parent = element.parentElement;
    if (!parent) return '';
    const brief = (node) => {
      const classes = [...node.classList].slice(0, 3).map((name) => '.' + name).join('');
      const text = cut((node.innerText || node.getAttribute('aria-label') || '').trim().replace(/\s+/g, ' '), 40);
      return `<${node.tagName.toLowerCase()}${node.id ? '#' + node.id : ''}${classes}>${text ? ' ' + text : ''}`;
    };
    const siblings = [...parent.children];
    const at = siblings.indexOf(element);
    const from = Math.max(0, at - 3);
    const lines = siblings.slice(from, at + 4).map((node, index) => (from + index === at ? '→ ' : '  ') + brief(node) + (from + index === at ? '   (the element)' : ''));
    return [
      `Inside ${brief(parent)} (selector ${selectorOf(parent)}), child ${at + 1} of ${siblings.length}:`,
      from > 0 ? '  …' : '',
      ...lines,
      at + 4 < siblings.length ? '  …' : '',
    ]
      .filter(Boolean)
      .join('\n');
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
      around: around(element),
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
    overlay.style.cssText = 'position:fixed;z-index:2147483647;pointer-events:none;border:1.5px dashed #7c5cff;background:rgba(124,92,255,.12);border-radius:2px;transition:all .05s';
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
    picker = { overlay, move, click, key, id };
  };

  // A phone or tablet has no desktop scrollbars and no hover cursor; show the page that way.
  if (/iPhone|iPad/.test(navigator.userAgent)) {
    document.addEventListener('DOMContentLoaded', () => {
      const style = document.createElement('style');
      style.textContent = '::-webkit-scrollbar{display:none!important}html{scrollbar-width:none!important}*{-webkit-tap-highlight-color:transparent}';
      (document.head || document.documentElement).appendChild(style);
    });
  }

  // ---- annotations: arrows, boxes, notes and element comments, drawn on the live page ----
  // Shapes live in page coordinates inside one absolutely placed layer, so they scroll with the page and a capture of
  // the page (html-to-image) carries them. The app asks for a tool, then for the notes, then clears it all.
  const ANN_COLOR = '#7c5cff';
  let ann = null;

  const annBadge = (note, x, y) => {
    const badge = document.createElement('div');
    badge.textContent = String(note.n);
    badge.style.cssText = `position:absolute;left:${x - 10}px;top:${y - 10}px;width:20px;height:20px;border-radius:50%;background:${ANN_COLOR};color:#fff;font:600 11px/20px system-ui,sans-serif;text-align:center;box-shadow:0 1px 4px rgba(0,0,0,.35)`;
    const picked = ann.selected === note;
    if (picked) badge.style.boxShadow = '0 0 0 2px #fff, 0 1px 4px rgba(0,0,0,.45)';
    const open = (event) => {
      event.preventDefault();
      event.stopPropagation();
      annSelect(note);
    };
    badge.setAttribute('data-ann-ui', '');
    badge.title = 'Edit this note';
    badge.style.pointerEvents = 'auto';
    badge.style.cursor = 'pointer';
    badge.addEventListener('click', open);
    ann.layer.appendChild(badge);
    if (note.text) {
      const label = document.createElement('div');
      label.textContent = note.text;
      label.setAttribute('data-ann-ui', '');
      label.title = 'Edit this note';
      label.style.cssText = `position:absolute;left:${x + 14}px;top:${y - 10}px;max-width:240px;padding:2px 8px;border-radius:6px;background:${ANN_COLOR};color:#fff;font:12px/1.4 system-ui,sans-serif;white-space:pre-wrap;word-break:break-word;box-shadow:${picked ? '0 0 0 2px #fff, ' : ''}0 1px 4px rgba(0,0,0,.35);pointer-events:auto;cursor:pointer`;
      label.addEventListener('click', open);
      ann.layer.appendChild(label);
    }
  };

  const annShape = (note) => {
    const svg = (tag, attrs) => {
      const node = document.createElementNS('http://www.w3.org/2000/svg', tag);
      for (const [key, value] of Object.entries(attrs)) node.setAttribute(key, String(value));
      ann.svg.appendChild(node);
    };
    if (note.type === 'pick') {
      // An element is outlined exactly as the inspector does it: a thin dashed CSS border, not a stroke.
      const outline = document.createElement('div');
      outline.style.cssText = `position:absolute;box-sizing:border-box;left:${note.x1}px;top:${note.y1}px;width:${note.x2 - note.x1}px;height:${note.y2 - note.y1}px;border:1.5px dashed ${ANN_COLOR};background:rgba(124,92,255,.12);border-radius:2px`;
      ann.layer.appendChild(outline);
    } else if (note.type === 'arrow') {
      svg('line', { x1: note.x1, y1: note.y1, x2: note.x2, y2: note.y2, stroke: ANN_COLOR, 'stroke-width': 3, 'stroke-linecap': 'round', 'marker-end': 'url(#__agentdeck_head)' });
    } else if (note.type === 'box') {
      svg('rect', {
        x: note.x1,
        y: note.y1,
        width: note.x2 - note.x1,
        height: note.y2 - note.y1,
        rx: 4,
        fill: 'rgba(124,92,255,.08)',
        stroke: ANN_COLOR,
        'stroke-width': 1.75,
      });
    }
  };

  const annRender = () => {
    if (!ann) return;
    for (const node of [...ann.svg.children]) if (node.tagName.toLowerCase() !== 'defs') node.remove();
    // The comment box being written stays; badges and labels are drawn again.
    for (const node of [...ann.layer.children]) if (node !== ann.editing) node.remove();
    for (const note of ann.notes) {
      annShape(note);
      annBadge(note, note.x1, note.y1);
    }
    const d = ann.drag;
    if (d) annShape({ type: ann.tool === 'arrow' ? 'arrow' : 'box', x1: ann.tool === 'arrow' ? d.x0 : Math.min(d.x0, d.x), y1: ann.tool === 'arrow' ? d.y0 : Math.min(d.y0, d.y), x2: ann.tool === 'arrow' ? d.x : Math.max(d.x0, d.x), y2: ann.tool === 'arrow' ? d.y : Math.max(d.y0, d.y) });
    // The element under the pointer while picking one.
    const h = ann.editing ? null : ann.hover;
    if (h) annShape({ type: 'pick', x1: h.x1, y1: h.y1, x2: h.x2, y2: h.y2 });
  };

  const annReport = () => post({ kind: 'ann-count', count: ann ? ann.notes.length : 0, undo: !!ann && ann.past.length > 0, redo: !!ann && ann.future.length > 0, selected: !!ann && !!ann.selected });

  const annExport = () =>
    (ann ? ann.notes : []).map((note) => ({
      n: note.n,
      type: note.type,
      text: note.text,
      x: Math.round(note.x1),
      y: Math.round(note.y1),
      element: note.element,
    }));

  // The comment box beside a new note. Enter keeps it, Escape drops a note nobody wrote on.
  const annEdit = (note, isNew) => {
    // Writing a note: nothing under the pointer is inspected until it is done.
    ann.hover = null;
    const before = JSON.stringify(ann.notes);
    const original = note.text;
    const box = document.createElement('textarea');
    box.setAttribute('data-ann-ui', '');
    box.rows = 2;
    box.placeholder = 'Add a note…';
    box.value = original;
    box.style.cssText = `position:absolute;left:0px;top:0px;width:220px;resize:none;padding:6px 8px;border:1px solid ${ANN_COLOR};border-radius:8px;background:#1b1b1b;color:#eee;font:12px/1.4 system-ui,sans-serif;outline:none;pointer-events:auto;z-index:2;box-shadow:0 4px 14px rgba(0,0,0,.4)`;
    ann.layer.appendChild(box);
    // Beside the note, kept inside the visible page: too close to an edge, it steps back from it (below a note at the
    // top, above one at the bottom, left of one at the right edge).
    const room = 8;
    const wide = box.offsetWidth || 220;
    const tall = box.offsetHeight || 56;
    const view = { left: window.scrollX, top: window.scrollY, right: window.scrollX + window.innerWidth, bottom: window.scrollY + window.innerHeight };
    let left = note.x1 + 14;
    let top = note.y1 + 14;
    if (left + wide > view.right - room) left = note.x1 - 14 - wide;
    if (top + tall > view.bottom - room) top = note.y1 - 14 - tall;
    box.style.left = `${Math.max(view.left + room, left)}px`;
    box.style.top = `${Math.max(view.top + room, top)}px`;
    ann.editing = box;
    annRender();
    box.focus();
    box.setSelectionRange(box.value.length, box.value.length);
    let done = false;
    const finish = (keep) => {
      if (done) return;
      done = true;
      box.remove();
      if (ann) ann.editing = null;
      if (!ann) return;
      const text = keep ? box.value.trim() : isNew ? '' : original;
      if (isNew) {
        note.text = text;
        // A pin with no words says nothing: it goes (and its step with it). A shape or an outline stays as it is.
        if (!text && note.type === 'note') {
          ann.notes = ann.notes.filter((item) => item !== note);
          ann.past.pop();
        }
      } else if (text !== original) {
        ann.past.push(before);
        if (ann.past.length > 50) ann.past.shift();
        ann.future = [];
        note.text = text;
        if (!text && note.type === 'note') ann.notes = ann.notes.filter((item) => item !== note);
      }
      // A note that was clicked stays selected; a note just made is not.
      ann.selected = !isNew && ann.notes.includes(note) ? note : null;
      annRenumber();
      annRender();
      annReport();
    };
    box.addEventListener('keydown', (event) => {
      event.stopPropagation();
      if (event.key === 'Enter' && !event.shiftKey) {
        event.preventDefault();
        finish(true);
      } else if (event.key === 'Escape') {
        event.preventDefault();
        finish(false);
      }
    });
    box.addEventListener('blur', () => finish(true));
  };

  // A click on a note selects it and opens it again for writing.
  const annSelect = (note) => {
    if (ann.editing) ann.editing.blur();
    ann.selected = note;
    annRender();
    annReport();
    annEdit(note, false);
  };

  // The history: the notes before each change, as text. A new change forgets what was undone.
  const annCommit = () => {
    ann.past.push(JSON.stringify(ann.notes));
    if (ann.past.length > 50) ann.past.shift();
    ann.future = [];
  };
  const annRenumber = () => ann.notes.forEach((item, index) => (item.n = index + 1));
  const annDelete = (note) => {
    annCommit();
    if (ann.selected === note) ann.selected = null;
    ann.notes = ann.notes.filter((item) => item !== note);
    annRenumber();
    annRender();
    annReport();
  };
  const annRemoveAll = () => {
    if (ann.editing) ann.editing.blur();
    if (ann.notes.length === 0) return;
    annCommit();
    ann.selected = null;
    ann.notes = [];
    annRender();
    annReport();
  };
  // What a delete button does: the selected note goes; with none selected, all of them.
  const annRemove = () => (ann && ann.selected ? annDelete(ann.selected) : annRemoveAll());
  const annUndo = () => {
    if (!ann || ann.editing || ann.past.length === 0) return;
    ann.selected = null;
    ann.future.push(JSON.stringify(ann.notes));
    ann.notes = JSON.parse(ann.past.pop());
    annRender();
    annReport();
  };
  const annRedo = () => {
    if (!ann || ann.editing || ann.future.length === 0) return;
    ann.selected = null;
    ann.past.push(JSON.stringify(ann.notes));
    ann.notes = JSON.parse(ann.future.pop());
    annRender();
    annReport();
  };
  // Ctrl+Z undoes, Ctrl+Shift+Z or Ctrl+Y redoes, while marking up and not writing a comment.
  const annKey = (event) => {
    if (!ann || ann.editing) return;
    if ((event.key === 'Delete' || event.key === 'Backspace') && ann.selected) {
      event.preventDefault();
      event.stopPropagation();
      annRemove();
      return;
    }
    if (!(event.ctrlKey || event.metaKey)) return;
    const key = event.key.toLowerCase();
    if (key === 'z' && !event.shiftKey) annUndo();
    else if ((key === 'z' && event.shiftKey) || key === 'y') annRedo();
    else return;
    event.preventDefault();
    event.stopPropagation();
  };

  const annAdd = (note) => {
    annCommit();
    ann.notes.push({ ...note, n: ann.notes.length + 1, text: '' });
    ann.hover = null;
    annRender();
    annReport();
    annEdit(ann.notes[ann.notes.length - 1], true);
  };

  const annInUi = (event) => event.target && event.target.closest && event.target.closest('[data-ann-ui]');
  const annAt = (event) => ({ x: event.pageX, y: event.pageY });
  const annElement = (event) => {
    const target = document.elementFromPoint(event.clientX, event.clientY);
    return target && target !== document.documentElement && target !== document.body ? target : null;
  };
  const annRect = (element) => {
    const r = element.getBoundingClientRect();
    return { x1: r.left + scrollX, y1: r.top + scrollY, x2: r.right + scrollX, y2: r.bottom + scrollY };
  };

  const annMove = (event) => {
    if (!ann || annInUi(event) || ann.editing) return;
    if (ann.drag) {
      Object.assign(ann.drag, annAt(event));
      annRender();
    } else if (ann.tool === 'pick') {
      const element = annElement(event);
      ann.hover = element ? annRect(element) : null;
      annRender();
    }
  };

  const annDown = (event) => {
    if (!ann || annInUi(event) || event.button !== 0) return;
    // A click elsewhere closes the box being written (its blur keeps the note) and starts nothing new.
    if (ann.editing) return;
    event.preventDefault();
    event.stopPropagation();
    const at = annAt(event);
    if (ann.tool === 'arrow' || ann.tool === 'box') ann.drag = { x0: at.x, y0: at.y, ...at };
  };

  const annUp = (event) => {
    if (!ann || !ann.drag) return;
    event.preventDefault();
    event.stopPropagation();
    const d = ann.drag;
    ann.drag = null;
    if (Math.hypot(d.x - d.x0, d.y - d.y0) < 12) return annRender();
    if (ann.tool === 'arrow') annAdd({ type: 'arrow', x1: d.x0, y1: d.y0, x2: d.x, y2: d.y });
    else annAdd({ type: 'box', x1: Math.min(d.x0, d.x), y1: Math.min(d.y0, d.y), x2: Math.max(d.x0, d.x), y2: Math.max(d.y0, d.y) });
  };

  const annClick = (event) => {
    if (!ann || annInUi(event)) return;
    // The page never sees a click while marking it up.
    event.preventDefault();
    event.stopPropagation();
    if (ann.editing) return ann.editing.blur();
    if (ann.selected) {
      ann.selected = null;
      annRender();
      annReport();
    }
    if (ann.tool === 'note') {
      const at = annAt(event);
      annAdd({ type: 'note', x1: at.x, y1: at.y, x2: at.x, y2: at.y });
    } else if (ann.tool === 'pick') {
      const element = annElement(event);
      if (!element) return;
      const info = describe(element);
      ann.hover = null;
      annAdd({ type: 'pick', ...annRect(element), element: { tag: info.tag, text: info.text, selector: info.selector, html: info.html, box: info.box, style: info.style } });
    }
  };

  const annStart = (tool) => {
    if (!ann) {
      const root = document.createElement('div');
      root.id = '__agentdeck_ann';
      const width = Math.max(document.documentElement.scrollWidth, innerWidth);
      const height = Math.max(document.documentElement.scrollHeight, innerHeight);
      root.style.cssText = `position:absolute;left:0;top:0;width:${width}px;height:${height}px;z-index:2147483646;pointer-events:none`;
      const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
      svg.setAttribute('width', String(width));
      svg.setAttribute('height', String(height));
      svg.style.cssText = 'position:absolute;left:0;top:0;overflow:visible';
      svg.innerHTML = `<defs><marker id="__agentdeck_head" viewBox="0 0 10 10" refX="8" refY="5" markerWidth="6" markerHeight="6" orient="auto"><path d="M0 0L10 5L0 10z" fill="${ANN_COLOR}"/></marker></defs>`;
      const layer = document.createElement('div');
      layer.style.cssText = 'position:absolute;left:0;top:0;width:100%;height:100%';
      root.append(svg, layer);
      document.documentElement.appendChild(root);
      document.documentElement.style.cursor = 'crosshair';
      ann = { tool, notes: [], selected: null, past: [], future: [], root, svg, layer, drag: null, hover: null, editing: null };
      document.addEventListener('mousemove', annMove, true);
      document.addEventListener('mousedown', annDown, true);
      document.addEventListener('mouseup', annUp, true);
      document.addEventListener('click', annClick, true);
      document.addEventListener('keydown', annKey, true);
    }
    ann.tool = tool;
    ann.hover = null;
    annRender();
  };

  const annStop = () => {
    if (!ann) return;
    document.removeEventListener('mousemove', annMove, true);
    document.removeEventListener('mousedown', annDown, true);
    document.removeEventListener('mouseup', annUp, true);
    document.removeEventListener('click', annClick, true);
    document.removeEventListener('keydown', annKey, true);
    ann.root.remove();
    document.documentElement.style.cursor = '';
    ann = null;
    annReport();
  };

  // ---- the backdrop of a dialog open in the app ----
  // The app's dialogs dim and blur everything behind them; this page is behind them too, and only the page itself
  // can blur its own content. It fades exactly like the app's backdrop (z-ui: 200ms in, ease-out-expo).
  let shade = null;
  let shadeGone = 0;
  const showShade = (look) => {
    clearTimeout(shadeGone);
    if (look) {
      if (!shade) {
        shade = document.createElement('div');
        shade.id = '__agentdeck_shade';
      }
      if (!shade.isConnected) document.documentElement.appendChild(shade);
      shade.style.cssText = `position:fixed;inset:0;z-index:2147483646;will-change:opacity;opacity:0;${look}`;
      // Started from where it is now, so the fade begins on the very next frame.
      shade.animate([{ opacity: 0 }, { opacity: 1 }], { duration: 200, easing: 'cubic-bezier(0.16, 1, 0.3, 1)', fill: 'forwards' });
    } else if (shade && shade.isConnected) {
      shade.getAnimations().forEach((animation) => animation.cancel());
      shade.style.opacity = '1';
      shade.animate([{ opacity: 1 }, { opacity: 0 }], { duration: 150, easing: 'cubic-bezier(0.4, 0, 1, 1)', fill: 'forwards' });
      shadeGone = setTimeout(() => shade.remove(), 160);
    }
  };

  // ---- requests from the app ----
  window.__agentdeck = {
    shade: showShade,
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
      } else if (kind === 'click') {
        // The events a real press makes, at a point of the page, on whatever is there.
        let result;
        try {
          const el = document.elementFromPoint(arg.x, arg.y);
          if (!el) throw new Error('nothing at ' + arg.x + ',' + arg.y);
          const opts = { bubbles: true, cancelable: true, composed: true, clientX: arg.x, clientY: arg.y, button: 0, buttons: 1 };
          el.dispatchEvent(new PointerEvent('pointerdown', opts));
          el.dispatchEvent(new MouseEvent('mousedown', opts));
          if (el.focus) el.focus();
          el.dispatchEvent(new PointerEvent('pointerup', { ...opts, buttons: 0 }));
          el.dispatchEvent(new MouseEvent('mouseup', { ...opts, buttons: 0 }));
          el.dispatchEvent(new MouseEvent('click', { ...opts, buttons: 0 }));
          result = { tag: el.tagName.toLowerCase(), text: cut((el.textContent || '').trim(), 120), id: el.id || undefined, class: cut(el.getAttribute('class') || '', 120) };
          post({ kind: 'click', id, result });
        } catch (error) {
          post({ kind: 'click', id, error: String(error.message || error) });
        }
      } else if (kind === 'fill') {
        // Types into a field the way a user does: focus, value, then the events frameworks listen for.
        try {
          const el = document.querySelector(arg.selector);
          if (!el) throw new Error('no element matches ' + arg.selector);
          el.focus();
          const proto = Object.getPrototypeOf(el);
          const setter = Object.getOwnPropertyDescriptor(proto, 'value')?.set;
          if (setter) setter.call(el, arg.value); else el.value = arg.value;
          el.dispatchEvent(new Event('input', { bubbles: true }));
          el.dispatchEvent(new Event('change', { bubbles: true }));
          post({ kind: 'fill', id, result: { tag: el.tagName.toLowerCase(), value: cut(String(el.value), 200) } });
        } catch (error) {
          post({ kind: 'fill', id, error: String(error.message || error) });
        }
      } else if (kind === 'drag') {
        // HTML5 drag and drop from one element onto another: the events a browser sends for the same gesture.
        try {
          const from = document.querySelector(arg.from);
          const to = document.querySelector(arg.to);
          if (!from) throw new Error('no element matches ' + arg.from);
          if (!to) throw new Error('no element matches ' + arg.to);
          const data = new DataTransfer();
          const fire = (el, type) => el.dispatchEvent(new DragEvent(type, { bubbles: true, cancelable: true, composed: true, dataTransfer: data }));
          fire(from, 'dragstart');
          fire(to, 'dragenter');
          fire(to, 'dragover');
          fire(to, 'drop');
          fire(from, 'dragend');
          post({ kind: 'drag', id, result: { from: from.tagName.toLowerCase(), to: to.tagName.toLowerCase() } });
        } catch (error) {
          post({ kind: 'drag', id, error: String(error.message || error) });
        }
      } else if (kind === 'upload') {
        // A file as the user gives it: set on a file input, or dropped onto any element.
        try {
          const el = document.querySelector(arg.selector);
          if (!el) throw new Error('no element matches ' + arg.selector);
          const bytes = Uint8Array.from(atob(arg.data), (c) => c.charCodeAt(0));
          const file = new File([bytes], arg.name, { type: arg.type });
          const data = new DataTransfer();
          data.items.add(file);
          if (el.tagName === 'INPUT' && el.type === 'file') {
            el.files = data.files;
            el.dispatchEvent(new Event('input', { bubbles: true }));
            el.dispatchEvent(new Event('change', { bubbles: true }));
          } else {
            for (const type of ['dragenter', 'dragover', 'drop']) {
              el.dispatchEvent(new DragEvent(type, { bubbles: true, cancelable: true, composed: true, dataTransfer: data }));
            }
          }
          post({ kind: 'upload', id, result: { name: file.name, size: file.size, target: el.tagName.toLowerCase() } });
        } catch (error) {
          post({ kind: 'upload', id, error: String(error.message || error) });
        }
      } else if (kind === 'eval') {
        Promise.resolve()
          .then(() => {
            try {
              return (0, eval)(String(arg));
            } catch (error) {
              // Top-level await is not valid in eval: run it inside an async function instead.
              if (!(error instanceof SyntaxError) || !/await/.test(error.message)) throw error;
              return (0, eval)('(async () => {' + String(arg) + '\n})()');
            }
          })
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
        annStop();
        startPicking(id);
      } else if (kind === 'ann') {
        // arg: { tool } starts or switches the tools, { state: true } only answers, { clear: true } ends it.
        if (arg && arg.clear) annStop();
        else if (arg && arg.undo) annUndo();
        else if (arg && arg.redo) annRedo();
        else if (arg && arg.remove) annRemove();
        else if (arg && arg.tool) {
          stopPicking();
          annStart(arg.tool);
        }
        post({ kind: 'ann', id, result: { notes: annExport() } });
      } else if (kind === 'unpick') {
        // The app is still waiting on this pick: answer it, so its Inspect button turns off.
        if (picker) post({ kind: 'pick', id: picker.id, element: null });
        stopPicking();
      }
    },
  };
})();
