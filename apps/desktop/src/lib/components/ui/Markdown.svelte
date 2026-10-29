<script module lang="ts">
  import { preloadHighlighter as preload } from '../../files/highlight';
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

  // Which paths exist (path_links.rs), kept for the session: a row that mounts again is marked at once, with no second
  // look at the disk. A path the agent is about to create is looked at again after a while (CHECK_AGAIN_MS).
  type Found = { exists: boolean; dir: boolean; path: string; at: number };
  const found = new Map<string, Found>();
  const FOUND_MAX = 3000;
  const CHECK_AGAIN_MS = 15_000;
  // Pictures from the web as data URLs (image_fetch), or null when they could not be had.
  // Kept by total size, not by count: one picture can be 15 MB, and a data URL is a third larger than its bytes.
  const pictures = new Map<string, string | null>();
  const PICTURES_MAX_CHARS = 40_000_000;
  let picturesChars = 0;
  function rememberPicture(url: string, data: string | null) {
    pictures.set(url, data);
    picturesChars += data?.length ?? 0;
    // The oldest go first; the newest stays even when it alone is over the limit (it is on screen).
    for (const [key, value] of pictures) {
      if (picturesChars <= PICTURES_MAX_CHARS || pictures.size <= 1) break;
      pictures.delete(key);
      picturesChars -= value?.length ?? 0;
    }
  }
</script>

