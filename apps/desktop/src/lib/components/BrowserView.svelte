<script lang="ts">
  import MenuButton from './MenuButton.svelte';
  import { guide } from '../guide.svelte';
  import { fly } from 'svelte/transition';
  import { onDestroy, onMount, tick, untrack } from 'svelte';
  import { revealTab } from '../tab-strip';
  import { ZDrawer, ZIcon, ZLoading, ZPopover, zTooltip } from '@shival99/z-ui-svelte';
  import { toast } from '../toast';
  import type { ZDropdownMenuItem, ZIconName } from '@shival99/z-ui-svelte';
  import {
    activeTab,
    back,
    browser,
    annotate,
    markHistory,
    parkBrowser,
    cancelPick,
    pickElement,
    annotationNotes,
    type AnnotateTool,
    stopAnnotating,
    closeTab,
    DEVICES,
    MAX_TABS,
    forgetVisit,
    forward,
    goTo,
    capturePageIfPossible,
    selectCaptureRegion,
    hideBrowser,
    listenToPage,
    matchHistory,
    moveBrowser,
    newTab,
    openExternal,
    recreateBrowser,
    detectTargets,
    refreshServers,
    servers,
    stopServers,
    openInPreview,
    runTarget as startTarget,
    asRunFailure,
    failureText,
    addBrowserExtension,
    terminalCommand,
    type RunTarget,
    reload,
    searchSuggestions,
    selectTab,
    showBrowser,
    stop,
    tabsFull,
    zoomBrowser,
    captureElement,
    type Device,
    type Rect,
  } from '../browser.svelte';
  import { inTauri } from '../history';
  import { trackRun } from '../activity.svelte';
  import RunCard from './RunCard.svelte';
  import ScreenshotEditor from './ScreenshotEditor.svelte';
  import TabListButton from './TabListButton.svelte';
  import type { PickerItem } from './PickerPopover.svelte';
  import { blurPage, cutHole, modalBlur, overPage, setBelow } from '../page-layer';
  import { devtoolsOpen, layout, setDevtools } from '../layout.svelte';
  import { runInTerminal, showTerminalTab, stopTerminalRun, terminal, terminalRuns } from '../terminal.svelte';
  import { activeSession, draftElements, draftImages, drafts, projectOf, workspace } from '../sessions.svelte';

  // The AgentDeck browser: tabs, an address bar, device sizes, an element picker, devtools, and a markup view
  // whose result goes to the chat. The pages themselves are native webviews placed over the stage below.
  let stage: HTMLDivElement | undefined = $state();
  let address = $state('');
  let addressInput: HTMLInputElement | undefined = $state();
  // A click that focuses the address selects it all; the mouse button coming up would put the caret back and undo it.
  let focusedByClick = false;
  /** Selects the whole address, and again a moment later: whatever deselects it right after focus (the caret put back by
   *  the mouse, a value written to the field) is undone, until the user has typed something. */
  function selectAddress(input: HTMLInputElement) {
    input.select();
    for (const delay of [0, 50, 150, 400]) {
      setTimeout(() => {
        if (document.activeElement === input && !dirty) input.select();
      }, delay);
    }
  }

  // Ctrl+L: the address, selected, ready to type over, while this browser is the one on screen.
  function focusAddress() {
    if (!addressInput) return;
    addressInput.focus();
    selectAddress(addressInput);
  }
  onMount(() => {
    window.addEventListener('agentdeck:focus-address', focusAddress);
    return () => window.removeEventListener('agentdeck:focus-address', focusAddress);
  });
  function onWindowKey(event: KeyboardEvent) {
    if (!(event.ctrlKey || event.metaKey) || event.shiftKey || event.altKey || event.key.toLowerCase() !== 'l' || !addressInput) return;
    event.preventDefault();
    addressInput.focus();
    selectAddress(addressInput);
  }
  let editing = $state(false);
  let busy = $state('');
  /** Resolves once what was just rendered has been drawn. */
  const painted = () => new Promise<void>((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => resolve())));
  /** Where the phone / tablet sits in the stage (stage coordinates), for its frame. */
  let deviceBox = $state<{ x: number; y: number; w: number; h: number; zoom: number } | null>(null);
  /** The device frame is changing size (another device, or turned): the frame glides and the page waits, then appears. */
  let morphing = $state(false);
  let morphTimer: ReturnType<typeof setTimeout> | undefined;
  const MORPH_MS = 300;
  const glideClass = 'transition-[left,top,width,height,border-radius] duration-300 ease-[cubic-bezier(.2,.8,.2,1)] motion-reduce:transition-none';

  const tab = $derived(activeTab());
  const errors = $derived(browser.console.filter((line) => line.level === 'error').length);
  const started = $derived(Boolean(tab.url));

  // ---- run the project: what it can run, read from its files; one click starts it and opens the page ----
  const projectPath = $derived((() => {
    const session = activeSession();
    return (session && projectOf(session)?.path) || '';
  })());
  let targets = $state<RunTarget[]>([]);
  // What is being started lives in the browser state, not here: leaving the panel and coming back keeps the spinner.
  const starting = $derived(browser.starting);
  $effect(() => {
    const path = projectPath;
    let current = true;
    void detectTargets(path)
      .then((list) => current && (targets = list))
      .catch(() => current && (targets = []));
    return () => (current = false);
  });

  async function runProject(target: RunTarget) {
    const id = `${target.cwd}|${target.command}`;
    if (starting.includes(id)) return;
    browser.starting = [...browser.starting, id];
    browser.error = '';
    browser.problem = null;
    const session = activeSession();
    // The Audit log gets the run: what was started, where, and the address it came up at (or why it did not).
    const finish = trackRun(session?.id ?? '', session?.title ?? '', (session && projectOf(session)?.name) || '', `${target.command}\n${target.cwd}`);
    try {
      const url = await startTarget(target);
      finish(true, `${target.name}: ${url}`);
      // The same path a link takes: the front tab, or a new one when it already shows a page.
      openInPreview(url, 'auto');
    } catch (error) {
      const failure = asRunFailure(error);
      finish(false, failure ? failureText(failure) : String(error));
      if (failure) browser.problem = { ...failure, name: target.name, command: target.command, cwd: target.cwd };
      else browser.error = String(error);
    } finally {
      browser.starting = browser.starting.filter((item) => item !== id);
    }
  }

  /** Runs a command in the Terminal panel (in its folder): for scripts that ask questions, or to see everything they print. */
  function inTerminal(command: string, cwd: string) {
    runInTerminal(terminalCommand(command, cwd));
    browser.problem = null;
  }

  // The address bar shows the front tab's address unless it is being typed in.
  $effect(() => {
    if (!editing) address = tab.url;
  });

  /** The webview's rectangle in window coordinates: the stage itself, or a phone / tablet frame centred in it. */
  function frame(): { rect: Rect; zoom: number } | null {
    if (!stage) return null;
    const full = stage.getBoundingClientRect();
    if (full.width < 20 || full.height < 20) return null;
    const box = full;
    const device = DEVICES[browser.device];
    if (!device.w) return { rect: { x: box.x, y: box.y, w: box.width, h: box.height }, zoom: 1 };
    const [dw, dh] = browser.landscape ? [device.h, device.w] : [device.w, device.h];
    // A narrower page is a zoomed-out one: the page lays out at the device width, the panel just shows it smaller.
    const zoom = Math.min(1, (box.width - 44) / dw, (box.height - 68) / dh);
    const w = dw * zoom;
    const h = dh * zoom;
    return { rect: { x: box.x + (box.width - w) / 2, y: box.y + (box.height - h) / 2 + 6, w, h }, zoom };
  }

  // One placement at a time, the newest rectangle last: overlapping calls could land out of order and leave the page misplaced.
  let syncing = false;
  let again = false;

  async function sync() {
    if (syncing) {
      again = true;
      return;
    }
    syncing = true;
    try {
      do {
        again = false;
        await placePage();
      } while (again);
    } finally {
      syncing = false;
    }
  }

  /** Takes the page off screen. The app is filled where it was see-through first, so the window never shows through. */
  async function takePageAway() {
    cutHole(null, null);
    placed = '';
    await painted();
    await hideBrowser();
  }

  /** Where the page was last placed (tab, rectangle, zoom): the same again is not sent again. */
  let placed = '';

  async function placePage() {
    const place = frame();
    if (!place) {
      deviceBox = null;
      await takePageAway();
      return;
    }
    const origin = stage!.getBoundingClientRect();
    // Keep the device frame responsive even before a URL has been opened. There is no native page
    // to place yet, but switching Tablet/Mobile should still change the empty Preview layout.
    deviceBox = DEVICES[browser.device].w ? { x: place.rect.x - origin.x, y: place.rect.y - origin.y, w: place.rect.w, h: place.rect.h, zoom: place.zoom } : null;
    // While the frame glides the native page stays away: resizing it every frame tears the picture and re-lays out the site.
    if (morphing) return;
    // An empty tab (a new one, nothing typed yet) shows no page: the previous tab's page must go away.
    if (inTauri() && !started && !layout.dragging) {
      await takePageAway();
      return;
    }
    if (!inTauri() || !started || layout.dragging) return;
    try {
      // Only the tab in front is on screen: showing it hides the others (once it is up, so no frame is left empty).
      const { x, y, w, h } = place.rect;
      const key = `${tab.id}|${place.zoom}|${tab.opened}|${Math.round(x * 100)},${Math.round(y * 100)},${Math.round(w * 100)},${Math.round(h * 100)}`;
      if (key === placed) return;
      const [shownTab, shownZoom] = placed.split('|');
      // The page already on screen only moves: a plain move skips showing it and hiding the others again.
      if (shownTab === tab.id && tab.opened) await moveBrowser(place.rect);
      else await showBrowser(place.rect);
      // The app is see-through where the page is, so whatever it opens over the page can sit above it.
      cutHole(stage!, place.rect);
      if (tab.opened && shownZoom !== String(place.zoom)) await zoomBrowser(place.zoom);
      placed = key;
    } catch (error) {
      // The page went away under us (closed, or being recreated for another device): open it again next time.
      if (String(error).includes('the browser is not open')) {
        tab.opened = false;
        again = true;
        return;
      }
      browser.error = String(error);
    }
  }

  function normalize(input: string): string {
    const text = input.trim();
    if (!text) return '';
    if (/^[a-z][a-z0-9+.-]*:\/\//i.test(text)) return text;
    if (/^(localhost|127\.|\[::1\])/.test(text)) return `http://${text}`;
    // Words with spaces are a search, not an address.
    if (/\s/.test(text) || !/\./.test(text)) return `https://www.google.com/search?q=${encodeURIComponent(text)}`;
    return `https://${text}`;
  }

  async function go() {
    const url = normalize(address);
    if (!url) return;
    browser.error = '';
    editing = false;
    dirty = false;
    address = url;
    // The address is set before the page opens, so the first open never uses a stale one.
    if (tab.opened) await goTo(url);
    else {
      tab.url = url;
      await tick();
      await sync();
    }
  }

  // ---- address bar: security, and suggestions while typing ----
  const SEARCH = 'https://www.google.com/search?q=';
  const searchUrl = (text: string) => `${SEARCH}${encodeURIComponent(text)}`;
  type Suggestion = { kind: 'go' | 'search' | 'history' | 'google'; text: string; url: string; title?: string };
  /** Typed in, not just focused: suggestions only follow what the user writes. */
  let dirty = $state(false);
  let closed = $state(false);
  let picked = $state(-1);
  let remote = $state<string[]>([]);

  const suggestions = $derived.by<Suggestion[]>(() => {
    const text = address.trim();
    if (!editing || !dirty || !text) return [];
    const target = normalize(text);
    const isSearch = target.startsWith(SEARCH);
    const list: Suggestion[] = [isSearch ? { kind: 'search', text, url: target } : { kind: 'go', text, url: target }];
    for (const visit of matchHistory(text)) if (visit.url !== target) list.push({ kind: 'history', text: visit.url, url: visit.url, title: visit.title });
    if (!isSearch) list.push({ kind: 'search', text, url: searchUrl(text) });
    for (const phrase of remote) if (phrase.toLowerCase() !== text.toLowerCase()) list.push({ kind: 'google', text: phrase, url: searchUrl(phrase) });
    return list.slice(0, 10);
  });
  const suggestOpen = $derived(!closed && suggestions.length > 0);

  // Google's own suggestions for a search, a moment after typing stops (fetched by the app, not the page).
  $effect(() => {
    const text = address.trim();
    const wanted = editing && dirty && text && normalize(text).startsWith(SEARCH);
    if (!wanted) {
      remote = [];
      return;
    }
    let current = true;
    const timer = setTimeout(() => void searchSuggestions(text).then((found) => current && (remote = found)), 180);
    return () => {
      current = false;
      clearTimeout(timer);
    };
  });

  function choose(item: Suggestion) {
    address = item.url;
    void go();
  }

  function onAddressKey(event: KeyboardEvent) {
    const count = suggestions.length;
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      if (!count) return;
      event.preventDefault();
      closed = false;
      picked = event.key === 'ArrowDown' ? (picked + 1 >= count ? -1 : picked + 1) : picked < 0 ? count - 1 : picked - 1;
    } else if (event.key === 'Enter' && suggestOpen && picked >= 0) {
      event.preventDefault();
      choose(suggestions[picked]);
    } else if (event.key === 'Delete' && event.shiftKey && suggestOpen && suggestions[picked]?.kind === 'history') {
      event.preventDefault();
      forgetVisit(suggestions[picked].url);
      picked = -1;
    } else if (event.key === 'Escape') {
      event.preventDefault();
      if (suggestOpen) closed = true;
      else {
        address = tab.url;
        dirty = false;
        addressInput?.blur();
      }
    }
  }

  const security = $derived.by(() => {
    try {
      const url = new URL(tab.url);
      if (url.protocol !== 'http:' && url.protocol !== 'https:') return null;
      const local = /^(localhost|127\.0\.0\.1|\[::1\])$/.test(url.hostname) || url.hostname.endsWith('.localhost');
      const secure = url.protocol === 'https:';
      return {
        kind: secure ? ('secure' as const) : local ? ('local' as const) : ('insecure' as const),
        protocol: url.protocol.slice(0, -1).toUpperCase(),
        host: url.hostname,
        port: url.port || (secure ? '443' : '80'),
      };
    } catch {
      return null;
    }
  });
  let lockButton = $state<HTMLElement | null>(null);
  let lockOpen = $state(false);

  // ---- tabs: no scrollbar; when they do not fit an arrow lists them all, and the front one stays in view ----
  let tabStrip: HTMLDivElement | undefined = $state();
  let tabStripWidth = $state(0);
  let tabsOverflow = $state(false);
  // Tabs cut off on the left: the strip is scrolled past its first tab.
  let tabScrolled = $state(false);
  $effect(() => {
    void browser.tabs.length;
    void tabStripWidth;
    void tick().then(() => {
      tabsOverflow = browser.tabs.length > 1 && !!tabStrip && tabStrip.scrollWidth > tabStrip.clientWidth + 1;
    });
  });
  $effect(() => {
    const id = browser.activeId;
    void browser.tabs.length;
    void tabStripWidth;
    void tick().then(() => {
      revealTab(tabStrip, id);
      tabScrolled = (tabStrip?.scrollLeft ?? 0) > 1;
    });
  });
  const tabListItems = $derived<PickerItem<string>[]>(browser.tabs.map((item) => ({ id: item.id, label: tabTitle(item), hint: item.url ? item.url.replace(/^https?:\/\//, '') : undefined, icon: 'lucideGlobe' as ZIconName, selected: item.id === browser.activeId, data: item.id })));
  function addTab() {
    if (tabsFull()) {
      toast.info(`Tab limit reached (${MAX_TABS}). Close a tab to open another.`);
      return;
    }
    newTab();
  }

  // The page is a native layer above the app: anything the app opens over it would be hidden behind it. While
  // something is over the page, the page goes below the app, which is see-through there (page-layer.ts).
  function checkLayer() {
    if (!inTauri() || !stage) return;
    const over = tab.opened && overPage(stage, stage.getBoundingClientRect());
    setBelow(over);
    blurPage(over ? modalBlur() : '');
  }
  let observer: MutationObserver | undefined;
  let resizer: ResizeObserver | undefined;

  onMount(() => {
    void listenToPage();
    if (stage) {
      resizer = new ResizeObserver(() => void sync());
      resizer.observe(stage);
    }
    // Opened at once, not on the next frame: the page goes below as the overlay appears.
    observer = new MutationObserver(checkLayer);
    observer.observe(document.body, { childList: true, subtree: true });
    void sync();
    window.addEventListener('resize', onResize);
    // The stage can move without resizing (a floating window moved, a panel slid): follow it.
    let last = '';
    let frameId = 0;
    const watch = () => {
      // A floating tool window dragged onto the page, or off it, changes no DOM: look every frame.
      checkLayer();
      const box = stage?.getBoundingClientRect();
      const key = box ? `${Math.round(box.x)},${Math.round(box.y)},${Math.round(box.width)},${Math.round(box.height)}` : '';
      // While a drag holds the page back nothing is placed, so the change is kept for when it ends.
      if (key !== last && !layout.dragging) {
        last = key;
        void sync();
      }
      frameId = requestAnimationFrame(watch);
    };
    frameId = requestAnimationFrame(watch);
    // What runs in the background is read again every few seconds while the panel is on screen.
    void refreshServers();
    const poll = setInterval(() => void refreshServers(), 3000);
    return () => {
      cancelAnimationFrame(frameId);
      clearInterval(poll);
    };
  });
  const onResize = () => void sync();

  onDestroy(() => {
    resizer?.disconnect();
    observer?.disconnect();
    window.removeEventListener('resize', onResize);
    // Another tool took the panel: the page stays alive off-screen, so the agent can still look at it and act on it.
    if (inTauri()) {
      cutHole(null, null);
      setBelow(false);
      blurPage('');
      void parkBrowser();
    }
  });

  // Opened by a link or the developer hook.
  $effect(() => {
    const url = browser.autostart;
    if (!url) return;
    browser.autostart = '';
    untrack(() => {
      const front = activeTab();
      if (front.opened && front.url !== url) void goTo(url);
      else {
        front.url = url;
        void tick().then(sync);
      }
    });
  });

  // Another device or a turn: the page steps aside, the frame glides to its new size, then the page comes back in it.
  let lastShape = `${browser.device}|${browser.landscape}`;
  $effect(() => {
    const shape = `${browser.device}|${browser.landscape}`;
    if (shape === lastShape) return;
    lastShape = shape;
    untrack(() => {
      morphing = true;
      clearTimeout(morphTimer);
      void takePageAway();
      morphTimer = setTimeout(() => {
        morphing = false;
        void sync();
      }, MORPH_MS + 40);
    });
  });
  onDestroy(() => clearTimeout(morphTimer));

  // A new device means a new user agent, which only a new page can have: the pages are created again at the same address.
  let lastDevice = browser.device;
  $effect(() => {
    const device = browser.device;
    if (device === lastDevice) return;
    lastDevice = device;
    untrack(() => void recreateBrowser().then(sync));
  });

  // Switching tabs, opening one, rotating, a covering menu: show the right page in the right place.
  $effect(() => {
    void browser.activeId;
    void browser.landscape;
    void browser.device;
    void started;
    // The mark-up bar opens in this very update, so the page moves before the next frame is drawn.
    void browser.annotating;
    // A floating window being carried or resized: the page steps aside and comes back where it lands.
    if (layout.dragging) untrack(() => void takePageAway());
    void layout.dragging;
    untrack(() => void sync());
  });

  // ---- pick, comment, annotate, send ----
  const clip = (text: string, max: number) => (text.length > max ? `${text.slice(0, max)}…` : text);

  // An error shows for a few seconds, then goes by itself: it is never left on a page it has nothing to do with.
  $effect(() => {
    const message = browser.error;
    if (!message) return;
    const timer = setTimeout(() => {
      if (browser.error === message) browser.error = '';
    }, 6000);
    return () => clearTimeout(timer);
  });

  /** The inspector: point at an element in the live page and click it. It goes to the message box as an element, with no picture. */
  async function pick() {
    if (browser.picking) {
      void cancelPick();
      return;
    }
    browser.error = '';
    try {
      if (browser.annotating) await stopAnnotating();
      const picked = await pickElement();
      if (!picked) return;
      const { element, url, title } = picked;
      const id = workspace.activeId;
      // A picture of the element alone, so the agent sees it next to its markup.
      const picture = await captureElement(browser.activeId, element.box);
      draftElements[id] = [...(draftElements[id] ?? []), { id: crypto.randomUUID(), picture, tag: element.tag, text: clip(element.text, 80), selector: element.selector, html: clip(element.html, 2500), around: element.around, box: element.box, style: element.style, url, title }];
      toast.success('Element added to the message');
    } catch (error) {
      browser.error = String(error);
    }
  }

  // ---- mark up the live page ----
  // The arrow button starts it. The tools draw on the page itself (an element with a comment, an arrow, a box, a
  // pinned note); once there are marks, Add to chat takes a picture of the page as it is, marks included.
  const MARK_TOOLS: { tool: AnnotateTool; label: string; hint: string; icon: ZIconName }[] = [
    { tool: 'pick', label: 'Comment on an element', hint: 'Click an element to comment on it', icon: 'lucideMousePointerClick' as ZIconName },
    { tool: 'arrow', label: 'Draw an arrow', hint: 'Drag to draw an arrow', icon: 'lucideMoveUpRight' as ZIconName },
    { tool: 'box', label: 'Draw a box', hint: 'Drag to draw a box', icon: 'lucideSquare' as ZIconName },
    { tool: 'note', label: 'Add a note', hint: 'Click to pin a note', icon: 'lucideMessageSquarePlus' as ZIconName },
  ];

  async function toggleMarkup() {
    browser.error = '';
    try {
      if (browser.annotating) await stopAnnotating();
      else {
        // Inspecting and marking up are two ways to point at the page: starting one ends the other.
        if (browser.picking) await cancelPick();
        await annotate('pick');
      }
    } catch (error) {
      browser.error = String(error);
    }
  }

  async function useTool(tool: AnnotateTool) {
    try {
      await annotate(tool);
    } catch (error) {
      browser.error = String(error);
    }
  }

  async function addMarksToChat() {
    busy = 'sending';
    browser.error = '';
    try {
      const notes = await annotationNotes();
      // The page as it is now, marks included, straight from the engine: one quick capture.
      // No picture when the page cannot be captured: the marks still go to the chat, as text and elements.
      const picture = await capturePageIfPossible();
      // What was written on an element goes with that element, as its note. The rest, drawn on the picture, are
      // numbered lines in the message.
      const words = notes
        .filter((note) => note.text && !note.element)
        .map((note) => `${note.n}. ${note.text}`)
        .join('\n');
      const id = workspace.activeId;
      if (words) drafts[id] = [drafts[id], words].filter(Boolean).join('\n');
      // An element that was marked travels as an element too (its markup and look), next to the picture.
      const marked = notes.flatMap((note) => (note.element ? [{ element: note.element, note: note.text ? `${note.n}. ${note.text}` : `${note.n}.` }] : []));
      if (marked.length) {
        draftElements[id] = [
          ...(draftElements[id] ?? []),
          ...marked.map(({ element, note }, index) => ({ id: crypto.randomUUID(), tag: element.tag, text: clip(element.text, 80), selector: element.selector, html: clip(element.html, 2500), around: element.around, box: element.box, style: element.style, url: tab.url, title: tab.title, note, ...(index === 0 && picture ? { picture } : {}) })),
        ];
      }
      // Keep the full marked-page capture with its element chip, like an Inspect capture. Freehand-only marks have
      // no element chip, so their screenshot stays in the regular image attachments.
      if (picture && !marked.length) draftImages[id] = [...(draftImages[id] ?? []), { id: crypto.randomUUID(), name: 'page-notes.png', mime: 'image/png', src: picture }];
      await stopAnnotating();
      toast.success('Added to the message');
    } catch (error) {
      browser.error = String(error);
    } finally {
      busy = '';
    }
  }

  let screenshotOpen = $state(false);
  let screenshotImage = $state('');

  async function openScreenshotEditor(regionOnly: boolean, fullPage = false) {
    busy = 'capture';
    browser.error = '';
    try {
      let selected: Awaited<ReturnType<typeof selectCaptureRegion>> = null;
      if (regionOnly) {
        selected = await selectCaptureRegion();
        if (!selected) return;
      } else {
        if (browser.annotating) await stopAnnotating();
        if (browser.picking) await cancelPick();
      }
      const captured = await capturePageIfPossible(fullPage ? 1 : 2, fullPage);
      if (!captured) throw new Error('Could not capture the page. Try again after it finishes loading.');
      screenshotImage = selected ? await cropScreenshot(captured, selected.box, selected.viewport) : captured;
      screenshotOpen = true;
    } catch (error) {
      browser.error = String(error);
    } finally {
      busy = '';
    }
  }

  async function cropScreenshot(dataUrl: string, box: Rect, viewport: { w: number; h: number }): Promise<string> {
    const image = new Image();
    image.src = dataUrl;
    await image.decode();
    const scale = image.naturalWidth / viewport.w;
    const x = Math.max(0, Math.round(box.x * scale));
    const y = Math.max(0, Math.round(box.y * scale));
    const width = Math.min(image.naturalWidth - x, Math.round(box.w * scale));
    const height = Math.min(image.naturalHeight - y, Math.round(box.h * scale));
    if (width <= 0 || height <= 0) throw new Error('The selected region is outside the captured page.');
    const canvas = document.createElement('canvas');
    canvas.width = width;
    canvas.height = height;
    const context = canvas.getContext('2d');
    if (!context) throw new Error('Could not prepare the selected screenshot.');
    context.drawImage(image, x, y, width, height, 0, 0, width, height);
    return canvas.toDataURL('image/png');
  }

  const DEVICE_BUTTONS = (['desktop', 'tablet', 'mobile'] as Device[]).map((value) => ({
    value,
    label: DEVICES[value].w ? `${DEVICES[value].label} · ${DEVICES[value].w}×${DEVICES[value].h}` : DEVICES[value].label,
    icon: (value === 'desktop' ? 'lucideMonitor' : value === 'tablet' ? 'lucideTablet' : 'lucideSmartphone') as ZIconName,
  }));
  // ---- the "more" menu ----
  // At phone-width, controls move into More tools instead of disappearing.
  let barWidth = $state(1000);
  const compact = $derived(barWidth < 420);
  let toolsWidth = $state(0);
  const alive = $derived({ length: servers.list.filter((item) => item.running).length + terminalRuns.list.length });
  let processOpen = $state(false);
  const pageTools = $derived<(ZDropdownMenuItem & { separatorBefore?: boolean })[]>([
    { label: browser.picking ? 'Stop inspecting' : 'Inspect an element', icon: 'lucideMousePointerClick', iconSize: 12, checked: browser.picking, disabled: !started, onClick: () => void pick() },
    { label: browser.annotating ? 'Stop marking up' : 'Mark up the page', icon: 'lucideMousePointer2', iconSize: 12, checked: browser.annotating, disabled: !started, onClick: () => void toggleMarkup() },
    { label: 'Create guide', icon: 'lucideClipboardList', iconSize: 12, disabled: !started, onClick: () => (guide.open = true) },
    { label: errors > 0 ? `Developer tools (${errors} errors)` : 'Developer tools', icon: 'lucideSquareTerminal', iconSize: 12, checked: devtoolsOpen(), onClick: () => setDevtools(!devtoolsOpen()) },
    { label: alive.length > 0 ? `Running in the background (${alive.length})` : 'Running in the background', icon: 'lucideServer', iconSize: 12, checked: processOpen, onClick: () => (processOpen = !processOpen) },
    ...(browser.annotating
      ? [
          ...MARK_TOOLS.map((item, index) => ({ label: item.label, icon: item.icon, iconSize: 12, checked: browser.annotateTool === item.tool, separatorBefore: index === 0, onClick: () => void useTool(item.tool) })),
          { label: 'Undo', icon: 'lucideUndo2' as ZIconName, iconSize: 12, separatorBefore: true, disabled: !browser.annotateUndo, onClick: () => void markHistory('undo') },
          { label: 'Redo', icon: 'lucideRedo2' as ZIconName, iconSize: 12, disabled: !browser.annotateRedo, onClick: () => void markHistory('redo') },
          { label: browser.annotateSelected ? 'Delete selected note' : 'Delete all notes', icon: 'lucideTrash2' as ZIconName, iconSize: 12, class: 'text-destructive!', disabled: browser.annotateCount === 0, onClick: () => void markHistory('remove') },
          ...(browser.annotateCount > 0 ? [{ label: 'Add notes to chat', icon: 'lucidePlus' as ZIconName, iconSize: 12, disabled: busy !== '', onClick: () => void addMarksToChat() }] : []),
        ]
      : []),
  ]);
  // Each toolbar button is 28px with a 2px gap; reserve padding and the always-visible menu trigger.
  const visibleToolCount = $derived.by(() => {
    let used = 0;
    let count = 0;
    for (const item of pageTools) {
      const next = 30 + (item.separatorBefore ? 8 : 0);
      if (used + next > Math.max(0, toolsWidth - 76)) break;
      used += next;
      count += 1;
    }
    return count;
  });
  const screenshotItems = $derived<ZDropdownMenuItem[]>([
    { label: 'Capture full viewport', icon: 'lucideScan' as ZIconName, iconSize: 12, disabled: !started || busy !== '', onClick: () => void openScreenshotEditor(false) },
    { label: 'Capture full scrollable page', icon: 'lucidePanelsTopLeft' as ZIconName, iconSize: 12, disabled: !started || busy !== '', onClick: () => void openScreenshotEditor(false, true) },
    { label: 'Select capture region', icon: 'lucideCrop' as ZIconName, iconSize: 12, disabled: !started || busy !== '', onClick: () => void openScreenshotEditor(true) },
  ]);
  const menuItems = $derived<ZDropdownMenuItem[]>([
    ...(compact
      ? [
          ...DEVICE_BUTTONS.map((item) => ({ label: item.label, icon: item.icon, iconSize: 12, class: `text-xs! py-1 ${browser.device === item.value ? 'bg-primary/10 text-primary' : ''}`, checked: browser.device === item.value, onClick: () => (browser.device = item.value) })),
          { label: 'Rotate', icon: 'lucideRotateCw' as ZIconName, iconSize: 12, class: `text-xs! py-1 ${browser.landscape && browser.device !== 'desktop' ? 'bg-primary/10 text-primary' : ''}`, disabled: browser.device === 'desktop', checked: browser.landscape && browser.device !== 'desktop', onClick: () => (browser.landscape = !browser.landscape) },
          { label: browser.picking ? 'Stop inspecting' : 'Inspect an element', icon: 'lucideMousePointerClick' as ZIconName, iconSize: 12, class: `text-xs! py-1 ${browser.picking ? 'bg-primary/10 text-primary' : ''}`, checked: browser.picking, disabled: !started, onClick: () => void pick() },
          { label: browser.annotating ? 'Stop marking up' : 'Mark up the page', icon: 'lucideMousePointer2' as ZIconName, iconSize: 12, class: `text-xs! py-1 ${browser.annotating ? 'bg-primary/10 text-primary' : ''}`, checked: browser.annotating, disabled: !started, onClick: () => void toggleMarkup() },
          { label: 'Create guide', icon: 'lucideClipboardList' as ZIconName, iconSize: 12, class: 'text-xs! py-1', disabled: !started, onClick: () => (guide.open = true) },
        ]
      : []),
    ...(compact && browser.annotating
      ? [
          ...MARK_TOOLS.map((item) => ({ label: item.label, icon: item.icon, iconSize: 12, class: `text-xs! py-1 ${browser.annotateTool === item.tool ? 'bg-primary/10 text-primary' : ''}`, checked: browser.annotateTool === item.tool, onClick: () => void useTool(item.tool) })),
          { label: 'Undo', icon: 'lucideUndo2' as ZIconName, iconSize: 12, class: 'text-xs! py-1', disabled: !browser.annotateUndo, onClick: () => void markHistory('undo') },
          { label: 'Redo', icon: 'lucideRedo2' as ZIconName, iconSize: 12, class: 'text-xs! py-1', disabled: !browser.annotateRedo, onClick: () => void markHistory('redo') },
          { label: browser.annotateSelected ? 'Delete selected note' : 'Delete all notes', icon: 'lucideTrash2' as ZIconName, iconSize: 12, class: 'text-xs! py-1 text-destructive!', disabled: browser.annotateCount === 0, onClick: () => void markHistory('remove') },
          ...(browser.annotateCount > 0 ? [{ label: 'Add notes to chat', icon: 'lucidePlus' as ZIconName, iconSize: 12, class: 'text-xs! py-1', disabled: busy !== '', onClick: () => void addMarksToChat() }] : []),
        ]
      : []),
    { label: /windows/i.test(navigator.userAgent) ? 'Add unpacked extension…' : 'Extensions require Windows', icon: 'lucidePuzzle' as ZIconName, iconSize: 12, class: 'text-xs! py-1', divide: 'before', disabled: !inTauri() || !tab.opened || !/windows/i.test(navigator.userAgent), onClick: () => void addExtension() },
    { label: 'Open in external browser', icon: 'lucideExternalLink' as ZIconName, iconSize: 12, class: 'text-xs! py-1', divide: compact ? 'before' : undefined, disabled: !tab.url, onClick: () => void openExternal(tab.url) },
    { label: 'Copy address', icon: 'lucideCopy' as ZIconName, iconSize: 12, class: 'text-xs! py-1', disabled: !tab.url, onClick: () => void navigator.clipboard.writeText(tab.url).then(() => toast.success('Address copied')) },
    { label: 'Close tab', icon: 'lucideX' as ZIconName, iconSize: 12, class: 'text-xs! py-1', divide: 'before', onClick: () => void closeTab(tab.id) },
  ]);
  const moreItems = $derived([
    ...pageTools.slice(visibleToolCount).map((item) => ({ ...item, class: `text-xs! py-1 ${item.class ?? ''} ${item.checked ? 'bg-primary/10 text-primary' : ''}` })),
    ...menuItems,
  ]);

  async function addExtension() {
    try {
      const name = await addBrowserExtension(tab.id);
      if (name) toast.success(`Added ${name} to Browser Preview`);
    } catch (error) {
      toast.error(String(error));
    }
  }

  // What runs in the background (servers started from the Preview or by an agent): open it, or stop it.
  // Commands running in the Terminal panel's tabs are listed too (their address, when they printed one).
  // The rows whose live output is open.
  let shown = $state<Record<string, boolean>>({});
  const rowCount = $derived(servers.list.length + terminalRuns.list.length);
  const terminalAge = (run: { startedAt: number }) => Math.max(0, Math.floor((clock - run.startedAt) / 1000));
  function terminalState(run: { url: string | null; interruptedAt: number | null }) {
    if (run.interruptedAt !== null && clock - run.interruptedAt > 2500) return { label: 'Still running · press stop again to close the terminal', tone: 'text-amber-500' };
    return run.url ? { label: `Running in terminal · ${run.url.replace(/^https?:\/\//, '')}`, tone: 'text-emerald-500' } : { label: 'Running in terminal', tone: 'text-emerald-500' };
  }
  const terminalName = (id: string) => terminal.tabs.find((tab) => tab.id === id)?.name ?? 'terminal';
  function stopAll() {
    void stopServers(null);
    for (const run of [...terminalRuns.list]) stopTerminalRun(run.id);
  }
  const portOf = (item: { url: string | null }) => item.url?.match(/:(\d+)/)?.[1];
  const runningFor = (age: number) => (age < 60 ? `${age}s` : age < 3600 ? `${Math.floor(age / 60)}m ${age % 60}s` : `${Math.floor(age / 3600)}h ${Math.floor((age % 3600) / 60)}m ${age % 60}s`);
  const processName = (item: { command: string; url: string | null }) => `${item.command.length > 34 ? `${item.command.slice(0, 34)}…` : item.command}${portOf(item) ? ` · :${portOf(item)}` : ''}`;
  /** What a background server is doing, for its row: not answering yet, up on an address, or gone. */
  function serverState(item: { key: number; age: number; url: string | null; running: boolean }) {
    const age = ageOf(item);
    if (!item.running && !item.url) return { label: `Stopped: no answer on port ${item.key}`, tone: 'text-destructive', hint: `It never answered on port ${item.key} and was stopped. The command may have failed or uses another port: run it in a terminal to see its output.` };
    if (!item.running) return { label: 'Stopped: the command ended', tone: 'text-destructive', hint: 'This server has stopped. Remove it from the list, or run it again.' };
    if (item.url) return { label: `Running · ${item.url.replace(/^https?:\/\//, '')}`, tone: 'text-emerald-500', hint: '' };
    if (age < 90) return { label: `Starting · waiting for port ${item.key} to answer`, tone: 'text-amber-500', hint: `Still starting: waiting for port ${item.key} to answer.` };
    return { label: `Not answering on port ${item.key}`, tone: 'text-destructive', hint: `Nothing answers on port ${item.key}. The command may have failed or uses another port: run it in a terminal to see its output.` };
  }
  /** The project a server runs in: the sidebar project that holds its folder, else the folder's own name. */
  function projectNameOf(cwd: string) {
    const flat = (path: string) => path.replace(/\\/g, '/').replace(/\/+$/, '').toLowerCase();
    const here = flat(cwd);
    const found = workspace.projects.find((project) => project.path && (here === flat(project.path) || here.startsWith(`${flat(project.path)}/`)));
    return found?.name ?? (cwd.replace(/[\\/]+$/, '').split(/[\\/]/).pop() || cwd);
  }
  // The age of a running server counts up every second while the list is open. The list is read every 3 s and its ages
  // are whole seconds, so using them as they come makes the number jump back and forth: each server's start time is kept
  // instead, and only moved when a reading is clearly different (a restart, a long stall).
  let clock = $state(Date.now());
  const starts = new Map<number, number>();
  $effect(() => {
    const read = servers.at;
    for (const item of servers.list) {
      const start = read - item.age * 1000;
      const known = starts.get(item.key);
      if (known === undefined || Math.abs(start - known) > 2500) starts.set(item.key, start);
    }
  });
  $effect(() => {
    if (!processOpen) return;
    clock = Date.now();
    const tick = setInterval(() => (clock = Date.now()), 1000);
    return () => clearInterval(tick);
  });
  const ageOf = (item: { key: number; age: number; running: boolean }) => (item.running ? Math.max(0, Math.floor((clock - (starts.get(item.key) ?? clock - item.age * 1000)) / 1000)) : item.age);

  const icon = 'grid size-7 shrink-0 cursor-pointer place-items-center rounded-md text-muted-foreground outline-none hover:bg-primary/10 hover:text-primary disabled:pointer-events-none disabled:opacity-40';
  const tabTitle = (item: { title: string; url: string }) => item.title || (item.url ? item.url.replace(/^https?:\/\//, '') : 'New tab');
</script>

{#snippet processTitle()}
  <span class="flex w-full min-w-0 items-center gap-2">
    <span class="truncate">Running in the background</span>
    <span class="rounded-full bg-muted px-1.5 text-[11px] text-muted-foreground tabular-nums">{rowCount}</span>
    <span class="flex-1"></span>
    <button type="button" class="grid size-7 cursor-pointer place-items-center rounded-md border border-border bg-muted/60 outline-none hover:bg-accent" aria-label="Close" onclick={() => (processOpen = false)}><ZIcon zType={'lucideX' as ZIconName} class="size-4" /></button>
  </span>
{/snippet}

{#snippet processFooter()}
  {#if rowCount > 1}
    <button type="button" class="flex w-full cursor-pointer items-center justify-center gap-2 rounded-md bg-destructive/10 px-2 py-2 text-xs text-destructive outline-none hover:bg-destructive/20" onclick={() => { stopAll(); processOpen = false; }}>
      <svg viewBox="0 0 24 24" class="size-3 shrink-0" aria-hidden="true"><rect x="4" y="4" width="16" height="16" rx="4" fill="currentColor" /></svg> Stop all
    </button>
  {/if}
{/snippet}

<svelte:window onkeydown={onWindowKey} />

<div class="flex h-full w-full min-h-0 flex-1 flex-col" aria-label="Browser">
  {#if !inTauri()}
    <div class="grid flex-1 place-items-center p-6 text-center text-xs text-muted-foreground">The browser runs in the AgentDeck desktop app.</div>
  {:else}
    <!-- Tabs -->
    <div class="flex h-9 shrink-0 items-center gap-1 border-b border-border pr-1 pl-1" role="tablist" aria-label="Browser tabs">
      <div class="relative flex min-w-0 shrink items-center">
      <div bind:this={tabStrip} bind:clientWidth={tabStripWidth} onscroll={() => (tabScrolled = (tabStrip?.scrollLeft ?? 0) > 1)} class="flex min-w-0 flex-1 items-center gap-1 overflow-hidden">
        {#each browser.tabs as item (item.id)}
          <div
            data-tab-id={item.id}
            role="tab"
            tabindex="0"
            aria-selected={item.id === browser.activeId}
            class="group/tab flex h-7 min-w-24 max-w-44 shrink-0 cursor-pointer items-center gap-1.5 rounded-md pr-1 pl-2 text-xs outline-none {item.id === browser.activeId ? 'bg-primary/15 text-primary' : 'text-muted-foreground hover:bg-primary/10 hover:text-primary'}"
            onclick={() => selectTab(item.id)}
            onkeydown={(event) => event.key === 'Enter' && selectTab(item.id)}
            title={tabTitle(item)}
          >
            <span class="grid size-3.5 shrink-0 place-items-center">
              {#if item.loading}
                <ZLoading zLoading zSpinner="spinner" zSize="2xs" zColor="current" />
              {:else if item.favicon}
                <img src={item.favicon} alt="" class="size-3.5 rounded-sm" onerror={(event) => ((event.currentTarget as HTMLElement).style.display = 'none')} />
              {:else}
                <ZIcon zType="lucideGlobe" class="size-3.5" />
              {/if}
            </span>
            <span class="min-w-0 flex-1 truncate">{tabTitle(item)}</span>
            <button type="button" class="grid size-4.5 shrink-0 cursor-pointer place-items-center rounded-full outline-none focus-visible:ring-0 hover:bg-primary/20" aria-label="Close tab" onclick={(event) => { event.stopPropagation(); void closeTab(item.id); }}>
              <ZIcon zType="lucideX" zSize="12" class="size-3" />
            </button>
          </div>
        {/each}
      </div>
      {#if tabsOverflow}
        <!-- Fade into the background, as the chat's tab bar does: a tab that is cut off does not end in a hard edge. -->
        <div class="pointer-events-none absolute inset-y-0 right-0 z-10 w-10 bg-gradient-to-r from-transparent to-background" aria-hidden="true"></div>
      {/if}
      {#if tabScrolled}
        <div class="pointer-events-none absolute inset-y-0 left-0 z-10 w-10 bg-gradient-to-r from-background to-transparent" aria-hidden="true"></div>
      {/if}
      </div>
      {#if tabsOverflow}
        <div class="flex shrink-0 items-center bg-background">
          <TabListButton items={tabListItems} onpick={selectTab} label="Open tabs" />
        </div>
      {/if}
      <button type="button" class="grid size-7 shrink-0 cursor-pointer place-items-center rounded-md text-muted-foreground outline-none hover:bg-primary/10 hover:text-primary {tabsFull() ? 'opacity-40' : ''}" aria-label="New tab" aria-disabled={tabsFull()} {@attach zTooltip({ content: tabsFull() ? `Tab limit reached (${MAX_TABS})` : 'New tab', position: 'top' })} onclick={addTab}>
        <ZIcon zType="lucidePlus" class="size-4" />
      </button>
    </div>

    <!-- Address bar: navigation and address stay together; page tools live in the row below. -->
    <div bind:clientWidth={barWidth} class="flex shrink-0 items-center gap-1 border-b border-border px-1 py-2">
      <div class="flex shrink-0 items-center gap-0.5">
        <button type="button" class={icon} aria-label="Back" {@attach zTooltip({ content: 'Back', position: 'top' })} onclick={() => void back()}><ZIcon zType={'lucideArrowLeft' as ZIconName} class="size-4" /></button>
        <button type="button" class={icon} aria-label="Forward" {@attach zTooltip({ content: 'Forward', position: 'top' })} onclick={() => void forward()}><ZIcon zType={'lucideArrowRight' as ZIconName} class="size-4" /></button>
        <button type="button" class={icon} aria-label={tab.loading ? 'Stop' : 'Reload'} {@attach zTooltip({ content: tab.loading ? 'Stop loading' : 'Reload', position: 'top' })} onclick={() => void (tab.loading ? stop() : reload())}>
          {#if tab.loading}<ZLoading zLoading zSpinner="spinner" zSize="xs" zColor="primary" />{:else}<ZIcon zType={'lucideRotateCw' as ZIconName} class="size-4" />{/if}
        </button>
      </div>
      <form class="relative min-w-0 flex-1" onsubmit={(event) => { event.preventDefault(); void go(); }}>
        <button
          bind:this={lockButton}
          type="button"
          class="absolute top-0.5 left-0.5 z-10 grid size-6 cursor-pointer place-items-center rounded outline-none hover:bg-primary/10 {security?.kind === 'secure' ? 'text-emerald-600 dark:text-emerald-400' : security?.kind === 'insecure' ? 'text-amber-600 dark:text-amber-400' : 'text-muted-foreground'}"
          aria-label={security ? 'Site information' : 'Search'}
          aria-expanded={lockOpen}
          disabled={!security}
          {@attach zTooltip({ content: security ? (security.kind === 'secure' ? 'Connection is secure' : security.kind === 'local' ? 'Local address' : 'Connection is not secure') : 'Search', position: 'bottom' })}
          onclick={() => (lockOpen = !lockOpen)}
        >
          {#if security?.kind === 'secure'}
            <svg viewBox="0 0 24 24" class="size-3.5" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect width="18" height="11" x="3" y="11" rx="2" /><path d="M7 11V7a5 5 0 0 1 10 0v4" /></svg>
          {:else if security?.kind === 'insecure'}
            <ZIcon zType={'lucideTriangleAlert' as ZIconName} class="size-3.5" />
          {:else if security}
            <svg viewBox="0 0 24 24" class="size-3.5" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect width="18" height="11" x="3" y="11" rx="2" /><path d="M7 11V7a5 5 0 0 1 9.9-1" /></svg>
          {:else}
            <ZIcon zType={'lucideSearch' as ZIconName} class="size-3.5" />
          {/if}
        </button>
        <input
          bind:this={addressInput}
          bind:value={address}
          class="h-7 w-full border bg-muted/40 pr-3 pl-8 text-xs outline-none transition-colors placeholder:text-muted-foreground focus:border-primary focus:bg-background {suggestOpen ? 'rounded-t-md rounded-b-none border-primary border-b-border bg-background' : 'rounded-md border-border'}"
          placeholder="Search or enter an address"
          aria-label="Address"
          role="combobox"
          aria-expanded={suggestOpen}
          aria-controls="address-suggestions"
          aria-autocomplete="list"
          spellcheck="false"
          autocomplete="off"
          oninput={() => { dirty = true; closed = false; picked = -1; }}
          onkeydown={onAddressKey}
          onfocus={(event) => { editing = true; dirty = false; closed = false; picked = -1; selectAddress(event.currentTarget as HTMLInputElement); }}
          onblur={() => { editing = false; dirty = false; }}
          onmousedown={() => (focusedByClick = document.activeElement !== addressInput)}
          onmouseup={(event) => {
            if (!focusedByClick) return;
            focusedByClick = false;
            event.preventDefault();
          }}
        />
        {#if suggestOpen}
          <!-- Drawn by the app over the page (data-page-over puts the page below the app while this is open). -->
          <div id="address-suggestions" data-page-over role="listbox" tabindex="-1" class="absolute top-full right-0 left-0 z-50 max-h-72 overflow-x-hidden overflow-y-auto rounded-t-none rounded-b-md border border-t-0 border-primary bg-popover p-1 text-popover-foreground shadow-lg" onmousedown={(event) => event.preventDefault()}>
            {#each suggestions as item, index (`${item.kind}|${item.url}`)}
              <div
                role="option"
                tabindex="-1"
                aria-selected={index === picked}
                class="flex min-w-0 cursor-pointer items-center gap-2 rounded-md px-2 py-1.5 text-xs {index === picked ? 'bg-primary/15 text-primary' : 'hover:bg-primary/10'}"
                onmousemove={() => (picked = index)}
                onclick={() => choose(item)}
                onkeydown={(event) => event.key === 'Enter' && choose(item)}
              >
                <ZIcon zType={(item.kind === 'history' ? 'lucideClock' : item.kind === 'go' ? 'lucideGlobe' : 'lucideSearch') as ZIconName} class="size-3.5 shrink-0 text-muted-foreground" />
                {#if item.kind === 'search'}
                  <span class="min-w-0 flex-1 truncate">Search Google for "{item.text}"</span>
                {:else if item.kind === 'go'}
                  <span class="min-w-0 flex-1 truncate">Go to {item.text}</span>
                {:else if item.kind === 'history'}
                  <span class="min-w-0 flex-1 truncate">{item.title || item.text.replace(/^https?:\/\//, '')}</span>
                  {#if item.title}<span class="max-w-[50%] shrink truncate text-muted-foreground">{item.text.replace(/^https?:\/\/(www\.)?/, '')}</span>{/if}
                {:else}
                  <span class="min-w-0 flex-1 truncate">{item.text}</span>
                {/if}
              </div>
            {/each}
          </div>
        {/if}
        <ZPopover anchor={lockButton} bind:open={lockOpen} zTrigger="manual" zPosition="bottom-left" zOffset={6} zWidth={280}>
          {#snippet children()}
            {#if security}
              <div class="grid gap-2 p-3 text-xs">
                <div class="flex items-center gap-2 font-medium {security.kind === 'secure' ? 'text-emerald-600 dark:text-emerald-400' : security.kind === 'insecure' ? 'text-amber-600 dark:text-amber-400' : ''}">
                  {security.kind === 'secure' ? 'Connection is secure' : security.kind === 'local' ? 'Local address (not encrypted)' : 'Connection is not secure'}
                </div>
                <p class="m-0 text-muted-foreground">
                  {security.kind === 'secure' ? 'Data sent to this site is encrypted.' : security.kind === 'local' ? 'This page runs on your own machine, so nothing leaves it.' : 'Anything you send to this site can be read by others. Do not enter passwords or card numbers.'}
                </p>
                <dl class="m-0 grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-1">
                  <dt class="text-muted-foreground">Protocol</dt><dd class="m-0">{security.protocol}</dd>
                  <dt class="text-muted-foreground">Host</dt><dd class="m-0 truncate" title={security.host}>{security.host}</dd>
                  <dt class="text-muted-foreground">Port</dt><dd class="m-0">{security.port}</dd>
                </dl>
                {#if security.kind === 'secure'}<p class="m-0 text-[11px] text-muted-foreground">Certificate details are not available in the embedded browser.</p>{/if}
              </div>
            {/if}
          {/snippet}
        </ZPopover>
      </form>
      <div class="flex shrink-0 items-center gap-0.5">
        <!-- Desktop, tablet, phone: one group, the current one pressed. -->
        <div class="mr-0.5 items-center rounded-md border border-border p-px {compact ? 'hidden' : 'flex'}" role="radiogroup" aria-label="Device">
          {#each DEVICE_BUTTONS as item, index (item.value)}
            <button
              type="button"
              role="radio"
              aria-checked={browser.device === item.value}
              aria-label={item.label}
              class="grid h-6 w-7 cursor-pointer place-items-center rounded-sm outline-none {browser.device === item.value ? 'bg-primary/15 text-primary' : 'text-muted-foreground hover:bg-primary/10 hover:text-primary'}"
              {@attach zTooltip({ content: item.label, position: 'top' })}
              onclick={() => (browser.device = item.value)}
            >
              <ZIcon zType={item.icon} class="size-3.5" />
            </button>
            {#if index < DEVICE_BUTTONS.length - 1}
              <span class="mx-px h-4 w-px shrink-0 bg-border" aria-hidden="true"></span>
            {/if}
          {/each}
        </div>
        <ZDrawer bind:open={processOpen} zPlacement="right" zWidth="380px" zFloating zClosable={false} zTitle={processTitle} footer={processFooter}>
<div class="flex min-h-full flex-col gap-2">
              {#if rowCount === 0}
                <div class="col-start-1 row-start-1 flex min-h-38 flex-col items-center justify-center gap-2 self-center text-xs text-muted-foreground"><ZIcon zType={'lucideServerOff' as ZIconName} zSize="24" /> Nothing is running in the background</div>
              {/if}
              {#each terminalRuns.list as run (run.id)}
                <div class="flex min-w-0 flex-wrap items-center gap-2 rounded-xl border border-border bg-card/40 px-3 py-2.5 transition-colors hover:border-primary/40">
                  <button type="button" class="flex min-w-0 flex-1 cursor-pointer items-center gap-2 rounded-md text-left text-xs outline-none" onclick={() => { if (run.url) openInPreview(run.url, 'auto'); else showTerminalTab(run.id); processOpen = false; }}>
                    <ZIcon zType={(run.url ? 'lucideExternalLink' : 'lucideSquareTerminal') as ZIconName} zSize="15" class="shrink-0 text-muted-foreground" />
                    <span class="flex min-w-0 flex-1 flex-col leading-tight">
                      <span class="flex min-w-0 items-center gap-1.5 text-[11px] text-muted-foreground">
                        <span class="truncate" title={run.cwd}>{run.cwd ? projectNameOf(run.cwd) : terminalName(run.id)}</span>
                        <span class="shrink-0 rounded bg-muted px-1 text-[10px] leading-4">Terminal</span>
                      </span>
                      <span class="mt-0.5 truncate" title={run.command}>{run.command.length > 34 ? `${run.command.slice(0, 34)}…` : run.command}{portOf(run) ? ` · :${portOf(run)}` : ''}</span>
                      <span class="mt-1 truncate text-[11px] {terminalState(run).tone}">{terminalState(run).label}</span>
                    </span>
                    <span class="shrink-0 whitespace-nowrap text-muted-foreground tabular-nums">{runningFor(terminalAge(run))}</span>
                  </button>
                  <button type="button" class="grid size-6 shrink-0 cursor-pointer place-items-center rounded-md text-muted-foreground outline-none hover:bg-primary/10 hover:text-primary" aria-label="Show output" aria-expanded={shown[`t:${run.id}`] ?? false} {@attach zTooltip({ content: 'Output', position: 'top' })} onclick={() => (shown[`t:${run.id}`] = !shown[`t:${run.id}`])}>
                    <ZIcon zType={'lucideChevronRight' as ZIconName} zSize="12" class="transition-transform duration-200 {shown[`t:${run.id}`] ? 'rotate-90' : ''}" />
                  </button>
                  <button type="button" class="grid size-6 shrink-0 cursor-pointer place-items-center rounded-md bg-destructive/10 text-destructive outline-none hover:bg-destructive/20" aria-label="Stop (Ctrl+C)" {@attach zTooltip({ content: 'Stop (Ctrl+C)', position: 'top' })} onclick={() => stopTerminalRun(run.id)}>
                    <svg viewBox="0 0 24 24" class="size-3 shrink-0" aria-hidden="true"><rect x="4" y="4" width="16" height="16" rx="4" fill="currentColor" /></svg>
                  </button>
                  {#if shown[`t:${run.id}`]}<RunCard terminalId={run.id} command={run.command} cwd={run.cwd} startedAt={run.startedAt} />{/if}
                </div>
              {/each}
              {#each servers.list as item, index (`${item.key}:${item.pid}`)}
                <!-- One line each, cut short when long; a line between rows. -->
                <div class="flex min-w-0 flex-wrap items-center gap-2 rounded-xl border border-border bg-card/40 px-3 py-2.5 transition-colors hover:border-primary/40">
                  <button type="button" class="flex min-w-0 flex-1 cursor-pointer items-center gap-2 rounded-md text-left text-xs outline-none" onclick={() => { if (item.url && item.running) { openInPreview(item.url, 'auto'); processOpen = false; } else toast.info(serverState(item).hint); }}>
                    <ZIcon zType={(item.running ? (item.url ? 'lucideExternalLink' : 'lucideLoaderCircle') : 'lucideCircleOff') as ZIconName} zSize="15" class="shrink-0 text-muted-foreground {item.running && !item.url ? 'animate-spin' : ''}" />
                    <span class="flex min-w-0 flex-1 flex-col leading-tight">
                      <span class="truncate text-[11px] text-muted-foreground" title={item.cwd}>{projectNameOf(item.cwd)}</span>
                      <span class="mt-0.5 truncate">{processName(item)}</span>
                      <span class="mt-1 truncate text-[11px] {serverState(item).tone}">{serverState(item).label}</span>
                    </span>
                    <span class="shrink-0 whitespace-nowrap text-muted-foreground tabular-nums">{runningFor(ageOf(item))}</span>
                  </button>
                  <button type="button" class="grid size-6 shrink-0 cursor-pointer place-items-center rounded-md text-muted-foreground outline-none hover:bg-primary/10 hover:text-primary" aria-label="Show output" aria-expanded={shown[`s:${item.key}`] ?? false} {@attach zTooltip({ content: 'Output', position: 'top' })} onclick={() => (shown[`s:${item.key}`] = !shown[`s:${item.key}`])}>
                    <ZIcon zType={'lucideChevronRight' as ZIconName} zSize="12" class="transition-transform duration-200 {shown[`s:${item.key}`] ? 'rotate-90' : ''}" />
                  </button>
                  <button type="button" class="grid size-6 shrink-0 cursor-pointer place-items-center rounded-md bg-destructive/10 text-destructive outline-none hover:bg-destructive/20" aria-label={item.running ? 'Stop' : 'Remove from the list'} {@attach zTooltip({ content: item.running ? 'Stop' : 'Remove from the list', position: 'top' })} onclick={() => void stopServers(item.key)}>
                    {#if item.running}<svg viewBox="0 0 24 24" class="size-3 shrink-0" aria-hidden="true"><rect x="4" y="4" width="16" height="16" rx="4" fill="currentColor" /></svg>{:else}<ZIcon zType={'lucideX' as ZIconName} zSize="12" />{/if}
                  </button>
                  {#if shown[`s:${item.key}`]}<RunCard port={item.key} command={item.command} cwd={item.cwd} startedAt={Date.now() - item.age * 1000} running={item.running} />{/if}
                </div>
              {/each}
</div>
        </ZDrawer>
      </div>
    </div>
    <div bind:clientWidth={toolsWidth} class="flex min-h-9 shrink-0 items-center justify-between gap-1 border-b border-border px-1 py-1" role="toolbar" aria-label="Page tools">
      <div class="flex min-w-0 items-center gap-0.5 overflow-hidden">
        <MenuButton items={screenshotItems} icon="lucideScan" label="Capture screenshot" width={220} />
        <span class="mx-0.5 h-4 w-px shrink-0 bg-border" aria-hidden="true"></span>
        {#each pageTools.slice(0, visibleToolCount) as item (`${item.label}:${item.icon}`)}
          {#if item.separatorBefore}<span class="mx-0.5 h-4 w-px shrink-0 bg-border" aria-hidden="true"></span>{/if}
          <button type="button" class="{icon} relative {item.checked ? '!bg-primary/15 !text-primary hover:!bg-primary/20 hover:!text-primary' : ''} {item.class ?? ''}" aria-label={item.label} title={item.label} aria-pressed={item.checked} disabled={item.disabled ?? false} onclick={() => item.onClick?.()}>
            <ZIcon zType={item.icon as ZIconName} class={item.icon === 'lucideSquare' ? 'size-3.5' : 'size-4'} />
            {#if item.icon === 'lucideSquareTerminal' && errors > 0}<span class="absolute -top-0.5 -right-0.5 min-w-3.5 rounded-full bg-destructive px-1 text-[9px] leading-3.5 text-white">{errors}</span>{/if}
            {#if item.icon === 'lucideServer' && alive.length > 0}<span class="pointer-events-none absolute -top-0.5 -right-0.5 min-w-3.5 rounded-full bg-primary px-1 text-center text-[9px] leading-3.5 text-primary-foreground">{alive.length}</span>{/if}
          </button>
        {/each}
      </div>
      <div data-page-menu class="flex shrink-0 items-center gap-1">
        <MenuButton items={moreItems} icon={compact ? 'lucideMenu' : 'lucideEllipsis'} label="More tools" width={240} />
      </div>
    </div>
    {#if browser.error}
      <div class="shrink-0 border-b border-border px-3 py-1 text-xs text-destructive" role="status">{browser.error}</div>
    {/if}
    <ScreenshotEditor
      bind:open={screenshotOpen}
      imageUrl={screenshotImage}
      filename={`screenshot-${new Date().toISOString().slice(0, 10)}.png`}
      onadd={(src) => {
        const id = workspace.activeId;
        draftImages[id] = [...(draftImages[id] ?? []), { id: crypto.randomUUID(), name: 'screenshot.png', mime: 'image/png', src }];
        toast.success('Added to the message');
      }}
    />
    {#if browser.problem}
      {@const problem = browser.problem}
      <div class="flex max-h-64 shrink-0 flex-col gap-1.5 overflow-hidden border-b border-border px-3 py-2 text-xs" role="alert">
        <div class="flex items-start gap-2">
          <p class="m-0 min-w-0 flex-1 wrap-break-word font-medium text-destructive">{problem.message}</p>
          <button type="button" class="grid size-5 shrink-0 cursor-pointer place-items-center rounded outline-none hover:bg-accent" aria-label="Dismiss" onclick={() => (browser.problem = null)}><ZIcon zType={'lucideX' as ZIconName} zSize="12" /></button>
        </div>
        {#if problem.hint}<p class="m-0 text-foreground">{problem.hint}</p>{/if}
        {#if problem.log}<pre class="m-0 min-h-0 flex-1 overflow-auto rounded-md bg-muted/60 p-2 font-mono text-[10px] leading-snug whitespace-pre-wrap text-muted-foreground">{problem.log}</pre>{/if}
        <div class="flex items-center gap-2">
          <button type="button" class="inline-flex cursor-pointer items-center gap-1.5 rounded-md bg-primary/10 px-2 py-1 text-primary outline-none hover:bg-primary/20" onclick={() => inTerminal(problem.command, problem.cwd)}>
            <ZIcon zType={'lucideSquareTerminal' as ZIconName} zSize="12" /> Run in the Terminal
          </button>
          {#if problem.interactive}<span class="text-muted-foreground">Answer its questions there, then open the address it prints.</span>{/if}
        </div>
      </div>
    {/if}

    <!-- The page itself lives in a native webview placed over this area. -->
    <div class="relative min-h-0 flex-1 bg-muted/30">
      <div bind:this={stage} class="absolute inset-0"></div>
      {#if deviceBox}
        <!-- The device: a bezel around the page (the page itself is the native layer above) and its caption. -->
        <div
          class="pointer-events-none absolute border-[6px] border-black bg-background shadow-lg ring-1 ring-border {browser.device === 'mobile' ? 'rounded-2xl' : 'rounded-xl'} {morphing ? glideClass : ''}"
          style:left="{deviceBox.x - 6}px"
          style:top="{deviceBox.y - 6}px"
          style:width="{deviceBox.w + 12}px"
          style:height="{deviceBox.h + 12}px"
          aria-hidden="true"
        ></div>
        <div class="absolute flex items-center justify-center gap-2 text-[11px] text-muted-foreground {morphing ? glideClass : ''}" style:left="{deviceBox.x - 6}px" style:width="{deviceBox.w + 12}px" style:top="{deviceBox.y - 30}px">
          <span>{DEVICES[browser.device].name} · {browser.landscape ? `${DEVICES[browser.device].h}×${DEVICES[browser.device].w}` : `${DEVICES[browser.device].w}×${DEVICES[browser.device].h}`}{deviceBox.zoom < 1 ? ` · ${Math.round(deviceBox.zoom * 100)}%` : ''}</span>
          <button type="button" class="grid size-5 cursor-pointer place-items-center rounded hover:bg-accent hover:text-foreground" aria-label="Rotate" onclick={() => (browser.landscape = !browser.landscape)} {@attach zTooltip({ content: 'Rotate', position: 'top' })}>
            <ZIcon zType={'lucideRotateCw' as ZIconName} class="size-3" />
          </button>
        </div>
      {/if}

      {#if !started}
        <div class="absolute grid place-items-center p-6 text-center text-xs text-muted-foreground {deviceBox ? '' : 'inset-0'}" style:left={deviceBox ? `${deviceBox.x}px` : undefined} style:top={deviceBox ? `${deviceBox.y}px` : undefined} style:width={deviceBox ? `${deviceBox.w}px` : undefined} style:height={deviceBox ? `${deviceBox.h}px` : undefined}>
          <div class="flex flex-col items-center gap-2">
            <ZIcon zType="lucideGlobe" zSize="28" class="size-7 opacity-60" />
            <p class="m-0">{targets.length ? 'Run the project, or enter an address above.' : 'Search or enter an address above.'}</p>
            {#if targets.length}
              <div class="mt-1 flex w-72 max-w-full flex-col gap-1 text-left" role="list" aria-label="Ways to run the project">
                {#each targets as target (`${target.cwd}|${target.command}`)}
                  {@const key = `${target.cwd}|${target.command}`}
                  <div class="flex items-center gap-2 rounded-md border border-border bg-background/60 px-2.5 py-1.5" role="listitem">
                    <div class="min-w-0 flex-1">
                      <div class="flex min-w-0 items-center gap-1.5">
                        <span class="truncate text-xs font-medium text-foreground">{target.name}</span>
                        {#if target.url}<span class="shrink-0 rounded bg-muted px-1 font-mono text-[9px] leading-4">{target.url.match(/:(\d+)/)?.[1]}</span>{/if}
                        {#if target.wrapper}<span class="shrink-0 rounded bg-muted px-1 font-mono text-[9px] leading-4" title="Runs {target.wrapper}{target.prompts ? ', which asks questions: run it in the Terminal' : ''}">script{target.prompts ? ' · asks' : ''}</span>{/if}
                      </div>
                      <div class="truncate font-mono text-[10px]" title={target.command}>{target.command}</div>
                    </div>
                    <button
                      type="button"
                      class="grid size-7 shrink-0 cursor-pointer place-items-center rounded-md outline-none hover:bg-accent hover:text-foreground disabled:pointer-events-none disabled:opacity-50"
                      aria-label="Run {target.name}"
                      disabled={starting.includes(key)}
                      {@attach zTooltip({ content: starting.includes(key) ? 'Starting…' : 'Run and open in the Preview', position: 'top' })}
                      onclick={() => void runProject(target)}
                    >
                      {#if starting.includes(key)}<ZLoading zLoading zSpinner="spinner" zSize="xs" zColor="primary" />{:else}<ZIcon zType={'lucidePlay' as ZIconName} class="size-4" />{/if}
                    </button>
                  </div>
                {/each}
              </div>
            {:else}
              <p class="m-0 max-w-60">Dev servers on localhost work best. Inspect elements, mark up the page, and send it all to the chat.</p>
            {/if}
          </div>
        </div>
      {/if}
    </div>

  {/if}
</div>
