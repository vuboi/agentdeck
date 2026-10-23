<script lang="ts">
  import '../app.css';
  // Importing the store applies the saved theme before the first paint of the app shell.
  import '../lib/theme.svelte';

  import { onMount } from 'svelte';
  import TitleTooltip from '../lib/components/TitleTooltip.svelte';
  import { ZToaster } from '@shival99/z-ui-svelte';
  import { importHistory } from '../lib/sessions.svelte';
  import { warmIcons } from '../lib/icons';
  import { installTabShortcuts } from '../lib/content-tabs.svelte';
  import { listenToPage, resetBrowser } from '../lib/browser.svelte';

  // Before anything below mounts: an F5 reloads this window but not the pages it opened, which would stay on screen.
  void resetBrowser();

  let { children } = $props();

  // In the desktop app, replace the mock workspace with the Claude Code / Codex history on this machine.
  onMount(() => {
    warmIcons();
    void importHistory();
    // The agent opens pages and switches devices through events: listen from the start, not only once the Preview
    // panel has been shown (an event before that was lost, and browser_open timed out).
    void listenToPage();
    return installTabShortcuts();
  });
</script>

<!-- No browser menu (Reload, Inspect, ...): the app's own menus listen on their elements and open themselves. F12 still opens the devtools. -->
<svelte:window oncontextmenu={(event) => event.preventDefault()} />

{@render children()}
<TitleTooltip />
<!-- Small pill notes for copy, delete and the like: see toast() in lib/toast.ts. -->
<ZToaster zVariant="pill" zPosition="top-center" />
