<script lang="ts">
  import Titlebar from '../lib/components/Titlebar.svelte';
  import Sidebar from '../lib/components/Sidebar.svelte';
  import Chat from '../lib/components/Chat.svelte';
  import RightPanel from '../lib/components/RightPanel.svelte';
</script>

<svelte:head><title>AgentDeck</title><meta name="description" content="Your multi-agent desktop workspace" /></svelte:head>
<div class="app-shell"><Titlebar /><div class="workspace"><Sidebar /><Chat /><RightPanel /></div></div>
