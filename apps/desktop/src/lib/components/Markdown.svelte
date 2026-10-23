<script lang="ts">
  import { untrack } from 'svelte';
  import { isLocalUrl, linkMenu, openExternal, openInPreview } from '../browser.svelte';
  import { inTauri } from '../history';
  import { renderMarkdown } from '../markdown';
  import { theme } from '../theme.svelte';
  import { fileRootOf, openFileTab } from '../content-tabs.svelte';
  import { activeSession, workspace } from '../sessions.svelte';

  // An answer as rich text: headings, lists, tables, quotes, links and code cards with a copy button.
  // Code is coloured once the answer has stopped streaming, so nothing flickers while text arrives.
  let { text, streaming = false }: { text: string; streaming?: boolean } = $props();

  // Text arrives in bursts. It is revealed at a steady, time-based pace instead (faster when more is waiting),
  // whole words at a time, and each new word fades in, as in Claude and ChatGPT. A finished answer that was
  // never streamed shows at once.
  const MIN_CPS = 40;
  const DRAIN_SECONDS = 0.6;
  const FADE_MS = 320;
  const STAGGER_MS = 18;

  let live = untrack(() => streaming);
  let shown = $state(untrack(() => (streaming ? 0 : text.length)));
  const catching = $derived(shown < text.length);
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
    if (!live || reducedMotion) {
      shown = target;
      return;
    }
    let progress = untrack(() => shown);
    if (progress > target) shown = progress = target;
    let last = performance.now();
    let frame = 0;
    const step = (now: number) => {
      const dt = Math.min(64, now - last) / 1000;
      last = now;
      const backlog = target - progress;
      if (backlog <= 0) return;
      progress = Math.min(target, progress + Math.max(MIN_CPS, backlog / DRAIN_SECONDS) * dt);
      const next = wordEnd(source, Math.floor(progress), target);
      if (next > shown) shown = next;
      progress = Math.max(progress, shown);
      frame = requestAnimationFrame(step);
    };
    frame = requestAnimationFrame(step);
    return () => cancelAnimationFrame(frame);
  });

  const html = $derived(renderMarkdown(text.slice(0, shown)));
  let root: HTMLDivElement | undefined = $state();

  // The whole answer is re-rendered as HTML on each step, so the fade is kept by offset in its visible text:
  // a word seen before keeps its start time (a negative delay resumes its fade), a new word starts now.
  const fadeStarts = new Map<number, number>();
  let seen = 0;

  function fadeTail(container: HTMLElement) {
    const now = performance.now();
    for (const [at, start] of fadeStarts) if (now - start > FADE_MS) fadeStarts.delete(at);
    const from = Math.min(seen, ...fadeStarts.keys());
    const walker = document.createTreeWalker(container, NodeFilter.SHOW_TEXT);
    const tail: [Text, number][] = [];
    let offset = 0;
    for (let node = walker.nextNode() as Text | null; node; node = walker.nextNode() as Text | null) {
      // Svelte owns the top-level nodes of {@html}; only text inside elements is split. Code stays plain.
      const parent = node.parentElement;
      if (offset + node.data.length > from && parent && parent !== container && !parent.closest('pre')) tail.push([node, offset]);
      offset += node.data.length;
    }
    let fresh = 0;
    for (const [node, base] of tail) {
      const out = document.createDocumentFragment();
      let at = base;
      for (const piece of node.data.split(/(\s+)/)) {
        if (!piece) continue;
        if (at + piece.length <= from || /^\s+$/.test(piece)) {
          out.append(piece);
        } else {
          let start = fadeStarts.get(at);
          if (start === undefined) fadeStarts.set(at, (start = now + Math.min(fresh++, 12) * STAGGER_MS));
          const word = document.createElement('span');
          word.className = 'md-fade';
          word.style.animationDelay = `${Math.round(start - now)}ms`;
          word.textContent = piece;
          out.append(word);
        }
        at += piece.length;
      }
      node.replaceWith(out);
    }
    seen = offset;
  }

  $effect(() => {
    void html;
    if (!root || !live || reducedMotion) return;
    const container = root;
    untrack(() => fadeTail(container));
  });

  async function colour(container: HTMLElement, dark: boolean) {
    const blocks = [...container.querySelectorAll<HTMLElement>('.md-code:not([data-coloured])')];
    if (blocks.length === 0) return;
    const { codeToHtml } = await import('shiki');
    for (const block of blocks) {
      const code = block.querySelector('code');
      const pre = block.querySelector('pre');
      if (!code || !pre) continue;
      block.dataset.coloured = '';
      const language = block.dataset.lang || 'text';
      try {
        const out = document.createElement('div');
        out.innerHTML = await codeToHtml(code.textContent ?? '', { lang: language, theme: dark ? 'github-dark' : 'github-light' });
        const coloured = out.querySelector('code');
        if (coloured && container.contains(block)) code.innerHTML = coloured.innerHTML;
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
    untrack(() => void colour(container, dark));
  });

  const linkOf = (event: Event) => (event.target as HTMLElement).closest<HTMLAnchorElement>('a[href]');

  /** A link the agent wrote to a file: `E:/repo/a.txt`, `/home/me/a.txt`, `file:///…`, or `src/a.ts:42` inside the
   *  project. Answers the file's absolute path, or null for a web address. */
  function filePathOf(href: string, projectRoot: string): string | null {
    const raw = decodeURI(href.replace(/^file:\/\/\/?/i, '')).replace(/#.*$/, '');
    // `path:12` or `path:12:3` points at a line; the viewer opens the file.
    const path = raw.replace(/:\d+(:\d+)?$/, '');
    if (/^[a-z]:[\\/]/i.test(path) || (path.startsWith('/') && !path.startsWith('//'))) return path;
    if (projectRoot && path && !/^[a-z][a-z0-9+.-]*:/i.test(path) && !path.startsWith('#')) return `${projectRoot}/${path.replace(/^\.\//, '')}`;
    return null;
  }

  // The viewer reads only inside a root folder: the project when the file is in it, else the file's own folder.
  function openFile(path: string, projectRoot: string) {
    openFileTab(path, fileRootOf(path, projectRoot));
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
<div bind:this={root} class="md" {onclick} {oncontextmenu}>{@html html}</div>

<style>
  /* A word that has just arrived: fades and sharpens into place. */
  .md :global(.md-fade) {
    animation: md-fade 320ms cubic-bezier(0.25, 0.1, 0.25, 1) both;
  }
  @keyframes md-fade {
    from {
      opacity: 0;
      filter: blur(3px);
    }
    to {
      opacity: 1;
      filter: blur(0);
    }
  }
</style>
