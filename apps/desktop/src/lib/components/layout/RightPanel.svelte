<script lang="ts">
  import { windowDrag } from '../../app/platform/window-drag';
  import { ZButton, ZIcon } from '@shival99/z-ui-svelte';
  import { detachTools, dockTool, isAway, layout } from '../../app/ui/layout.svelte';
  import { TOOLS, toolOf } from '../../chat/content/tools';
  import DockZone from '../ui/DockZone.svelte';
  import Splitter from '../ui/Splitter.svelte';
  import ToolTabs from '../ui/ToolTabs.svelte';
  import ToolView from '../chat/ToolView.svelte';

  const current = $derived(toolOf(layout.activeTool));
  // The panel shows a tool that is here: when its tool goes to a window or another zone, the next one takes over.
  // ("Open elsewhere" stays only for the case where every tool has left.)
  $effect(() => {
    if (!isAway(layout.activeTool)) return;
    const next = TOOLS.find((tool) => !isAway(tool.value));
    if (next) layout.activeTool = next.value;
  });
  let tabsWidth = $state(0);
</script>

<aside class="relative flex h-full w-full min-w-0 flex-1 flex-col bg-sidebar" aria-label="Tools panel">
  <!-- Header row: the tool tabs, left-aligned; on the right, next to the window controls (which sit over the right
       edge), the button that tears the open tool out into its own window. -->
  <div class="flex h-10 shrink-0 items-center gap-1 border-b border-border pl-1.5 {layout.toolsOnLeft ? 'pr-1.5' : 'pr-[100px]'}" {@attach windowDrag}>
    <div class="flex min-w-0 flex-1 justify-start" bind:clientWidth={tabsWidth}>
      <ToolTabs room={tabsWidth ? tabsWidth - 4 : undefined} />
    </div>
    {#if !isAway(layout.activeTool)}
      <ZButton
        zWave={false}
        zType="ghost"
        zSize="sm"
        class="size-8! shrink-0 p-0 !bg-transparent !text-muted-foreground hover:!bg-primary/10 hover:!text-primary"
        aria-label="Open {current.label} in a window"
        title="Open {current.label} in a window"
        onclick={detachTools}
      >
        <ZIcon zType="lucideMaximize2" class="size-3.5" />
      </ZButton>
    {/if}
  </div>

  <div class="relative flex min-h-0 flex-1 flex-col">
  {#if isAway(layout.activeTool)}
    <!-- The tool lives in the modal right now; docking brings it back here. -->
    <div class="flex flex-1 flex-col items-center justify-center gap-3 p-6 text-center text-muted-foreground">
      <ZIcon zType="lucideAppWindow" class="size-8 opacity-60" />
      <p class="m-0 text-xs">{current.label} is open in a window or docked elsewhere.</p>
      <ZButton zWave={false} zType="outline" zSize="sm" class="h-7 gap-1.5 text-xs" onclick={() => dockTool(layout.activeTool)}>
        <ZIcon zType="lucideMinimize2" class="size-3.5" />
        Dock back
      </ZButton>
    </div>
  {:else}
    <ToolView tool={layout.activeTool} />
  {/if}
  </div>

  <!-- Tools dropped on the lower part of the panel sit under its tool, resized like the terminal. -->
  {#if layout.docked.toolsBottom.length > 0}
    <Splitter axis="y" label="Resize tools bottom zone" onDelta={(delta) => (layout.sizes.toolsBottom = Math.min(Math.max(layout.sizes.toolsBottom - delta, 96), window.innerHeight - 200))} />
    <div class="flex shrink-0" style:height="{layout.sizes.toolsBottom}px"><DockZone zone="toolsBottom" /></div>
  {/if}
</aside>
