<script lang="ts">
  import { untrack } from 'svelte';
  import { isLocalUrl, linkMenu, openExternal, openInPreview } from '../browser.svelte';
  import { inTauri } from '../history';
  import { renderMarkdown } from '../markdown';
  import { theme } from '../theme.svelte';

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

  // Right click on a link: open it in the AgentDeck browser, the system browser, or copy it.
  function oncontextmenu(event: MouseEvent) {
    const link = linkOf(event);
    if (!link || !inTauri()) return;
    event.preventDefault();
    Object.assign(linkMenu, { open: true, url: link.href, x: event.clientX, y: event.clientY });
  }

  function onclick(event: MouseEvent) {
    // A link to a local dev server opens in the AgentDeck browser; any other address in the system browser.
    const link = linkOf(event);
    if (link && inTauri()) {
      event.preventDefault();
      if (isLocalUrl(link.href)) openInPreview(link.href);
      else void openExternal(link.href);
      return;
    }
    const button = (event.target as HTMLElement).closest<HTMLButtonElement>('[data-copy]');
    const code = button?.closest('.md-code')?.querySelector('code');
    if (!button || !code) return;
    void navigator.clipboard.writeText(code.textContent ?? '').then(() => {
      button.textContent = 'Copied';
      setTimeout(() => (button.textContent = 'Copy'), 1400);
    });
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions (the copy buttons inside are real buttons) -->
<div bind:this={root} class="md" {onclick} {oncontextmenu}>{@html html}</div>
