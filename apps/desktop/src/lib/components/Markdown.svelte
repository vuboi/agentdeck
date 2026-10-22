<script lang="ts">
  import { untrack } from 'svelte';
  import { isLocalUrl, linkMenu, openExternal, openInPreview } from '../browser.svelte';
  import { inTauri } from '../history';
  import { renderMarkdown } from '../markdown';
  import { theme } from '../theme.svelte';
  import { fileRootOf, openFileTab } from '../content-tabs.svelte';
  import { toast } from '../toast';
  import { activeSession, workspace } from '../sessions.svelte';

  // An answer as rich text: headings, lists, tables, quotes, links and code cards with a copy button.
  // Code is coloured once the answer has stopped streaming, so nothing flickers while text arrives.
  let { text, streaming = false }: { text: string; streaming?: boolean } = $props();

  const html = $derived(renderMarkdown(text));
  let root: HTMLDivElement | undefined = $state();

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
    if (streaming || !root) return;
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
    void navigator.clipboard.writeText(code.textContent ?? '').then(() => toast.success('Copied'));
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions (the copy buttons inside are real buttons) -->
<div bind:this={root} class="md" {onclick} {oncontextmenu}>{@html html}</div>
