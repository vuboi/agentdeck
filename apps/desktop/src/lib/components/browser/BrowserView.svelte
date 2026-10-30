<script lang="ts">
  import MenuButton from '../ui/MenuButton.svelte';
  import { TRIGGER_ON } from '../../app/ui/popover';
  import { guide } from '../../app/ui/guide.svelte';
  import { fly } from 'svelte/transition';
  import { onDestroy, onMount, tick, untrack } from 'svelte';
  import { ZIcon, ZLoading, zTooltip } from '@shival99/z-ui-svelte';
  import { toast } from '../../app/platform/toast';
  import { logFlow } from '../../tasks/flow-log';
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
    screenshot,
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
    terminalCommand,
    type RunTarget,
    reload,
    searchSuggestions,
    selectTab,
    showBrowser,
    stop,
    tabsFull,
    zoomBrowser,
    captureElementFile,
    capturePageFileIfPossible,
    type Device,
    type Rect,
  } from '../../browser/browser.svelte';
  import { inTauri } from '../../app/platform/history';
  import { trackRun } from '../../chat/runs/activity.svelte';
  import BrowserTabStrip from './BrowserTabStrip.svelte';
  import BrowserSuggestions from './BrowserSuggestions.svelte';
  import BrowserSecurityPopover from './BrowserSecurityPopover.svelte';
  import BrowserProcessDrawer from './BrowserProcessDrawer.svelte';
  import BrowserExtensionsMenu from './BrowserExtensionsMenu.svelte';
  import BrowserStartPage from './BrowserStartPage.svelte';
  import BrowserDeviceFrame from './BrowserDeviceFrame.svelte';
  import { SEARCH, searchUrl, normalize, siteSecurity, clip, cropScreenshot, type Suggestion } from './browser-helpers';
  import ScreenshotEditor from '../files/ScreenshotEditor.svelte';
  import { blurPage, cutHole, modalBlur, overPage, setBelow } from '../../browser/page-layer';
  import { devtoolsOpen, layout, setDevtools } from '../../app/ui/layout.svelte';
  import { runInTerminal, showTerminalTab, stopTerminalRun, terminal, terminalRuns } from '../../terminal/terminal.svelte';
  import { activeSession, draftElements, draftImages, drafts, projectOf, workspace } from '../../chat/sessions.svelte';
  import { keepPicture, keepPictureUrl, pictureUrl } from '../../chat/agent';

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
    // An address that could not be opened takes the page's place (its error shows there instead).
    if (inTauri() && (!started || browser.navError) && !layout.dragging) {
      await takePageAway();
      return;
    }
    if (!inTauri() || !started || browser.navError || layout.dragging) return;
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
      if (!browser.navError) browser.error = String(error);
    }
  }

  async function go() {
    const url = normalize(address);
    if (!url) return;
    browser.error = '';
    editing = false;
    dirty = false;
    address = url;
    const navigatingTab = tab;
    navigatingTab.loading = true;
    // Paint both spinners before the native webview starts opening the address.
    await painted();
    if (tab.id !== navigatingTab.id || !navigatingTab.loading) {
      navigatingTab.loading = false;
      return;
    }
    // The address is set before the page opens, so the first open never uses a stale one.
    try {
      await goTo(url);
      if (navigatingTab.opened) return;
      await tick();
      await sync();
    } catch (error) {
      navigatingTab.loading = false;
      browser.navError = String(error);
    }
  }

  // ---- address bar: security, and suggestions while typing ----
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

  const security = $derived(siteSecurity(tab.url));
  let lockButton = $state<HTMLElement | null>(null);
  let lockOpen = $state(false);
  /** Extensions run in the Windows Preview only (WebView2). */
  const canExtend = inTauri() && /windows/i.test(navigator.userAgent);

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
    void browser.navError;
    // The mark-up bar opens in this very update, so the page moves before the next frame is drawn.
    void browser.annotating;
    // A floating window being carried or resized: the page steps aside and comes back where it lands.
    if (layout.dragging) untrack(() => void takePageAway());
    void layout.dragging;
    untrack(() => void sync());
  });

  // ---- pick, comment, annotate, send ----
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
  // Inspect stays on after an element is picked, so several can be added one after another. The toggle, Escape, a
  // new page or another tab ends it.
  let keepPicking = false;

  async function pick() {
    if (browser.picking) {
      keepPicking = false;
      void cancelPick();
      return;
    }
    browser.error = '';
    keepPicking = true;
    try {
      if (browser.annotating) await stopAnnotating();
      while (keepPicking) {
        const picked = await pickElement();
        if (!picked || !keepPicking) return;
        // The mode stays lit while the element is captured, so the button does not flicker between two picks.
        browser.picking = true;
        const { element, url, title } = picked;
        const id = workspace.activeId;
        // A picture of the element alone, so the agent sees it next to its markup.
        const path = await captureElementFile(browser.activeId, element.box);
        const picture = path ? { url: await pictureUrl(path), path } : undefined;
        draftElements[id] = [...(draftElements[id] ?? []), { id: crypto.randomUUID(), picture: picture?.url, picturePath: picture?.path, tag: element.tag, text: clip(element.text, 80), selector: element.selector, html: clip(element.html, 2500), around: element.around, box: element.box, style: element.style, url, title }];
        toast.success('Element added to the message');
      }
    } catch (error) {
      browser.error = String(error);
    } finally {
      keepPicking = false;
      browser.picking = false;
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
    const id = workspace.activeId;
    logFlow(id, 'marks to chat start', `annotating=${browser.annotating} count=${browser.annotateCount}`);
    console.info('[marks to chat] start', { annotating: browser.annotating, count: browser.annotateCount, id });
    toast.info(`Adding notes to chat… (${browser.annotateCount} marks)`);
    try {
      const notes = await annotationNotes();
      logFlow(id, 'marks to chat notes', JSON.stringify(notes.map((note) => ({ n: note.n, text: note.text, element: !!note.element }))));
      // What was written goes into the message at once: the words, and each marked element with its note. They do
      // not wait for the picture, which can be slow or fail (then the page's own capture is tried, below).
      // What was written on an element goes with that element, as its note. The rest, drawn on the picture, are
      // numbered lines in the message.
      const words = notes
        .filter((note) => note.text && !note.element)
        .map((note) => `${note.n}. ${note.text}`)
        .join('\n');
      if (words) drafts[id] = [drafts[id], words].filter(Boolean).join('\n');
      const marked = notes.flatMap((note) => (note.element ? [{ element: note.element, note: note.text ? `${note.n}. ${note.text}` : `${note.n}.` }] : []));
      // An element that was marked travels as an element too (its markup and look), next to the picture.
      const chips = marked.map(({ element, note }) => ({ id: crypto.randomUUID(), tag: element.tag, text: clip(element.text, 80), selector: element.selector, html: clip(element.html, 2500), around: element.around, box: element.box, style: element.style, url: tab.url, title: tab.title, note }));
      if (chips.length) draftElements[id] = [...(draftElements[id] ?? []), ...chips];
      // The page as it is now, marks included: the engine's capture, else the page's own (the one the agent's screenshot
      // uses). Without a picture the words and elements above are still in the message.
      logFlow(id, 'marks to chat capture', `words=${words.length} chips=${chips.length}`);
      // The page is saved as a WebP file at once (its path); the page's own screenshot is the fallback, kept as a file too.
      const file = await capturePageFileIfPossible();
      const picture = file ? undefined : await screenshot().then((shot) => shot.data).catch(() => undefined);
      logFlow(id, 'marks to chat picture', file ? `file=${file}` : picture ? `length=${picture.length}` : 'none');
      if (!file && !picture && !words && !chips.length) throw new Error('Could not take the picture of the page to add it to the chat. Try again.');
      if ((file || picture) && chips.length) {
        const kept = file ? { url: await pictureUrl(file), path: file } : await keepPictureUrl(picture!);
        draftElements[id] = (draftElements[id] ?? []).map((item) => (item.id === chips[0].id ? { ...item, picture: kept.url, picturePath: kept.path } : item));
      }
      // Keep the full marked-page capture with its element chip, like an Inspect capture. Freehand-only marks have
      // no element chip, so their screenshot stays in the regular image attachments.
      if (file && !chips.length) draftImages[id] = [...(draftImages[id] ?? []), { id: crypto.randomUUID(), name: 'page-notes.webp', mime: 'image/webp', src: '', path: file }];
      else if (picture && !chips.length) draftImages[id] = [...(draftImages[id] ?? []), await keepPicture({ id: crypto.randomUUID(), name: 'page-notes.png', mime: 'image/png', src: picture })];
      await stopAnnotating();
      if (!picture) browser.error = 'The picture of the page could not be taken: the notes went to the message as text.';
      else toast.success('Added to the message');
    } catch (error) {
      logFlow(id, 'marks to chat error', String(error));
      browser.error = String(error);
    } finally {
      logFlow(id, 'marks to chat end', `busy was ${busy}`);
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
  let captureToolsWidth = $state(0);
  let menuToolsWidth = $state(0);
  const alive = $derived({ length: servers.list.filter((item) => item.running).length + terminalRuns.list.length });
  let processOpen = $state(false);
  const mainTools = $derived<ZDropdownMenuItem[]>([
    { label: browser.picking ? 'Stop inspecting' : 'Inspect an element', icon: 'lucideMousePointerClick', iconSize: 12, checked: browser.picking, disabled: !started, onClick: () => void pick() },
    { label: browser.annotating ? 'Stop marking up' : 'Mark up the page', icon: 'lucideMousePointer2', iconSize: 12, checked: browser.annotating, disabled: !started, onClick: () => void toggleMarkup() },
    { label: 'Create guide', icon: 'lucideClipboardList', iconSize: 12, disabled: !started, onClick: () => (guide.open = true) },
    { label: errors > 0 ? `Developer tools (${errors} errors)` : 'Developer tools', icon: 'lucideSquareTerminal', iconSize: 12, checked: devtoolsOpen(), onClick: () => setDevtools(!devtoolsOpen()) },
    { label: alive.length > 0 ? `Running in the background (${alive.length})` : 'Running in the background', icon: 'lucideServer', iconSize: 12, checked: processOpen, onClick: () => (processOpen = !processOpen) },
  ]);
  const noteTools = $derived<(ZDropdownMenuItem & { separatorBefore?: boolean })[]>([
    ...(browser.annotating
      ? [
          ...MARK_TOOLS.map((item, index) => ({ label: item.label, icon: item.icon, iconSize: 12, checked: browser.annotateTool === item.tool, separatorBefore: index === 0, onClick: () => void useTool(item.tool) })),
          { label: 'Undo', icon: 'lucideUndo2' as ZIconName, iconSize: 12, separatorBefore: true, disabled: !browser.annotateUndo, onClick: () => void markHistory('undo') },
          { label: 'Redo', icon: 'lucideRedo2' as ZIconName, iconSize: 12, disabled: !browser.annotateRedo, onClick: () => void markHistory('redo') },
          { label: browser.annotateSelected ? 'Delete selected note' : 'Delete all notes', icon: 'lucideTrash2' as ZIconName, iconSize: 12, class: 'text-destructive!', disabled: browser.annotateCount === 0, onClick: () => void markHistory('remove') },
          ...(browser.annotateCount > 0 ? [{ label: 'Add notes to chat', icon: 'lucidePlus' as ZIconName, iconSize: 12, class: 'z-button-shimmer border border-transparent rounded-md text-primary! font-medium', disabled: busy !== '', onClick: () => void addMarksToChat() }] : []),
        ]
      : []),
  ]);
  const pageTools = $derived<(ZDropdownMenuItem & { separatorBefore?: boolean })[]>([...mainTools, ...noteTools]);
  // Measure the fixed groups too: capture, extensions and More must never squeeze the tool buttons.
  const visibleToolCount = $derived.by(() => {
    let used = 0;
    let count = 0;
    const available = Math.max(0, toolsWidth - captureToolsWidth - menuToolsWidth - 12);
    for (const item of pageTools) {
      const next = 30 + (item.separatorBefore ? 7 : 0);
      if (used + next > available) break;
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
          ...DEVICE_BUTTONS.map((item) => ({ label: item.label, icon: item.icon, iconSize: 12, class: `text-sm! py-1 ${browser.device === item.value ? 'bg-primary/10 text-primary' : ''}`, checked: browser.device === item.value, onClick: () => (browser.device = item.value) })),
          { label: 'Rotate', icon: 'lucideRotateCw' as ZIconName, iconSize: 12, class: `text-sm! py-1 ${browser.landscape && browser.device !== 'desktop' ? 'bg-primary/10 text-primary' : ''}`, disabled: browser.device === 'desktop', checked: browser.landscape && browser.device !== 'desktop', onClick: () => (browser.landscape = !browser.landscape) },
        ]
      : []),
    // Always offered (on Windows): with no page open the extension is put in place and loads with the next one.
    { label: canExtend ? 'Add unpacked extension…' : 'Extensions require Windows', icon: 'lucidePuzzle' as ZIconName, iconSize: 12, class: 'text-sm! py-1', divide: 'before', disabled: !canExtend, onClick: () => void extMenu?.addExtension() },
    { label: 'Open in external browser', icon: 'lucideExternalLink' as ZIconName, iconSize: 12, class: 'text-sm! py-1', divide: compact ? 'before' : undefined, disabled: !tab.url, onClick: () => void openExternal(tab.url) },
    { label: 'Copy address', icon: 'lucideCopy' as ZIconName, iconSize: 12, class: 'text-sm! py-1', disabled: !tab.url, onClick: () => void navigator.clipboard.writeText(tab.url).then(() => toast.success('Address copied')) },
    { label: 'Close tab', icon: 'lucideX' as ZIconName, iconSize: 12, class: 'text-sm! py-1', divide: 'before', onClick: () => void closeTab(tab.id) },
  ]);
  const moreItems = $derived<ZDropdownMenuItem[]>([
    ...mainTools.slice(visibleToolCount),
    ...menuItems,
    // Overflow notes belong together at the bottom, once, using Menu's actual section API.
    ...noteTools.slice(Math.max(0, visibleToolCount - mainTools.length)).map((item, index) => ({
      ...item,
      divide: index === 0 || item.separatorBefore ? 'before' as const : undefined,
    })),
  ]);

  let extOpen = $state(false);
  let extMenu = $state<{ addExtension: () => Promise<void> } | undefined>();

  const icon = 'grid size-6 shrink-0 cursor-pointer place-items-center rounded-md text-muted-foreground outline-none hover:bg-primary/10 hover:text-primary disabled:pointer-events-none disabled:opacity-40';
  const tabTitle = (item: { title: string; url: string }) => item.title || (item.url ? item.url.replace(/^https?:\/\//, '') : 'New tab');
</script>

<svelte:window onkeydown={onWindowKey} />

<div class="flex h-full w-full min-h-0 flex-1 flex-col" aria-label="Browser">
  {#if !inTauri()}
    <div class="grid flex-1 place-items-center p-6 text-center text-sm text-muted-foreground">The browser runs in the AgentDeck desktop app.</div>
  {:else}
    <BrowserTabStrip />

    <!-- Address bar: navigation and address stay together; page tools live in the row below. -->
    <div bind:clientWidth={barWidth} class="flex h-10 shrink-0 items-center gap-1 border-b border-border px-1.5">
      <div class="flex shrink-0 items-center gap-0.5">
        <button type="button" class="{icon} size-8!" aria-label="Back" {@attach zTooltip({ content: 'Back', position: 'top' })} onclick={() => void back()}><ZIcon zType={'lucideArrowLeft' as ZIconName} class="size-4" /></button>
        <button type="button" class="{icon} size-8!" aria-label="Forward" {@attach zTooltip({ content: 'Forward', position: 'top' })} onclick={() => void forward()}><ZIcon zType={'lucideArrowRight' as ZIconName} class="size-4" /></button>
        <button type="button" class="{icon} size-8!" aria-label={tab.loading ? 'Stop' : 'Reload'} {@attach zTooltip({ content: tab.loading ? 'Stop loading' : 'Reload', position: 'top' })} onclick={() => void (tab.loading ? stop() : reload())}>
          {#if tab.loading}<ZLoading zLoading zSpinner="spinner" zSize="xs" zColor="primary" />{:else}<ZIcon zType={'lucideRotateCw' as ZIconName} class="size-4" />{/if}
        </button>
      </div>
      <form class="relative min-w-0 flex-1" onsubmit={(event) => { event.preventDefault(); void go(); }}>
        <button
          bind:this={lockButton}
          type="button"
          class="absolute top-0.5 left-0.5 z-10 grid size-7 cursor-pointer place-items-center rounded outline-none hover:bg-primary/10 {security?.kind === 'secure' ? 'text-emerald-600 dark:text-emerald-400' : security?.kind === 'insecure' ? 'text-amber-600 dark:text-amber-400' : 'text-muted-foreground'}"
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
          class="h-8 w-full border bg-muted/40 pr-3 pl-9 text-sm outline-none transition-colors placeholder:text-muted-foreground focus:border-primary focus:bg-background {suggestOpen ? 'rounded-t-md rounded-b-none border-primary border-b-border bg-background' : 'rounded-md border-border'}"
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
          <BrowserSuggestions {suggestions} bind:picked onchoose={choose} />
        {/if}
        <BrowserSecurityPopover anchor={lockButton} bind:open={lockOpen} {security} />
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
              class="grid h-7 w-8 cursor-pointer place-items-center rounded-sm outline-none {browser.device === item.value ? 'bg-primary/15 text-primary' : 'text-muted-foreground hover:bg-primary/10 hover:text-primary'}"
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
        <BrowserProcessDrawer bind:open={processOpen} />
      </div>
    </div>
    <div bind:clientWidth={toolsWidth} class="flex min-h-9 shrink-0 items-center justify-between gap-1 border-b border-border px-1 py-1" role="toolbar" aria-label="Page tools">
      <div class="flex min-w-0 flex-1 items-center gap-0.5 overflow-hidden">
        <div bind:clientWidth={captureToolsWidth} class="flex shrink-0 items-center gap-0.5">
          <MenuButton items={screenshotItems} icon="lucideScan" label="Capture screenshot" width={220} />
          <span class="mx-0.5 h-4 w-px shrink-0 bg-border" aria-hidden="true"></span>
        </div>
        {#each pageTools.slice(0, visibleToolCount) as item (`${item.label}:${item.icon}`)}
          {#if item.separatorBefore}<span class="mx-0.5 h-4 w-px shrink-0 bg-border" aria-hidden="true"></span>{/if}
          <button type="button" class="{icon} relative {item.checked ? '!bg-primary/15 !text-primary hover:!bg-primary/20 hover:!text-primary' : ''} {item.class ?? ''}" aria-label={item.label} title={item.label} aria-pressed={item.checked} disabled={item.disabled ?? false} onclick={() => item.onClick?.()}>
            <ZIcon zType={item.icon as ZIconName} class={item.icon === 'lucideSquare' ? 'size-3.5' : 'size-4'} />
            {#if item.icon === 'lucideSquareTerminal' && errors > 0}<span class="absolute -top-0.5 -right-0.5 min-w-3.5 rounded-full bg-destructive px-1 text-[9px] leading-3.5 text-white">{errors}</span>{/if}
            {#if item.icon === 'lucideServer' && alive.length > 0}<span class="pointer-events-none absolute -top-0.5 -right-0.5 min-w-3.5 rounded-full bg-primary px-1 text-center text-[9px] leading-3.5 text-primary-foreground">{alive.length}</span>{/if}
          </button>
        {/each}
      </div>
      <div bind:clientWidth={menuToolsWidth} data-page-menu class="flex shrink-0 items-center gap-1">
        {#if canExtend}
          <BrowserExtensionsMenu bind:this={extMenu} bind:open={extOpen} buttonClass={icon} />
        {/if}
        <MenuButton items={moreItems} icon={compact ? 'lucideMenu' : 'lucideEllipsis'} label="More tools" width={240} />
      </div>
    </div>
    {#if browser.error}
      <div class="shrink-0 border-b border-border px-3 py-1 text-sm text-destructive" role="status">{browser.error}</div>
    {/if}
    <ScreenshotEditor
      bind:open={screenshotOpen}
      imageUrl={screenshotImage}
      filename={`screenshot-${new Date().toISOString().slice(0, 10)}.png`}
      onadd={async (src) => {
        const id = workspace.activeId;
        draftImages[id] = [...(draftImages[id] ?? []), await keepPicture({ id: crypto.randomUUID(), name: 'screenshot.png', mime: 'image/png', src })];
        toast.success('Added to the message');
      }}
    />
    {#if browser.problem}
      {@const problem = browser.problem}
      <div class="flex max-h-64 shrink-0 flex-col gap-1.5 overflow-hidden border-b border-border px-3 py-2 text-sm" role="alert">
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
        <BrowserDeviceFrame {deviceBox} {morphing} />
      {/if}

    {#if !started}
      <BrowserStartPage {deviceBox} {targets} {starting} onrun={(target) => void runProject(target)} />
    {/if}
    {#if browser.navError}
      <div class="absolute inset-0 flex flex-col items-center justify-center gap-2 p-6 text-center text-sm" role="alert">
        <ZIcon zType={'lucideCircleAlert' as ZIconName} zSize="28" class="size-7 text-destructive" />
        <p class="m-0 max-w-96 break-words text-destructive">{browser.navError}</p>
      </div>
    {/if}
    </div>

  {/if}
</div>
