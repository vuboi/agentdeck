<script lang="ts">
  import { onDestroy, onMount, tick, untrack } from 'svelte';
  import { ZButton, ZIcon, zTooltip } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import {
    back,
    browser,
    cancelPick,
    DEVICES,
    forward,
    goTo,
    hideBrowser,
    listenToPage,
    moveBrowser,
    pickElement,
    reload,
    screenshot,
    showBrowser,
    snapshot,
    zoomBrowser,
    type Device,
    type Rect,
  } from '../browser.svelte';
  import { inTauri } from '../history';
  import { draftImages, drafts, workspace } from '../sessions.svelte';
  import AnnotationEditor from './AnnotationEditor.svelte';

  // The AgentDeck browser: any page (a local dev server, a site) in a real webview, with device sizes, an
  // element picker, a console and network log, and a freeze-frame annotation view whose result goes to the chat.
  let stage: HTMLDivElement | undefined = $state();
  let address = $state(browser.url);
  let started = $state(browser.opened);
  let shot = $state<{ data: string; w: number; h: number } | null>(null);
  let drawer = $state<'console' | 'network' | null>(null);
  let busy = $state('');
  let notice = $state('');
  let covered = $state(false);

  const DEVICE_ICON: Record<Device, ZIconName> = {
    desktop: 'lucideMonitor' as ZIconName,
    tablet: 'lucideTablet' as ZIconName,
    mobile: 'lucideSmartphone' as ZIconName,
  };
  const errors = $derived(browser.console.filter((line) => line.level === 'error').length);

  /** The webview's rectangle in window coordinates: the stage itself, or a phone / tablet frame centred in it. */
  function frame(): { rect: Rect; zoom: number } | null {
    if (!stage) return null;
    const box = stage.getBoundingClientRect();
    if (box.width < 20 || box.height < 20) return null;
    const device = DEVICES[browser.device];
    if (!device.w) return { rect: { x: box.x, y: box.y, w: box.width, h: box.height }, zoom: 1 };
    // A narrower page is a zoomed-out one: the page lays out at the device width, the panel just shows it smaller.
    const zoom = Math.min(1, (box.width - 16) / device.w, (box.height - 16) / device.h);
    const w = device.w * zoom;
    const h = device.h * zoom;
    return { rect: { x: box.x + (box.width - w) / 2, y: box.y + (box.height - h) / 2, w, h }, zoom };
  }

  async function sync() {
    if (!inTauri() || !started || shot || covered) return;
    const place = frame();
    if (!place) return;
    try {
      if (browser.opened) await moveBrowser(place.rect);
      else await showBrowser(place.rect);
      await zoomBrowser(place.zoom);
    } catch (error) {
      browser.error = String(error);
    }
  }

  function normalize(input: string): string {
    const text = input.trim();
    if (!text) return '';
    if (/^[a-z][a-z0-9+.-]*:\/\//i.test(text)) return text;
    return /^(localhost|127\.|\[::1\])/.test(text) ? `http://${text}` : `https://${text}`;
  }

  async function go() {
    const url = normalize(address);
    if (!url) return;
    browser.error = '';
    address = url;
    // The address is set before the page opens, so the first open never uses a stale one.
    browser.url = url;
    started = true;
    if (browser.opened) await goTo(url);
    else await sync();
  }

  // The page is a native layer above the app: a menu or dialog opened over it would hide behind it.
  const OVERLAYS = '[data-z-popover], [role="dialog"]:not([aria-label="Annotate screenshot"]), [data-z-modal]';
  let observer: MutationObserver | undefined;
  let resizer: ResizeObserver | undefined;

  onMount(() => {
    void listenToPage();
    if (stage) {
      resizer = new ResizeObserver(() => void sync());
      resizer.observe(stage);
    }
    observer = new MutationObserver(() => {
      const now = document.querySelector(OVERLAYS) !== null;
      if (now === covered) return;
      covered = now;
      if (now) void hideBrowser();
    });
    observer.observe(document.body, { childList: true, subtree: true });
    void sync();
    window.addEventListener('resize', onResize);
  });
  const onResize = () => void sync();

  onDestroy(() => {
    resizer?.disconnect();
    observer?.disconnect();
    window.removeEventListener('resize', onResize);
    if (inTauri()) void hideBrowser();
  });

  // Opened by the developer hook while the view is already on screen.
  $effect(() => {
    if (!browser.autostart) return;
    browser.autostart = false;
    address = browser.url;
    started = true;
    // Already showing a page: go to the new one.
    if (browser.opened) void goTo(browser.url);
  });

  $effect(() => {
    void browser.device;
    void covered;
    void started;
    untrack(() => void sync());
  });

  // ---- pick, annotate, send ----
  const clip = (text: string, max: number) => (text.length > max ? `${text.slice(0, max)}…` : text);

  /** What the agent needs to know about the page, written as text for the composer. */
  async function pageContext(notes: { n: number; text: string; x: number; y: number }[] = []): Promise<string> {
    let title = browser.title;
    let viewport = '';
    let body = '';
    try {
      const snap = await snapshot();
      title = snap.title || title;
      viewport = `${snap.viewport.w}×${snap.viewport.h}`;
      body = clip(snap.html.slice(Math.max(0, snap.html.indexOf('<body'))), 2500);
    } catch {
      // The page may not answer (a blank page); the rest of the context is still useful.
    }
    const lines = [`Browser: ${title || 'page'} — ${browser.url} (${browser.device}${viewport ? ` ${viewport}` : ''})`];
    if (notes.length) lines.push('Notes (numbered on the screenshot):', ...notes.map((note) => `${note.n}. ${note.text} (at ${note.x},${note.y})`));
    const picked = browser.picked;
    if (picked) lines.push(`Selected element: \`${picked.selector}\` (${picked.box.w}×${picked.box.h})`, '```html', clip(picked.html, 2500), '```');
    else if (body) lines.push('Page HTML (start of body):', '```html', body, '```');
    const recent = browser.console.filter((line) => line.level === 'error').slice(-5);
    if (recent.length) lines.push('Console errors:', ...recent.map((line) => `- ${clip(line.text, 300)}`));
    return lines.join('\n');
  }

  function addToDraft(text: string, image?: string) {
    const id = workspace.activeId;
    drafts[id] = [drafts[id], text].filter(Boolean).join('\n\n');
    if (image) draftImages[id] = [...(draftImages[id] ?? []), { id: crypto.randomUUID(), name: 'browser-annotation.png', mime: 'image/png', src: image }];
    notice = 'Added to the message box.';
    setTimeout(() => (notice = ''), 2500);
  }

  async function pick() {
    if (browser.picking) {
      void cancelPick();
      return;
    }
    browser.error = '';
    try {
      await pickElement();
    } catch (error) {
      browser.error = String(error);
    }
  }

  async function annotate() {
    busy = 'Capturing the page…';
    browser.error = '';
    try {
      const taken = await screenshot();
      await hideBrowser();
      shot = taken;
    } catch (error) {
      browser.error = String(error);
    } finally {
      busy = '';
    }
  }

  async function annotated(png: string, notes: { n: number; text: string; x: number; y: number }[]) {
    shot = null;
    addToDraft(await pageContext(notes), png);
    await tick();
    void sync();
  }

  async function cancelAnnotation() {
    shot = null;
    await tick();
    void sync();
  }

  function toggleDrawer(kind: 'console' | 'network') {
    drawer = drawer === kind ? null : kind;
  }

  const levelTone = (level: string) => (level === 'error' ? 'text-destructive' : level === 'warn' ? 'text-warning' : 'text-muted-foreground');
  const statusTone = (status: number) => (status === 0 || status >= 400 ? 'text-destructive' : 'text-success');
</script>

<div class="flex h-full min-h-0 flex-1 flex-col" aria-label="Browser">
  {#if !inTauri()}
    <div class="grid flex-1 place-items-center p-6 text-center text-xs text-muted-foreground">The browser runs in the AgentDeck desktop app.</div>
  {:else}
    <!-- Address bar -->
    <form class="flex shrink-0 items-center gap-1 border-b border-border px-2 py-1.5" onsubmit={(event) => { event.preventDefault(); void go(); }}>
      <ZButton zWave={false} zType="ghost" zSize="sm" class="size-7! p-0 text-muted-foreground" aria-label="Back" title="Back" onclick={() => void back()}>
        <ZIcon zType={'lucideArrowLeft' as ZIconName} class="size-4" />
      </ZButton>
      <ZButton zWave={false} zType="ghost" zSize="sm" class="size-7! p-0 text-muted-foreground" aria-label="Forward" title="Forward" onclick={() => void forward()}>
        <ZIcon zType={'lucideArrowRight' as ZIconName} class="size-4" />
      </ZButton>
      <ZButton zWave={false} zType="ghost" zSize="sm" class="size-7! p-0 text-muted-foreground" aria-label="Reload" title="Reload" onclick={() => void reload()}>
        <ZIcon zType={'lucideRotateCw' as ZIconName} class="size-4" />
      </ZButton>
      <input
        bind:value={address}
        class="h-7 min-w-0 flex-1 rounded-md border border-input bg-background px-2.5 text-xs outline-none focus:border-primary"
        placeholder="Enter a URL, e.g. localhost:4200"
        aria-label="Address"
        spellcheck="false"
        autocomplete="off"
      />
    </form>

    <!-- Tools -->
    <div class="flex shrink-0 flex-wrap items-center gap-1 border-b border-border px-2 py-1">
      {#each Object.entries(DEVICES) as [value, device] (value)}
        <ZButton
          zWave={false}
          zType="ghost"
          zSize="sm"
          class="size-7! p-0 {browser.device === value ? 'bg-primary/15 text-primary' : 'text-muted-foreground'}"
          aria-label={device.label}
          aria-pressed={browser.device === value}
          {@attach zTooltip({ content: device.w ? `${device.label} ${device.w}×${device.h}` : device.label, position: 'top' })}
          onclick={() => (browser.device = value as Device)}
        >
          <ZIcon zType={DEVICE_ICON[value as Device]} class="size-4" />
        </ZButton>
      {/each}
      <span class="mx-1 h-4 w-px bg-border" aria-hidden="true"></span>
      <ZButton zWave={false} zType="ghost" zSize="sm" class="size-7! p-0 {browser.picking ? 'bg-primary/15 text-primary' : 'text-muted-foreground'}" aria-label="Pick an element" aria-pressed={browser.picking} disabled={!started} onclick={pick} {@attach zTooltip({ content: 'Pick an element', position: 'top' })}>
        <ZIcon zType={'lucideMousePointerClick' as ZIconName} class="size-4" />
      </ZButton>
      <ZButton zWave={false} zType="ghost" zSize="sm" class="size-7! p-0 text-muted-foreground" aria-label="Annotate" disabled={!started || busy !== ''} onclick={annotate} {@attach zTooltip({ content: 'Annotate: mark up a screenshot for the agent', position: 'top' })}>
        <ZIcon zType={'lucideHighlighter' as ZIconName} class="size-4" />
      </ZButton>
      <div class="ml-auto flex items-center gap-1">
        <ZButton zWave={false} zType="ghost" zSize="sm" class="relative size-7! p-0 {drawer === 'console' ? 'bg-primary/15 text-primary' : 'text-muted-foreground'}" aria-label="Console" aria-pressed={drawer === 'console'} onclick={() => toggleDrawer('console')} {@attach zTooltip({ content: 'Console', position: 'top' })}>
          <ZIcon zType={'lucideSquareTerminal' as ZIconName} class="size-4" />
          {#if errors > 0}<span class="absolute -top-0.5 -right-0.5 min-w-3.5 rounded-full bg-destructive px-1 text-[9px] leading-3.5 text-white">{errors}</span>{/if}
        </ZButton>
        <ZButton zWave={false} zType="ghost" zSize="sm" class="size-7! p-0 {drawer === 'network' ? 'bg-primary/15 text-primary' : 'text-muted-foreground'}" aria-label="Network" aria-pressed={drawer === 'network'} onclick={() => toggleDrawer('network')} {@attach zTooltip({ content: 'Network', position: 'top' })}>
          <ZIcon zType={'lucideNetwork' as ZIconName} class="size-4" />
        </ZButton>
      </div>
    </div>

    {#if browser.picked}
      <div class="flex shrink-0 items-center gap-2 border-b border-border bg-primary/5 px-2 py-1 text-xs">
        <ZIcon zType={'lucideMousePointerClick' as ZIconName} class="size-3.5 shrink-0 text-primary" />
        <code class="min-w-0 flex-1 truncate" title={browser.picked.selector}>{browser.picked.selector}</code>
        <ZButton zWave={false} zType="outline" zSize="sm" class="h-6 px-2 text-xs" onclick={async () => addToDraft(await pageContext())}>Add to chat</ZButton>
        <button type="button" class="grid size-5 cursor-pointer place-items-center rounded text-muted-foreground hover:bg-accent" aria-label="Clear selection" onclick={() => (browser.picked = null)}>
          <ZIcon zType="lucideX" class="size-3" />
        </button>
      </div>
    {/if}
    {#if browser.error || notice || busy}
      <div class="shrink-0 border-b border-border px-3 py-1 text-xs {browser.error ? 'text-destructive' : 'text-muted-foreground'}" role="status">{browser.error || busy || notice}</div>
    {/if}

    <!-- The page itself lives in a native webview placed over this area. -->
    <div class="relative min-h-0 flex-1 bg-muted/30">
      <div bind:this={stage} class="absolute inset-0"></div>
      {#if !started}
        <div class="absolute inset-0 grid place-items-center p-6 text-center text-xs text-muted-foreground">
          <div class="flex flex-col items-center gap-2">
            <ZIcon zType="lucideGlobe" class="size-8 opacity-60" />
            <p class="m-0">Enter a URL above to open a page here.</p>
            <p class="m-0 max-w-60">Dev servers on localhost work best. You can pick elements, annotate, and send it all to the chat.</p>
          </div>
        </div>
      {/if}
      {#if shot}
        <div class="absolute inset-0 z-10 bg-background">
          <AnnotationEditor {shot} oncancel={cancelAnnotation} ondone={annotated} />
        </div>
      {/if}
    </div>

    {#if drawer}
      <div class="h-40 shrink-0 overflow-auto border-t border-border bg-background font-mono text-[11px]" role="log" aria-label={drawer}>
        {#if drawer === 'console'}
          {#each browser.console as line, index (index)}
            <div class="flex gap-2 border-b border-border/50 px-2 py-0.5 {levelTone(line.level)}"><span class="shrink-0 uppercase">{line.level}</span><span class="min-w-0 break-all whitespace-pre-wrap text-foreground">{line.text}</span></div>
          {:else}
            <div class="p-3 text-muted-foreground">Nothing logged yet.</div>
          {/each}
        {:else}
          {#each browser.network as line, index (index)}
            <div class="flex gap-2 border-b border-border/50 px-2 py-0.5"><span class="w-9 shrink-0 text-muted-foreground">{line.method}</span><span class="w-8 shrink-0 {statusTone(line.status)}">{line.status || 'ERR'}</span><span class="min-w-0 flex-1 truncate" title={line.url}>{line.url}</span><span class="shrink-0 text-muted-foreground">{line.ms}ms</span></div>
          {:else}
            <div class="p-3 text-muted-foreground">No requests yet.</div>
          {/each}
        {/if}
      </div>
    {/if}
  {/if}
</div>
