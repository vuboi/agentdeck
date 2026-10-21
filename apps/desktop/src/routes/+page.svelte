<script lang="ts">
  import { untrack } from 'svelte';
  import { ZResizable } from '@shival99/z-ui-svelte';
  import WindowControls from '../lib/components/WindowControls.svelte';
  import ThemePanel from '../lib/components/ThemePanel.svelte';
  import Sidebar from '../lib/components/Sidebar.svelte';
  import Chat from '../lib/components/Chat.svelte';
  import Terminal from '../lib/components/Terminal.svelte';
  import RightPanel from '../lib/components/RightPanel.svelte';
  import { layout } from '../lib/layout.svelte';
  import { terminal, handleTerminalShortcut } from '../lib/terminal.svelte';

  // Outer columns: sidebar | workspace | tools. The tools pane folds to 0 when closed.
  let columns = $state([18, 60, 22]);
  const columnPanes = [
    { id: 'sidebar', size: 18, min: 12, max: 30 },
    { id: 'workspace', min: 30 },
    { id: 'tools', size: 22, min: 14, max: 40, collapsible: true },
  ];

  // Vertical split inside the workspace: chat | terminal.
  let rows = $state([70, 30]);
  const rowPanes = [
    { id: 'chat', min: 25 },
    { id: 'terminal', min: 10, max: 70, collapsible: true },
  ];

  // Size changes are animated only when toggled, so dragging stays instant.
  let animating = $state(false);
  let animTimer: ReturnType<typeof setTimeout> | undefined;
  function animate() {
    animating = true;
    clearTimeout(animTimer);
    animTimer = setTimeout(() => (animating = false), 320);
  }
  const animClass = $derived(animating ? 'transition-[flex-basis] duration-300 ease-out' : '');

  // The divider is the handle itself: 1px wide, with a wider invisible hit area.
  // Its inner line is hidden so the border is a single crisp pixel row, not an offset anti-aliased one.
  const colHandle = "w-px bg-border [&>*]:hidden after:absolute after:inset-y-0 after:-left-1 after:w-[9px] after:content-['']";
  // Collapsed tools pane: no divider and no hit area, so nothing is left at the right edge.
  const colHandleClosed = "w-0 [&>*]:hidden";
  const rowHandle = "h-px bg-border [&>*]:hidden after:absolute after:inset-x-0 after:-top-1 after:h-[9px] after:content-['']";

  $effect(() => {
    const open = terminal.open;
    untrack(() => {
      animate();
      rows = open ? [70, 30] : [100, 0];
    });
  });

  $effect(() => {
    const open = layout.toolsOpen;
    untrack(() => {
      animate();
      columns = open ? [columns[0], 100 - columns[0] - 22, 22] : [columns[0], 100 - columns[0], 0];
    });
  });
</script>

<svelte:head><title>AgentDeck</title><meta name="description" content="Your multi-agent desktop workspace" /></svelte:head>
<svelte:window onkeydown={(event) => handleTerminalShortcut(event)} />

<div class="relative h-screen overflow-hidden bg-background text-foreground">
  <ZResizable
    zPanes={columnPanes}
    zHandle="line"
    zPersistKey="agentdeck.columns"
    zHandleClass={layout.toolsOpen ? colHandle : colHandleClosed}
    zPaneClass={animClass}
    bind:sizes={columns}
  >
    {#snippet pane(config, index)}
      {#if index === 0}
        <Sidebar />
      {:else if index === 1}
        <div class="flex h-full min-w-0 flex-col">
          <ZResizable zPanes={rowPanes} zDirection="vertical" zHandle="line" zHandleClass={rowHandle} zPaneClass={animClass} bind:sizes={rows}>
            {#snippet pane(rowConfig, rowIndex)}
              {#if rowIndex === 0}
                <Chat />
              {:else}
                <Terminal />
              {/if}
            {/snippet}
          </ZResizable>
        </div>
      {:else if layout.toolsOpen}
        <RightPanel />
      {/if}
    {/snippet}
  </ZResizable>

  <ThemePanel />
  <WindowControls />
</div>
