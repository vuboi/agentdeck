<script lang="ts">
  import '../app.css';
  // Importing the store applies the saved theme before the first paint of the app shell.
  import '../lib/theme.svelte';

  import { onMount } from 'svelte';
  import TitleTooltip from '../lib/components/TitleTooltip.svelte';
  import { ZToaster } from '@shival99/z-ui-svelte';
  let { children } = $props();
  const isTrayMenu = typeof window !== 'undefined' && window.location.pathname.startsWith('/tray');

  // In the desktop app, replace the mock workspace with the Claude Code / Codex history on this machine.
  onMount(() => {
    if (isTrayMenu) return;
    const scrollArrow = (event: PointerEvent) => {
      const target = event.target;
      if (!(target instanceof HTMLElement)) return;
      const bar = target.closest<HTMLElement>('.os-theme-z.os-scrollbar');
      if (!bar || target !== bar) return;

      const vertical = bar.classList.contains('os-scrollbar-vertical');
      const rect = bar.getBoundingClientRect();
      const position = vertical ? event.clientY - rect.top : event.clientX - rect.left;
      const length = vertical ? rect.height : rect.width;
      if (position >= 12 && position <= length - 12) return;

      const viewport = bar.closest<HTMLElement>('[data-overlayscrollbars]')?.querySelector<HTMLElement>('[data-overlayscrollbars-viewport]');
      if (!viewport) return;
      const delta = position < 12 ? -48 : 48;
      event.preventDefault();
      viewport.scrollBy(vertical ? { top: delta } : { left: delta });
    };
    document.addEventListener('pointerdown', scrollArrow, true);

    void import('../lib/icons').then(({ warmIcons }) => warmIcons());
    void import('../lib/sessions.svelte').then(({ importHistory }) => void importHistory());
    void import('../lib/browser.svelte').then(({ listenToPage, resetBrowser }) => {
      // An F5 reloads this window but not the pages it opened, which would stay on screen.
      void resetBrowser();
      void listenToPage();
    });
    // The agent opens pages and switches devices through events: listen from the start, not only once the Preview
    // panel has been shown (an event before that was lost, and browser_open timed out).
    let stopShortcuts: (() => void) | undefined;
    void import('../lib/content-tabs.svelte').then(({ installTabShortcuts }) => (stopShortcuts = installTabShortcuts()));
    return () => {
      stopShortcuts?.();
      document.removeEventListener('pointerdown', scrollArrow, true);
    };
  });
</script>

<!-- No browser menu (Reload, Inspect, ...): the app's own menus listen on their elements and open themselves. F12 still opens the devtools. -->
<svelte:window oncontextmenu={(event) => event.preventDefault()} />

{@render children()}
{#if !isTrayMenu}
  <TitleTooltip />
  <!-- In-app notices use the standard toast style; see toast() in lib/toast.ts. -->
  <ZToaster zVariant="default" zExpand={false} zPosition="top-center" />
{/if}
