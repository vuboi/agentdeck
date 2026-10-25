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
    return () => stopShortcuts?.();
  });
</script>

<!-- No browser menu (Reload, Inspect, ...): the app's own menus listen on their elements and open themselves. F12 still opens the devtools. -->
<svelte:window oncontextmenu={(event) => event.preventDefault()} />

{@render children()}
{#if !isTrayMenu}
  <TitleTooltip />
  <!-- In-app notices use the standard toast style; see toast() in lib/toast.ts. -->
  <ZToaster zVariant="default" zPosition="top-center" />
{/if}
