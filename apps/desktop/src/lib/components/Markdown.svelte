<script module lang="ts">
  import { preloadHighlighter as preload } from '../highlight';
  // shiki and its grammars load in the highlighter's worker as soon as the app is idle after start (they took about
  // 0.4 s plus 0.5 s for the Svelte grammar on the first file opened); in the worker this costs the page nothing.
  if (typeof window !== 'undefined') {
    const idle = globalThis.requestIdleCallback ?? ((run: () => void) => setTimeout(run, 300));
    setTimeout(() => idle(preload, { timeout: 1500 }), 600);
  }

  // Coloured code by language, theme and text, shared by every answer: a row the virtual list mounts again gets its
  // colours at once, with no second pass through shiki (it was kept per component, so every remount coloured again:
  // profiled at hundreds of ms in the middle of a scroll). Very long blocks stay plain.
  const COLOUR_MAX_CHARS = 20_000;
  const coloured = new Map<string, string>();
  const COLOURED_MAX = 600;
</script>

<script lang="ts">
  import { untrack } from 'svelte';
  import { highlightHtml } from '../highlight';
  import { isLocalUrl, linkMenu, openExternal, openInPreview, terminalCommand } from '../browser.svelte';
  import { inTauri } from '../history';
  import { colourFinished, colourStarted, eagerColour, markdownBlocks, renderMarkdown } from '../markdown';
  import { runInTerminal } from '../terminal.svelte';
  import { theme } from '../theme.svelte';
  import { fileRootOf, isAbsolutePath, resolveInProject } from '../paths';
  import { openInViewer } from '../file-viewer.svelte';
  import { activeSession, workspace } from '../sessions.svelte';

  // An answer as rich text: headings, lists, tables, quotes, links and code cards with a copy button.
  // Code is coloured once the answer has stopped streaming, so nothing flickers while text arrives.
  let { text, streaming = false, onrendered, flush = false }: { text: string; streaming?: boolean; onrendered?: () => void; /** Show the rest at once (a newer message came after this one). */ flush?: boolean } = $props();

  // Text arrives in bursts. It is revealed at a steady, time-based pace instead (faster when more is waiting),
  // whole words at a time, and each new word fades in, as in Claude and ChatGPT. A finished answer that was
  // never streamed shows at once.
  // A long backlog is drained over a longer time and never faster than MAX_CPS (about one word per frame), so
  // words keep appearing one after another instead of a whole paragraph popping in at once.
  // While the agent still writes, a calm, even pace like Claude and Codex (a backlog only nudges it up). Some agents
  // (Codex) hand over the whole answer at once, already complete: it is still typed out at an even pace, a little
  // faster only for a long answer (about 8 s for what is left), so it never runs ahead in bursts.
  const MIN_CPS = 30;
  const MAX_CPS = 120;
  const DRAIN_SECONDS = 4;
  // Once the run is over, what is left is shown within about 3 s, at most about 10 characters a frame: quick enough
  // that a finished reply does not look like it is still running, slow enough that words still come one after another
  // instead of in chunks. A newer message shows the rest at once (`flush`).
  const DONE_MIN_CPS = 150;
  const DONE_MAX_CPS = 600;
  const DONE_DRAIN_SECONDS = 3;
  const FADE_MS = 220;
  const STAGGER_MS = 18;
  // Past FADE_LIMIT words in flight, new ones simply appear: fading dozens at once costs frames.
  const FADE_LIMIT = 80;

  let live = untrack(() => streaming);
  let shown = $state(untrack(() => (streaming ? 0 : text.length)));
  const catching = $derived(shown < text.length);
  // Where the even reveal really is (may sit inside a word): kept across text updates, so each new burst does not
  // restart it from the end of the last shown word.
  let pace = untrack(() => shown);
  const reducedMotion = typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;

  /** The end of the word that `at` falls in, so a word never shows half typed. */
  function wordEnd(source: string, at: number, limit: number) {
    let end = at;
    while (end < limit && end - at < 24 && !/\s/.test(source[end])) end++;
    return end;
  }

  $effect(() => {
    const source = text;
    const target = source.length;
    if (streaming) live = true;
    if (!live || reducedMotion || (flush && !streaming)) {
      shown = pace = target;
      return;
    }
    let progress = Math.min(pace, target);
    if (untrack(() => shown) > target) shown = target;
    let last = performance.now();
    let frame = 0;
    const step = (now: number) => {
      const dt = Math.min(64, now - last) / 1000;
      last = now;
      const backlog = target - progress;
      if (backlog <= 0) return;
      const rate = streaming ? Math.min(MAX_CPS, Math.max(MIN_CPS, backlog / DRAIN_SECONDS)) : Math.min(DONE_MAX_CPS, Math.max(DONE_MIN_CPS, backlog / DONE_DRAIN_SECONDS));
      progress = pace = Math.min(target, progress + rate * dt);
      const next = wordEnd(source, Math.floor(progress), target);
      // `progress` keeps its own pace: pulling it to the end of each shown word would double the real speed.
      if (next > shown) shown = next;
      frame = requestAnimationFrame(step);
    };
    frame = requestAnimationFrame(step);
    return () => cancelAnimationFrame(frame);
  });

  // The answer is drawn block by block (markdown.ts `markdownBlocks`). While it streams, the blocks already complete
  // keep their HTML string, so their DOM is left alone; only the block being written is parsed and replaced. Parsing
  // and replacing the whole answer on every frame made a long answer stutter as it grew.
  let lastRaw: string[] = [];
  let lastHtml: string[] = [];
  const html = $derived.by(() => {
    const raw = markdownBlocks(text.slice(0, shown), lastRaw);
    const growing = streaming || shown < text.length;
    const next = raw.map((block, index) => (lastRaw[index] === block ? lastHtml[index] : renderMarkdown(block, !(growing && index === raw.length - 1))));
    lastRaw = raw;
    lastHtml = next;
    return next;
  });
  let root: HTMLDivElement | undefined = $state();

  // Notify the chat only after the completed response has been revealed and this DOM update has landed.
  $effect(() => {
    if (!streaming && text.length > 0 && shown >= text.length) onrendered?.();
  });

  // Each step replaces the HTML of the blocks that changed, so the fade is kept by offset in a block's visible text: a
  // word seen before keeps its start time (a negative delay resumes its fade), a new word starts now.
  const fadeStarts = new Map<string, number>();
  let seen: number[] = [];
  let fadedHtml: string[] = [];

  function fadeTail(blocks: HTMLElement[], from: number) {
    const now = performance.now();
    for (const [key, start] of fadeStarts) if (now - start > FADE_MS) fadeStarts.delete(key);
    let fresh = 0;
    for (let index = from; index < blocks.length; index++) {
      const block = blocks[index];
      const before = seen[index] ?? 0;
      const walker = document.createTreeWalker(block, NodeFilter.SHOW_TEXT);
      const tail: [Text, number][] = [];
      let offset = 0;
      for (let node = walker.nextNode() as Text | null; node; node = walker.nextNode() as Text | null) {
        // Svelte owns the top-level nodes of {@html}; only text inside elements is split. Code stays plain.
        const parent = node.parentElement;
        if (parent && parent !== block && !parent.closest('pre')) tail.push([node, offset]);
        offset += node.data.length;
      }
      for (const [node, base] of tail) {
        const out = document.createDocumentFragment();
        let at = base;
        let split = false;
        for (const piece of node.data.split(/(\s+)/)) {
          if (!piece) continue;
          const key = `${index}:${at}`;
          const known = fadeStarts.get(key);
          // A word shown before with no fade in progress stays plain. (When the markdown of the earlier text changes,
          // offsets shift by a character or two; without this check those words would fade in again.)
          if (/^\s+$/.test(piece) || (known === undefined && at < before) || (known === undefined && fadeStarts.size >= FADE_LIMIT)) {
            out.append(piece);
          } else {
            let start = known;
            if (start === undefined) fadeStarts.set(key, (start = now + Math.min(fresh++, 6) * STAGGER_MS));
            const word = document.createElement('span');
            word.className = 'md-fade';
            word.style.animationDelay = `${Math.round(start - now)}ms`;
            word.textContent = piece;
            out.append(word);
            split = true;
          }
          at += piece.length;
        }
        if (split) node.replaceWith(out);
      }
      seen[index] = offset;
    }
    seen.length = blocks.length;
  }

  $effect(() => {
    const current = html;
    if (!root || !live || reducedMotion) return;
    const container = root;
    untrack(() => {
      // Only the blocks whose HTML changed since the last step are walked: the others still hold their fades.
      let from = 0;
      while (from < current.length && fadedHtml[from] === current[from]) from++;
      fadedHtml = current;
      const blocks = [...container.children].filter((child): child is HTMLElement => child instanceof HTMLElement && child.classList.contains('md-block'));
      fadeTail(blocks, from);
    });
  });


  async function colour(container: HTMLElement, dark: boolean) {
    // A block coloured for the other theme (the theme changed, or the app was still settling) is coloured again.
    const blocks = [...container.querySelectorAll<HTMLElement>(`.md-code:not([data-coloured="${dark ? 'dark' : 'light'}"])`)];
    if (blocks.length === 0) return;
    colourStarted();
    try {
      await colourBlocks(container, dark, blocks);
    } finally {
      colourFinished();
    }
  }

  async function colourBlocks(container: HTMLElement, dark: boolean, blocks: HTMLElement[]) {
    for (const block of blocks) {
      const code = block.querySelector('code');
      const pre = block.querySelector('pre');
      if (!code || !pre) continue;
      block.dataset.coloured = dark ? 'dark' : 'light';
      const source = code.textContent ?? '';
      if (source.length > COLOUR_MAX_CHARS) continue;
      const language = block.dataset.lang || 'text';
      const key = `${dark ? 'd' : 'l'}|${language}|${source}`;
      const hit = coloured.get(key);
      if (hit !== undefined) {
        code.innerHTML = hit;
        continue;
      }
      try {
        // shiki works in its own thread (highlight.ts): the page only puts the colours in.
        const inner = await highlightHtml(source, language, dark);
        if (inner === null) continue;
        coloured.set(key, inner);
        if (coloured.size > COLOURED_MAX) coloured.delete(coloured.keys().next().value as string);
        if (container.contains(block)) code.innerHTML = inner;
      } catch {
        // An unknown language stays plain.
      }
    }
  }

  $effect(() => {
    void html;
    const dark = theme.dark;
    if (streaming || catching || !root) return;
    const container = root;
    // Right away, not when the page is idle: a chat is kept hidden until it sits at its end, and colouring in that
    // window means code never shows plain first and then changes colour in front of the reader (a visible blink).
    // Colours already worked out come from the cache synchronously; shiki itself is loaded ahead (see above).
    if (eagerColour()) {
      untrack(() => void colour(container, dark));
      return;
    }
    const run = () => void colour(container, dark);
    if (typeof requestIdleCallback === 'function') {
      const id = requestIdleCallback(run, { timeout: 800 });
      return () => cancelIdleCallback(id);
    }
    const id = setTimeout(run, 50);
    return () => clearTimeout(id);
  });

  const linkOf = (event: Event) => (event.target as HTMLElement).closest<HTMLAnchorElement>('a[href]');

  /** A link the agent wrote to a file: `E:/repo/a.txt`, `/home/me/a.txt`, `file:///…`, or `src/a.ts:42` inside the
   *  project. Answers the file's absolute path, or null for a web address. */
  function filePathOf(href: string, projectRoot: string): string | null {
    const raw = decodeURI(href.replace(/^file:\/\/\/?/i, '')).replace(/#.*$/, '');
    // `path:12` or `path:12:3` points at a line; the viewer opens the file.
    const path = raw.replace(/^\/([a-z]:[\\/])/i, '$1').replace(/:\d+(:\d+)?$/, '');
    if (isAbsolutePath(path)) return path;
    if (projectRoot && path && !/^[a-z][a-z0-9+.-]*:/i.test(path) && !path.startsWith('#')) return resolveInProject(path, projectRoot);
    return null;
  }

  // The viewer reads only inside a root folder: the project when the file is in it, else the file's own folder.
  function openFile(path: string, projectRoot: string) {
    openInViewer(path, fileRootOf(path, projectRoot));
  }

  // Right click on a link: open it in the AgentDeck browser, the system browser, or copy it.
  function oncontextmenu(event: MouseEvent) {
    const link = linkOf(event);
    if (!link || !inTauri()) return;
    event.preventDefault();
    const projectRoot = workspace.projects.find((project) => project.id === activeSession()?.projectId)?.path ?? '';
    const file = filePathOf(link.dataset.file ?? link.getAttribute('href') ?? '', projectRoot);
    Object.assign(linkMenu, {
      open: true,
      url: link.href,
      x: event.clientX,
      y: event.clientY,
      file: file ?? '',
      root: file ? fileRootOf(file, projectRoot) : '',
    });
  }

  const copiedTimers = new WeakMap<HTMLElement, ReturnType<typeof setTimeout>>();

  function onclick(event: MouseEvent) {
    // A link to a local dev server opens in the AgentDeck browser; any other address in the system browser.
    const link = linkOf(event);
    if (link && inTauri()) {
      event.preventDefault();
      if (link.dataset.file === undefined && link.getAttribute('href') === '#') return;
      // A file the agent linked to opens in a tab of its own, like a file picked in Explore.
      const projectRoot = workspace.projects.find((project) => project.id === activeSession()?.projectId)?.path ?? '';
      const file = filePathOf(link.dataset.file ?? link.getAttribute('href') ?? '', projectRoot);
      if (file) openFile(file, projectRoot);
      else if (isLocalUrl(link.href)) openInPreview(link.href);
      else void openExternal(link.href);
      return;
    }
    const run = (event.target as HTMLElement).closest<HTMLButtonElement>('[data-run]');
    if (run) {
      const command = run.closest('.md-code')?.querySelector('code')?.textContent;
      // Run in the folder of the chat's project, not wherever that shell happens to be (often the home folder).
      const folder = workspace.projects.find((project) => project.id === activeSession()?.projectId)?.path ?? '';
      if (command?.trim()) runInTerminal(terminalCommand(command, folder));
      return;
    }
    const table = (event.target as HTMLElement).closest<HTMLButtonElement>('[data-copy-table]');
    if (table) {
      // Tab-separated rows: a paste into a spreadsheet or a document keeps the columns.
      const rows = [...(table.closest('.md-table')?.querySelectorAll('tr') ?? [])].map((row) =>
        [...row.children].map((cell) => (cell.textContent ?? '').trim()).join('\t'),
      );
      void navigator.clipboard.writeText(rows.join('\n')).then(() => {
        table.setAttribute('data-copied', '');
        clearTimeout(copiedTimers.get(table));
        copiedTimers.set(table, setTimeout(() => table.removeAttribute('data-copied'), 1800));
      });
      return;
    }
    const button = (event.target as HTMLElement).closest<HTMLButtonElement>('[data-copy]');
    const code = button?.closest('.md-code')?.querySelector('code');
    if (!button || !code) return;
    void navigator.clipboard.writeText(code.textContent ?? '').then(() => {
      button.setAttribute('data-copied', '');
      clearTimeout(copiedTimers.get(button));
      copiedTimers.set(button, setTimeout(() => button.removeAttribute('data-copied'), 1800));
    });
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions (the copy buttons inside are real buttons) -->
<div bind:this={root} class="md" {onclick} {oncontextmenu}>{#each html as block, index (index)}<div class="md-block">{@html block}</div>{/each}</div>

<style>
  /* A word that has just arrived fades in, quickly and without blur, as in Claude Code: fast text never smears. */
  .md :global(.md-fade) {
    animation: md-appear 220ms ease-out both;
  }
  @keyframes md-appear {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }
</style>
