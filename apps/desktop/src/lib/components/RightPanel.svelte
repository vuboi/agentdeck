<script lang="ts">
  import { ZButton, ZIcon } from '@shival99/z-ui-svelte';
  import { detachTools, dockTool, isAway, layout } from '../layout.svelte';
  import { toolOf } from '../tools';
  import ToolTabs from './ToolTabs.svelte';
  import ToolView from './ToolView.svelte';

  const current = $derived(toolOf(layout.activeTool));
  let tabsWidth = $state(0);
</script>

<aside class="relative flex h-full min-w-0 flex-col bg-sidebar" aria-label="Tools panel">
  <!-- Header row: the tool tabs only, left-aligned; the window controls sit over the right edge. -->
  <div class="flex h-10 shrink-0 items-center border-b border-border pr-33 pl-1.5" data-tauri-drag-region="deep">
    <div class="flex min-w-0 flex-1 justify-start" bind:clientWidth={tabsWidth}>
      <ToolTabs room={tabsWidth ? tabsWidth - 4 : undefined} />
    </div>
  </div>

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
    <ZButton
      zWave={false}
      zType="ghost"
      zSize="sm"
      class="absolute right-2 bottom-2 h-7 w-7"
      aria-label="Open {current.label} in a window"
      title="Open in a window"
      onclick={detachTools}
    >
      <ZIcon zType="lucideMaximize2" class="size-3.5" />
    </ZButton>
  {/if}
</aside>
