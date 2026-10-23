<script lang="ts">
  import { fly } from 'svelte/transition';
  import { onDestroy, onMount, tick, untrack } from 'svelte';
  import { ZDropdownMenu, ZIcon, ZLoading, zTooltip } from '@shival99/z-ui-svelte';
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
    forward,
    goTo,
    capturePage,
    hideBrowser,
    listenToPage,
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
    type RunTarget,
    reload,
    selectTab,
    showBrowser,
    stop,
    zoomBrowser,
    type Device,
    type Rect,
  } from '../browser.svelte';
  import { inTauri } from '../history';
  import { trackRun } from '../activity.svelte';
  import { blurPage, cutHole, modalBlur, overPage, setBelow } from '../page-layer';
  import { layout } from '../layout.svelte';
  import { activeSession, draftElements, draftImages, drafts, projectOf, workspace } from '../sessions.svelte';
  import DevTools from './DevTools.svelte';

  // The AgentDeck browser: tabs, an address bar, device sizes, an element picker, devtools, and a markup view
  // whose result goes to the chat. The pages themselves are native webviews placed over the stage below.
  let stage: HTMLDivElement | undefined = $state();
  let address = $state('');
  let addressInput: HTMLInputElement | undefined = $state();

  // Ctrl+L: the address, selected, ready to type over, while this browser is the one on screen.
  function onWindowKey(event: KeyboardEvent) {
    if (!(event.ctrlKey || event.metaKey) || event.shiftKey || event.altKey || event.key.toLowerCase() !== 'l' || !addressInput) return;
    event.preventDefault();
    addressInput.focus();
    addressInput.select();
  }
  let editing = $state(false);
  let busy = $state('');
  /** Resolves once what was just rendered has been drawn. */
  const painted = () => new Promise<void>((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => resolve())));
  /** Where the phone / tablet sits in the stage (stage coordinates), for its frame. */
  let deviceBox = $state<{ x: number; y: number; w: number; h: number; zoom: number } | null>(null);

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
    if (starting) return;
    browser.starting = `${target.cwd}|${target.command}`;
    browser.error = '';
    const session = activeSession();
    // The Audit log gets the run: what was started, where, and the address it came up at (or why it did not).
    const finish = trackRun(session?.id ?? '', session?.title ?? '', (session && projectOf(session)?.name) || '', `${target.command}\n${target.cwd}`);
    try {
      const url = await startTarget(target);
      finish(true, `${target.name}: ${url}`);
      // The same path a link takes: the front tab, or a new one when it already shows a page.
      openInPreview(url, 'auto');
    } catch (error) {
      finish(false, String(error));
      browser.error = String(error);
    } finally {
      browser.starting = '';
    }
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
    // An empty tab (a new one, nothing typed yet) shows no page: the previous tab's page must go away.
    if (inTauri() && !started && !layout.dragging) {
      await takePageAway();
      return;
    }
    if (!inTauri() || !started || layout.dragging) return;
    const place = frame();
    if (!place) {
      await takePageAway();
      return;
    }
    const origin = stage!.getBoundingClientRect();
    deviceBox = DEVICES[browser.device].w ? { x: place.rect.x - origin.x, y: place.rect.y - origin.y, w: place.rect.w, h: place.rect.h, zoom: place.zoom } : null;
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
    address = url;
    // The address is set before the page opens, so the first open never uses a stale one.
    if (tab.opened) await goTo(url);
    else {
      tab.url = url;
      await tick();
      await sync();
    }
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
    void browser.devtools;
    void browser.devtoolsHeight;
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
      draftElements[id] = [...(draftElements[id] ?? []), { id: crypto.randomUUID(), tag: element.tag, text: clip(element.text, 80), selector: element.selector, html: clip(element.html, 2500), box: element.box, style: element.style, url, title }];
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
      const picture = await capturePage();
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
          ...marked.map(({ element, note }) => ({ id: crypto.randomUUID(), tag: element.tag, text: clip(element.text, 80), selector: element.selector, html: clip(element.html, 2500), box: element.box, style: element.style, url: tab.url, title: tab.title, note })),
        ];
      }
      draftImages[id] = [...(draftImages[id] ?? []), { id: crypto.randomUUID(), name: 'page-notes.png', mime: 'image/png', src: picture }];
      await stopAnnotating();
      toast.success('Added to the message');
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
  // A narrow toolbar hides the device group; its choices move to the top of this menu.
  let barWidth = $state(1000);
  const compact = $derived(barWidth < 560);
  const menuItems = $derived<ZDropdownMenuItem[]>([
    ...(compact
      ? [
          ...DEVICE_BUTTONS.map((item) => ({ label: item.label, icon: item.icon, iconSize: 12, class: 'text-xs! py-1', checked: browser.device === item.value, onClick: () => (browser.device = item.value) })),
          { label: 'Rotate', icon: 'lucideRotateCw' as ZIconName, iconSize: 12, class: 'text-xs! py-1', disabled: browser.device === 'desktop', checked: browser.landscape && browser.device !== 'desktop', onClick: () => (browser.landscape = !browser.landscape) },
        ]
      : []),
    { label: 'Open in external browser', icon: 'lucideExternalLink' as ZIconName, iconSize: 12, class: 'text-xs! py-1', divide: compact ? 'before' : undefined, disabled: !tab.url, onClick: () => void openExternal(tab.url) },
    { label: 'Copy address', icon: 'lucideCopy' as ZIconName, iconSize: 12, class: 'text-xs! py-1', disabled: !tab.url, onClick: () => void navigator.clipboard.writeText(tab.url).then(() => toast.success('Address copied')) },
    { label: 'Close tab', icon: 'lucideX' as ZIconName, iconSize: 12, class: 'text-xs! py-1', divide: 'before', onClick: () => void closeTab(tab.id) },
  ]);

  // What runs in the background (servers started from the Preview or by an agent): open it, or stop it.
  const alive = $derived(servers.list.filter((item) => item.running));
  const portOf = (item: { url: string | null }) => item.url?.match(/:(\d+)/)?.[1];
  const runningFor = (age: number) => (age < 60 ? `${age}s` : age < 3600 ? `${Math.floor(age / 60)}m` : `${Math.floor(age / 3600)}h ${Math.floor((age % 3600) / 60)}m`);
  const processItems = $derived<ZDropdownMenuItem[]>(
    servers.list.length === 0
      ? [{ label: 'Nothing is running in the background', icon: 'lucideServerOff' as ZIconName, iconSize: 12, class: 'text-xs! py-1', disabled: true }]
      : [
          ...servers.list.flatMap((item, index) => {
            const name = `${item.command.length > 34 ? `${item.command.slice(0, 34)}…` : item.command}${portOf(item) ? ` · :${portOf(item)}` : ''}`;
            return [
              { label: `${item.running ? '' : 'Exited · '}${name} · ${runningFor(item.age)}`, icon: (item.running ? 'lucideExternalLink' : 'lucideCircleOff') as ZIconName, iconSize: 12, class: 'text-xs! py-1', divide: index > 0 ? ('before' as const) : undefined, disabled: !item.running || !item.url, onClick: () => item.url && openInPreview(item.url, 'auto') },
              { label: item.running ? 'Stop' : 'Remove from the list', icon: 'lucideSquare' as ZIconName, iconSize: 12, class: 'text-xs! py-1 pl-6! text-destructive!', onClick: () => void stopServers(item.key) },
            ];
          }),
          ...(servers.list.length > 1 ? [{ label: 'Stop all', icon: 'lucideSquare' as ZIconName, iconSize: 12, class: 'text-xs! py-1 text-destructive!', divide: 'before' as const, onClick: () => void stopServers(null) }] : []),
        ],
  );

  const icon = 'grid size-7 shrink-0 cursor-pointer place-items-center rounded-md text-muted-foreground outline-none hover:bg-primary/10 hover:text-primary disabled:pointer-events-none disabled:opacity-40';
  const tabTitle = (item: { title: string; url: string }) => item.title || (item.url ? item.url.replace(/^https?:\/\//, '') : 'New tab');
</script>

<svelte:window onkeydown={onWindowKey} />

<div class="flex h-full min-h-0 flex-1 flex-col" aria-label="Browser">
  {#if !inTauri()}
    <div class="grid flex-1 place-items-center p-6 text-center text-xs text-muted-foreground">The browser runs in the AgentDeck desktop app.</div>
  {:else}
    <!-- Tabs -->
    <div class="flex shrink-0 items-center gap-1 px-2 py-1.5" role="tablist" aria-label="Browser tabs">
      <div class="flex min-w-0 flex-1 items-center gap-1 overflow-x-auto">
        {#each browser.tabs as item (item.id)}
          <div
            role="tab"
            tabindex="0"
            aria-selected={item.id === browser.activeId}
            class="group/tab flex h-7 min-w-24 max-w-44 shrink cursor-pointer items-center gap-1.5 rounded-lg pr-1 pl-2 text-xs outline-none {item.id === browser.activeId ? 'bg-primary/15 text-primary' : 'text-muted-foreground hover:bg-primary/10 hover:text-primary'}"
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
            <button type="button" class="grid size-4.5 shrink-0 cursor-pointer place-items-center rounded-full outline-none hover:bg-primary/20" aria-label="Close tab" onclick={(event) => { event.stopPropagation(); void closeTab(item.id); }}>
              <ZIcon zType="lucideX" class="size-3" />
            </button>
          </div>
        {/each}
      </div>
      <button type="button" class="grid size-7 shrink-0 cursor-pointer place-items-center rounded-lg text-muted-foreground outline-none hover:bg-primary/10 hover:text-primary" aria-label="New tab" {@attach zTooltip({ content: 'New tab', position: 'top' })} onclick={() => newTab()}>
        <ZIcon zType="lucidePlus" class="size-4" />
      </button>
    </div>

    <!-- Address bar: navigation and inspector on the left, the address in the middle, markup / devtools / menu on the right -->
    <div bind:clientWidth={barWidth} class="flex shrink-0 items-center gap-1 border-b border-border px-2 pb-1.5">
      <div class="flex shrink-0 items-center gap-0.5">
        <button type="button" class={icon} aria-label="Back" {@attach zTooltip({ content: 'Back', position: 'top' })} onclick={() => void back()}><ZIcon zType={'lucideArrowLeft' as ZIconName} class="size-4" /></button>
        <button type="button" class={icon} aria-label="Forward" {@attach zTooltip({ content: 'Forward', position: 'top' })} onclick={() => void forward()}><ZIcon zType={'lucideArrowRight' as ZIconName} class="size-4" /></button>
        <button type="button" class={icon} aria-label={tab.loading ? 'Stop' : 'Reload'} {@attach zTooltip({ content: tab.loading ? 'Stop loading' : 'Reload', position: 'top' })} onclick={() => void (tab.loading ? stop() : reload())}>
          {#if tab.loading}<ZLoading zLoading zSpinner="spinner" zSize="xs" zColor="primary" />{:else}<ZIcon zType={'lucideRotateCw' as ZIconName} class="size-4" />{/if}
        </button>
        <button type="button" class="{icon} {browser.picking ? 'bg-primary/15 text-primary' : ''}" aria-label="Inspect an element" disabled={!started} aria-pressed={browser.picking} {@attach zTooltip({ content: browser.picking ? 'Stop inspecting' : 'Inspect an element and add it to the message', position: 'top' })} onclick={() => void pick()}>
          <ZIcon zType={'lucideMousePointerClick' as ZIconName} class="size-4" />
        </button>
        <button type="button" class="{icon} {browser.annotating ? 'bg-primary/15 text-primary' : ''}" aria-label="Mark up the page" disabled={!started} aria-pressed={browser.annotating} {@attach zTooltip({ content: browser.annotating ? 'Stop marking up' : 'Mark up the page: draw and write notes', position: 'top' })} onclick={() => void toggleMarkup()}>
          <ZIcon zType={'lucideMousePointer2' as ZIconName} class="size-4" />
        </button>
      </div>
      <form class="min-w-0 flex-1" onsubmit={(event) => { event.preventDefault(); void go(); }}>
        <input
          bind:this={addressInput}
          bind:value={address}
          class="h-7 w-full rounded-md border border-border bg-muted/40 px-3 text-xs outline-none transition-colors placeholder:text-muted-foreground focus:border-primary focus:bg-background"
          placeholder="Search or enter an address"
          aria-label="Address"
          spellcheck="false"
          autocomplete="off"
          onfocus={(event) => { editing = true; (event.currentTarget as HTMLInputElement).select(); }}
          onblur={() => (editing = false)}
        />
      </form>
      <div class="flex shrink-0 items-center gap-0.5">
        <!-- Desktop, tablet, phone: one group, the current one pressed. Rotate is part of it, off for desktop. -->
        <div class="mr-1 items-center rounded-md border border-border p-px {compact ? 'hidden' : 'flex'}" role="radiogroup" aria-label="Device">
          {#each DEVICE_BUTTONS as item (item.value)}
            <button
              type="button"
              role="radio"
              aria-checked={browser.device === item.value}
              aria-label={item.label}
              class="grid h-6 w-7 cursor-pointer place-items-center rounded-[5px] outline-none {browser.device === item.value ? 'bg-primary/15 text-primary' : 'text-muted-foreground hover:bg-primary/10 hover:text-primary'}"
              {@attach zTooltip({ content: item.label, position: 'top' })}
              onclick={() => (browser.device = item.value)}
            >
              <ZIcon zType={item.icon} class="size-3.5" />
            </button>
          {/each}
          <span class="mx-0.5 h-4 w-px bg-border" aria-hidden="true"></span>
          <button
              type="button"
              aria-label="Rotate"
              aria-pressed={browser.landscape}
              disabled={browser.device === 'desktop'}
              class="grid h-6 w-7 cursor-pointer place-items-center rounded-[5px] outline-none disabled:pointer-events-none disabled:opacity-40 {browser.landscape && browser.device !== 'desktop' ? 'bg-primary/15 text-primary' : 'text-muted-foreground hover:bg-primary/10 hover:text-primary'}"
              {@attach zTooltip({ content: 'Rotate', position: 'top' })}
              onclick={() => (browser.landscape = !browser.landscape)}
            >
              <ZIcon zType={'lucideRotateCw' as ZIconName} class="size-3.5" />
            </button>
        </div>
        <button type="button" class="{icon} relative {browser.devtools ? 'bg-primary/15 text-primary' : ''}" aria-label="Developer tools" aria-pressed={browser.devtools} {@attach zTooltip({ content: 'Developer tools', position: 'top' })} onclick={() => (browser.devtools = !browser.devtools)}>
          <ZIcon zType={'lucideSquareTerminal' as ZIconName} class="size-4" />
          {#if errors > 0}<span class="absolute -top-0.5 -right-0.5 min-w-3.5 rounded-full bg-destructive px-1 text-[9px] leading-3.5 text-white">{errors}</span>{/if}
        </button>
<div data-page-menu class="relative flex items-center [&_svg]:size-3.5! [&_button]:size-7! [&_button]:rounded-md [&_button]:p-0 [&_button]:!bg-transparent [&_button]:!text-muted-foreground [&_button:hover]:!bg-primary/10 [&_button:hover]:!text-primary [&_button[aria-expanded=true]]:!bg-primary/15 [&_button[aria-expanded=true]]:!text-primary [&_button]:focus-visible:ring-0" {@attach zTooltip({ content: 'Running in the background', position: 'top' })}>
          <ZDropdownMenu zItems={processItems} zIcon={'lucideServer' as ZIconName} zLabel="" zButtonType="ghost" zButtonSize="sm" zPosition="bottom-right" zOffset={6} zMinWidth={260} zWave={false} />
          {#if alive.length > 0}<span class="pointer-events-none absolute -top-0.5 -right-0.5 min-w-3.5 rounded-full bg-primary px-1 text-center text-[9px] leading-3.5 text-primary-foreground">{alive.length}</span>{/if}
        </div>
<div data-page-menu class="flex items-center [&_svg]:size-3.5! [&_button]:size-7! [&_button]:rounded-md [&_button]:p-0 [&_button]:!bg-transparent [&_button]:!text-muted-foreground [&_button:hover]:!bg-primary/10 [&_button:hover]:!text-primary [&_button[aria-expanded=true]]:!bg-primary/15 [&_button[aria-expanded=true]]:!text-primary [&_button]:focus-visible:ring-0">
                  <ZDropdownMenu zItems={menuItems} zIcon={'lucideSettings2' as ZIconName} zLabel="" zButtonType="ghost" zButtonSize="sm" zPosition="bottom-right" zOffset={6} zMinWidth={200} zWave={false} />
        </div>
      </div>
    </div>
    {#if browser.annotating}
      <!-- Flies in by transform, never by size: a growing bar would move the native page every frame and make it shake. -->
      <div class="flex shrink-0 items-center gap-1 border-b border-border px-2 py-1" role="toolbar" aria-label="Mark up tools" in:fly={{ y: -8, duration: 200, opacity: 0 }}>
        {#each MARK_TOOLS as item (item.tool)}
          <button
            type="button"
            class="{icon} {browser.annotateTool === item.tool ? 'bg-primary/15 text-primary' : ''}"
            aria-label={item.label}
            aria-pressed={browser.annotateTool === item.tool}
            {@attach zTooltip({ content: item.label, position: 'top' })}
            onclick={() => void useTool(item.tool)}
          >
            <ZIcon zType={item.icon} class="size-4" />
          </button>
        {/each}
        <span class="mx-0.5 h-4 w-px shrink-0 bg-border" aria-hidden="true"></span>
        <button type="button" class={icon} aria-label="Undo" disabled={!browser.annotateUndo} {@attach zTooltip({ content: 'Undo (Ctrl+Z)', position: 'top' })} onclick={() => void markHistory('undo')}>
          <ZIcon zType={'lucideUndo2' as ZIconName} class="size-4" />
        </button>
        <button type="button" class={icon} aria-label="Redo" disabled={!browser.annotateRedo} {@attach zTooltip({ content: 'Redo (Ctrl+Shift+Z)', position: 'top' })} onclick={() => void markHistory('redo')}>
          <ZIcon zType={'lucideRedo2' as ZIconName} class="size-4" />
        </button>
        <button type="button" class="grid size-7 shrink-0 cursor-pointer place-items-center rounded-md text-destructive outline-none hover:bg-destructive/10 hover:text-destructive disabled:pointer-events-none disabled:opacity-40" aria-label={browser.annotateSelected ? 'Delete the selected note' : 'Delete all notes'} disabled={browser.annotateCount === 0} {@attach zTooltip({ content: browser.annotateSelected ? 'Delete the selected note' : 'Delete all notes', position: 'top' })} onclick={() => void markHistory('remove')}>
          <ZIcon zType={'lucideTrash2' as ZIconName} class="size-4" />
        </button>
        <span class="ml-1 min-w-0 truncate text-[11px] text-muted-foreground">{MARK_TOOLS.find((item) => item.tool === browser.annotateTool)?.hint}</span>
        {#if browser.annotateCount > 0}
          <button
            type="button"
            class="ml-auto inline-flex h-7 shrink-0 cursor-pointer items-center gap-1.5 rounded-md bg-primary px-2.5 text-xs font-medium text-primary-foreground outline-none hover:bg-primary/90 disabled:opacity-60"
            disabled={busy !== ''}
            onclick={() => void addMarksToChat()}
          >
            <ZIcon zType={'lucidePlus' as ZIconName} class="size-3.5" />
            Add to chat
          </button>
        {/if}
      </div>
    {/if}

    {#if browser.error}
      <div class="shrink-0 border-b border-border px-3 py-1 text-xs text-destructive" role="status">{browser.error}</div>
    {/if}

    <!-- The page itself lives in a native webview placed over this area. -->
    <div class="relative min-h-0 flex-1 bg-muted/30">
      <div bind:this={stage} class="absolute inset-0"></div>
      {#if deviceBox}
        <!-- The device: a bezel around the page (the page itself is the native layer above) and its caption. -->
        <div
          class="pointer-events-none absolute border-[10px] border-black shadow-lg ring-1 ring-border {browser.device === 'mobile' ? 'rounded-2xl' : 'rounded-xl'}"
          style:left="{deviceBox.x - 10}px"
          style:top="{deviceBox.y - 10}px"
          style:width="{deviceBox.w + 20}px"
          style:height="{deviceBox.h + 20}px"
          aria-hidden="true"
        ></div>
        <div class="absolute flex items-center justify-center gap-2 text-[11px] text-muted-foreground" style:left="{deviceBox.x - 10}px" style:width="{deviceBox.w + 20}px" style:top="{deviceBox.y - 34}px">
          <span>{DEVICES[browser.device].name} · {browser.landscape ? `${DEVICES[browser.device].h}×${DEVICES[browser.device].w}` : `${DEVICES[browser.device].w}×${DEVICES[browser.device].h}`}{deviceBox.zoom < 1 ? ` · ${Math.round(deviceBox.zoom * 100)}%` : ''}</span>
          <button type="button" class="grid size-5 cursor-pointer place-items-center rounded hover:bg-accent hover:text-foreground" aria-label="Rotate" onclick={() => (browser.landscape = !browser.landscape)} {@attach zTooltip({ content: 'Rotate', position: 'top' })}>
            <ZIcon zType={'lucideRotateCw' as ZIconName} class="size-3" />
          </button>
        </div>
      {/if}

      {#if !started}
        <div class="absolute inset-0 grid place-items-center p-6 text-center text-xs text-muted-foreground">
          <div class="flex flex-col items-center gap-2">
            <ZIcon zType="lucideGlobe" class="size-8 opacity-60" />
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
                      </div>
                      <div class="truncate font-mono text-[10px]" title={target.command}>{target.command}</div>
                    </div>
                    <button
                      type="button"
                      class="grid size-7 shrink-0 cursor-pointer place-items-center rounded-md outline-none hover:bg-accent hover:text-foreground disabled:pointer-events-none disabled:opacity-50"
                      aria-label="Run {target.name}"
                      disabled={starting !== ''}
                      {@attach zTooltip({ content: starting === key ? 'Starting…' : 'Run and open in the Preview', position: 'top' })}
                      onclick={() => void runProject(target)}
                    >
                      {#if starting === key}<ZLoading zLoading zSpinner="spinner" zSize="xs" zColor="primary" />{:else}<ZIcon zType={'lucidePlay' as ZIconName} class="size-4" />{/if}
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

    {#if browser.devtools}
      <DevTools />
    {/if}
  {/if}
</div>