<script lang="ts">
  import { untrack } from 'svelte';
  import { highlightHtml } from '../../files/highlight';
  import { isLocalUrl, linkMenu, openExternal, openInPreview, terminalCommand } from '../../browser/browser.svelte';
  import { inTauri } from '../../app/platform/history';
  import { colourFinished, colourStarted, eagerColour, markdownBlocks, renderMarkdown } from '../../app/text/markdown';
  import { runInTerminal } from '../../terminal/terminal.svelte';
  import { theme } from '../../app/ui/theme.svelte';
  import { displayPath, fileRootOf, isAbsolutePath, resolveInProject } from '../../files/paths';
  import { normalizePath } from '../../files/path-links';
  import { openInViewer } from '../../files/file-viewer.svelte';
  import { activeSession, workspace } from '../../chat/sessions.svelte';

  // An answer as rich text: headings, lists, tables, quotes, links and code cards with a copy button.
  // Code is coloured once the answer has stopped streaming, so nothing flickers while text arrives.
  let { text, streaming = false, onrendered, flush = false }: { text: string; streaming?: boolean; onrendered?: () => void; /** Show the rest at once (a newer message came after this one). */ flush?: boolean } = $props();

  // Text arrives in bursts. It is revealed at a steady, time-based pace instead (faster when more is waiting),
  // whole words at a time, and each new word fades in, as in Claude and ChatGPT. A finished answer that was
  // never streamed shows at once.
  // The pace follows the model, as llm-ui's throttle does: the speed is how fast text has been arriving (`inflow`, an
  // average over the last second or two) plus a correction that keeps a small buffer of BUFFER_CHARS not yet shown,
  // closed over LAG_SECONDS. So a fast model reads fast and a slow one slow, a burst is spread over the time the next one
  // takes to come, and the reveal neither falls seconds behind nor runs dry and stops between chunks.
  // When the run is over there is nothing more to wait for: what is left goes out at backlog / LAG_SECONDS.
  // MIN_CPS keeps a long backlog from crawling, MAX_CPS (about a word a frame) keeps a burst from popping in as whole
  // lines, and the speed eases towards its target so it never jerks. A newer message shows the rest at once (`flush`).
  const LAG_SECONDS = 0.6;
  const BUFFER_CHARS = 12;
  const MIN_CPS = 60;
  const MAX_CPS = 360;
  const EASE_PER_SECOND = 6;
  /** How long the arrival speed remembers: a burst counts for about this long. */
  const INFLOW_SECONDS = 1.5;
  /** The most text held back while the model writes, in seconds of its speed. */
  const MAX_HOLD_SECONDS = 1.2;
  const FADE_MS = 220;
  const STAGGER_MS = 18;
  // Past FADE_LIMIT words in flight, new ones simply appear: fading dozens at once costs frames.
  const FADE_LIMIT = 80;

  let live = untrack(() => streaming);
  // What has arrived before this block was put on screen shows at once: coming back to a chat that is still writing must
  // not type its answer out again from the start. Only text that arrives from now on is revealed.
  let shown = $state(untrack(() => text.length));
  // The reveal speed, carried across text updates so it eases instead of restarting.
  let rate = 0;
  // How fast text has been arriving (characters a second), and when and how much arrived last.
  let inflow = 0;
  let arrived = untrack(() => text.length);
  let arrivedAt = performance.now();
  // The usual time between two pieces (seconds), and how many pieces came: the speed is only known from the second one.
  let gapAverage = 0;
  let pieces = 0;
  const catching = $derived(shown < text.length);
  // Where the even reveal really is (may sit inside a word): kept across text updates, so each new burst does not
  // restart it from the end of the last shown word.
  let pace = untrack(() => shown);
  const reducedMotion = typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;

  /** The end of the word that `at` falls in, so a word never shows half typed. */
  function wordEnd(source: string, at: number, limit: number) {
    let end = at;
    while (end < limit && end - at < 24 && !/\s/.test(source[end])) end++;
    return graphemeEnd(source, end, limit);
  }

  /** Never cut inside a character drawn as one: the second half of a surrogate pair, a combining accent, an emoji joined
   *  with a zero-width joiner or a variation selector would otherwise show broken for a frame. */
  function graphemeEnd(source: string, at: number, limit: number) {
    let end = at;
    while (end < limit) {
      const code = source.charCodeAt(end);
      const joined = end > 0 && source.charCodeAt(end - 1) === 0x200d;
      if ((code >= 0xdc00 && code <= 0xdfff) || joined || /[‍︎️\p{M}]/u.test(source[end])) end++;
      else break;
    }
    return end;
  }

  /** A link `[name](path)` is never shown half typed: its raw text (with the whole path) would show and then shrink to
   *  the name. When `at` falls inside one, the reveal waits before it, or takes the whole link at once when it has arrived. */
  function linkSafe(source: string, at: number, limit: number) {
    const open = source.lastIndexOf('[', at - 1);
    if (open < 0 || at - open > 800) return at;
    const head = source.slice(open, at);
    if (!/^\[[^\]\n]*$|^\[[^\]\n]*\]$|^\[[^\]\n]*\]\([^)\n]*$/.test(head)) return at;
    const whole = /^\[[^\]\n]*\]\([^)\n]*\)/.exec(source.slice(open, limit));
    return whole ? open + whole[0].length : open;
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
    // A new piece arrived: fold its speed into `inflow`, weighted by how long it took to come.
    const arrival = performance.now();
    if (!streaming) inflow = 0;
    else if (target > arrived) {
      const gap = Math.max(0.016, (arrival - arrivedAt) / 1000);
      // The first piece after the block appears says nothing about speed (it may have waited for the first token).
      if (pieces++ > 0) {
        const speed = Math.min(MAX_CPS * 2, (target - arrived) / gap);
        const weight = Math.min(1, gap / INFLOW_SECONDS);
        inflow = inflow === 0 ? speed : inflow + (speed - inflow) * weight;
        gapAverage = gapAverage === 0 ? gap : gapAverage + (gap - gapAverage) * Math.max(0.2, weight);
      }
      arrivedAt = arrival;
    }
    arrived = target;
    let last = performance.now();
    let frame = 0;
    const step = (now: number) => {
      const dt = Math.min(64, now - last) / 1000;
      last = now;
      const backlog = target - progress;
      if (backlog <= 0) {
        rate = 0;
        return;
      }
      // While the model still writes, keep in hand what it sends between two pieces (at least BUFFER_CHARS, at most
      // MAX_HOLD_SECONDS of it) and go at its speed, so the reveal never runs dry and waits; once it is done, empty it.
      const buffer = streaming ? Math.max(BUFFER_CHARS, Math.min(inflow * gapAverage * 1.2, inflow * MAX_HOLD_SECONDS)) : 0;
      const ideal = (streaming ? inflow : 0) + (backlog - buffer) / LAG_SECONDS;
      // The floor only for a real backlog: near the buffer it would run dry and stop until the next piece comes.
      const wanted = Math.min(MAX_CPS, Math.max(backlog > buffer * 2 ? MIN_CPS : 15, ideal));
      rate = rate === 0 ? wanted : rate + (wanted - rate) * Math.min(1, dt * EASE_PER_SECOND);
      progress = pace = Math.min(target, progress + rate * dt);
      const next = linkSafe(source, wordEnd(source, Math.floor(progress), target), target);
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

  const NOT_FOUND = 'Không tìm thấy';

  function mark(link: HTMLAnchorElement, result: Found | undefined) {
    link.classList.toggle('md-path-ok', Boolean(result?.exists));
    link.classList.toggle('md-path-missing', result !== undefined && !result.exists);
    // A folder gets a folder icon, and a click opens it in the file manager.
    link.classList.toggle('md-path-dir', Boolean(result?.exists && result.dir));
    if (result && !result.exists) link.title = NOT_FOUND;
    else if (link.title === NOT_FOUND) link.removeAttribute('title');
  }

  /** Marks the links to files: one that exists is bold, one that does not is faded with a tooltip (no error). A path
   *  found in plain text comes with several guesses of where it ends (a name with spaces): the longest that exists wins,
   *  and the words after it go back to plain text. */
  async function checkPaths(container: HTMLElement) {
    const links = [...container.querySelectorAll<HTMLAnchorElement>('a[data-file]')].filter((link) => link.dataset.checked !== '1');
    if (!links.length) return;
    const projectRoot = workspace.projects.find((project) => project.id === activeSession()?.projectId)?.path ?? '';
    const guessesOf = (link: HTMLAnchorElement): [string, number, string][] => {
      try {
        const parsed = link.dataset.cands ? (JSON.parse(link.dataset.cands) as [string, number, string][]) : [];
        if (parsed.length) return parsed.map(([path, length, line]) => [resolveInProject(path, projectRoot), length, line]);
      } catch {
        // Falls to the single path below.
      }
      const file = filePathOf(link.dataset.file ?? '', projectRoot);
      return file ? [[file, 0, '']] : [];
    };
    const now = Date.now();
    const asked = new Set<string>();
    for (const link of links) {
      for (const [path] of guessesOf(link)) {
        const known = found.get(path);
        if (!known || (!known.exists && now - known.at > CHECK_AGAIN_MS)) asked.add(path);
      }
    }
    if (asked.size && inTauri()) {
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        const list = [...asked];
        const results = await invoke<{ exists: boolean; dir: boolean; path: string }[]>('path_resolve', { paths: list });
        results.forEach((result, index) => found.set(list[index], { ...result, at: Date.now() }));
        while (found.size > FOUND_MAX) found.delete(found.keys().next().value as string);
      } catch {
        return;
      }
    }
    for (const link of links) {
      if (!container.contains(link)) continue;
      const guesses = guessesOf(link);
      const hit = guesses.find(([path]) => found.get(path)?.exists);
      const pick = hit ?? guesses[guesses.length - 1];
      if (!pick) continue;
      const result = found.get(pick[0]);
      if (link.dataset.cands) {
        const rest = link.nextElementSibling;
        // The label is the file's own name (with the line it points at), never the path.
        const fullPath = result?.exists ? result.path : pick[0];
        link.textContent = `${displayPath(projectRoot, fullPath)}${pick[2]}`;
        link.dataset.file = `${fullPath}${pick[2]}`;
        link.title = fullPath;
        if (rest?.classList.contains('md-path-rest')) rest.textContent = (link.dataset.raw ?? '').slice(pick[1]);
      } else if (result?.exists) {
        const raw = link.dataset.file ?? '';
        const resolved = filePathOf(raw, projectRoot);
        if (!isAbsolutePath(raw)) link.dataset.file = result.path;
        if (resolved && link.textContent?.trim() === raw) link.textContent = displayPath(projectRoot, result.path);
      }
      mark(link, result);
      // A path that is not there yet stays open to a later look (the agent may be about to write it).
      if (result?.exists) link.dataset.checked = '1';
    }
  }

  /** A picture from the web (`data-remote`): fetched by the app, shown from its copy; when it cannot be had, a
   *  placeholder with a button that opens the address in the Preview. */
  async function loadPictures(container: HTMLElement) {
    const images = [...container.querySelectorAll<HTMLImageElement>('img[data-remote]:not([data-loading])')];
    for (const image of images) {
      const url = image.dataset.remote ?? '';
      image.dataset.loading = '1';
      let data = pictures.get(url);
      if (data === undefined) {
        try {
          const { invoke } = await import('@tauri-apps/api/core');
          data = await invoke<string>('image_fetch', { url });
        } catch {
          data = null;
        }
        rememberPicture(url, data);
      }
      if (!container.contains(image)) continue;
      if (data) {
        image.src = data;
        image.removeAttribute('data-remote');
        continue;
      }
      const holder = document.createElement('span');
      holder.className = 'md-image-missing';
      const label = document.createElement('span');
      label.textContent = image.alt || 'Picture not loaded';
      const open = document.createElement('a');
      open.href = url;
      open.textContent = 'Open in Preview';
      open.dataset.previewImage = '1';
      holder.append(label, open);
      image.replaceWith(holder);
    }
  }

  // After each render (and whenever the text settles): mark the file links and fetch the pictures. Blocks that were
  // already marked are skipped (`data-checked`), so only the block being written is looked at again.
  $effect(() => {
    void html;
    const container = root;
    if (!container || !inTauri()) return;
    const id = setTimeout(() => {
      void checkPaths(container);
      void loadPictures(container);
    }, streaming || catching ? 400 : 0);
    return () => clearTimeout(id);
  });

  const linkOf = (event: Event) => (event.target as HTMLElement).closest<HTMLAnchorElement>('a[href]');

  /** A link the agent wrote to a file: `E:/repo/a.txt`, `/home/me/a.txt`, `file:///…`, or `src/a.ts:42` inside the
   *  project. Answers the file's absolute path, or null for a web address. */
  function filePathOf(href: string, projectRoot: string): string | null {
    const source = /^file:\/\//i.test(href) ? href.replace(/#.*$/, '') : href;
    const raw = normalizePath(source);
    // `path:12` or `path:12:3` points at a line; the viewer opens the file.
    const path = raw.replace(/^\/([a-z]:[\\/])/i, '$1').replace(/:\d+(:\d+)?$/, '');
    if (isAbsolutePath(path) || /^~[\\/]/.test(path)) return path;
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
      if (link.classList.contains('md-path-dir') && link.dataset.file) {
        void import('@tauri-apps/api/core').then(({ invoke }) => invoke('path_open', { path: link.dataset.file }));
        return;
      }
      if (link.dataset.previewImage) {
        openInPreview(link.href);
        return;
      }
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
