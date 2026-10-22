<script lang="ts">
  import '../app.css';
  // Importing the store applies the saved theme before the first paint of the app shell.
  import '../lib/theme.svelte';

  import { onMount } from 'svelte';
  import TitleTooltip from '../lib/components/TitleTooltip.svelte';
  import { importHistory } from '../lib/sessions.svelte';
  import { warmIcons } from '../lib/icons';

  let { children } = $props();

  // In the desktop app, replace the mock workspace with the Claude Code / Codex history on this machine.
  onMount(() => {
    warmIcons();
    void importHistory();
  });
</script>

{@render children()}
<TitleTooltip />
