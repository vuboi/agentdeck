<script lang="ts">
  import { onMount } from 'svelte';
  import { ZButton, ZDropdownMenu, ZIcon } from '@shival99/z-ui-svelte';
  import { detachTools, dockTools, layout } from '../layout.svelte';
  import { TOOLS, toolOf } from '../tools';
  import ToolbarActions from './ToolbarActions.svelte';
  import ToolView from './ToolView.svelte';

  const current = $derived(toolOf(layout.activeTool));

  // Tabs that do not fit fold into a hamburger dropdown instead of scrolling sideways.
  const ACTIONS = 80; // short divider + palette + collapse
  const TAB = 34; // 32px button + 2px gap
  let headerWidth = $state(0);
  let header: HTMLDivElement | undefined = $state();

  onMount(() => {
    if (!header) return;
    const observer = new ResizeObserver(([entry]) => (headerWidth = entry.contentRect.width));
    observer.observe(header);
    return () => observer.disconnect();
  });

  const visibleCount = $derived.by(() => {
    if (headerWidth === 0) return TOOLS.length;
    const room = headerWidth - ACTIONS - 6; // contentRect already excludes the pr-33 padding
    const fit = Math.floor((room + 2) / TAB);
    return fit >= TOOLS.length ? TOOLS.length : Math.max(0, fit - 1);
  });
  const visibleTabs = $derived(TOOLS.slice(0, visibleCount));
  const hiddenTabs = $derived(TOOLS.slice(visibleCount));

  // Standard z-ui dropdown: icon + text rows, check on the current tool, library focus handling and animation.
  const menuItems = $derived(
    hiddenTabs.map((tool) => ({
      label: tool.label,
      icon: tool.icon,
      checked: tool.value === layout.activeTool,
      onClick: () => (layout.activeTool = tool.value),
    })),
  );
</script>

<aside class="relative flex h-full min-w-0 flex-col bg-sidebar" aria-label="Tools panel">
  <!-- Header row: icon tabs (+ hamburger when cramped), then palette / collapse after a short divider. -->
  <div bind:this={header} class="flex h-10 shrink-0 items-center overflow-hidden border-b border-border pr-33" data-tauri-drag-region>
    <div class="flex items-center gap-0.5 pl-1.5" role="tablist" aria-label="Workspace tools">
      {#each visibleTabs as tool (tool.value)}
        <ZButton
          zWave={false}
          role="tab"
          zType={layout.activeTool === tool.value ? 'subtle' : 'ghost'}
          zSize="sm"
          class="h-7 w-8 shrink-0 px-0"
          aria-selected={layout.activeTool === tool.value}
          aria-label={tool.label}
          title={tool.label}
          onclick={() => (layout.activeTool = tool.value)}
        >
          <ZIcon zType={tool.icon} class="size-4" />
        </ZButton>
      {/each}
      {#if hiddenTabs.length > 0}
        <ZDropdownMenu
          zItems={menuItems}
          zIcon="lucideMenu"
          zLabel=""
          zButtonType="ghost"
          zButtonSize="sm"
          zPosition="bottom-right"
          zOffset={6}
          zMinWidth={128}
          zWave={false}
        />
      {/if}
    </div>
    <ToolbarActions />
  </div>

  {#if layout.toolsDetached}
    <!-- The tool lives in the modal right now; docking brings it back here. -->
    <div class="flex flex-1 flex-col items-center justify-center gap-3 p-6 text-center text-muted-foreground">
      <ZIcon zType="lucideAppWindow" class="size-8 opacity-60" />
      <p class="m-0 text-xs">{current.label} is open in a window.</p>
      <ZButton zWave={false} zType="outline" zSize="sm" class="h-7 gap-1.5 text-xs" onclick={dockTools}>
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
