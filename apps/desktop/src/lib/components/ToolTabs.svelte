<script lang="ts">
  import MenuButton from './MenuButton.svelte';
  import { ZButton, ZIcon } from '@shival99/z-ui-svelte';
  import { flip } from 'svelte/animate';
  import { scale } from 'svelte/transition';
  import { isAway, layout, setToolsOpen } from '../layout.svelte';
  import { TOOLS } from '../tools';

  // Tabs that do not fit fold into a hamburger dropdown instead of squeezing the title.
  const TAB = 32; // 28px button + 4px gap

  // Room (px) this group may use. Undefined until the header is measured: show every tab.
  let { room }: { room: number | undefined } = $props();

  // A tool open in its own window or docked elsewhere has no tab here: it is not in the panel.
  const panelTools = $derived(TOOLS.filter((tool) => !isAway(tool.value)));
  const visibleCount = $derived.by(() => {
    if (room === undefined) return panelTools.length;
    const fit = Math.floor(room / TAB);
    // Everything fits, or one slot goes to the hamburger.
    return fit >= panelTools.length ? panelTools.length : Math.max(0, fit - 1);
  });
  const visibleTabs = $derived(panelTools.slice(0, visibleCount));
  const hiddenTabs = $derived(panelTools.slice(visibleCount));

  // Toggle: the open tool's button closes the panel; any other button opens the panel on that tool.
  function select(value: string) {
    if (layout.toolsOpen && layout.activeTool === value) {
      setToolsOpen(false);
      return;
    }
    layout.activeTool = value;
    setToolsOpen(true);
  }

  // Standard z-ui dropdown: icon + text rows, check on the current tool, library focus handling and animation.
  const menuItems = $derived(
    hiddenTabs.map((tool) => ({
      label: tool.label,
      icon: tool.icon,
      checked: tool.value === layout.activeTool,
      onClick: () => select(tool.value),
    })),
  );
</script>

<div class="flex shrink-0 items-center gap-1" role="tablist" aria-label="Workspace tools">
  {#each visibleTabs as tool (tool.value)}
    <!-- A tab that leaves (its tool torn out or docked elsewhere) shrinks away; the others close the gap. -->
    <span class="flex" animate:flip={{ duration: 180 }} transition:scale={{ start: 0.4, duration: 160 }}>
    <ZButton
      zWave={false}
      role="tab"
      zType="ghost"
      zSize="sm"
      class="size-7! shrink-0 p-0 {layout.activeTool === tool.value ? '!bg-primary/15 !text-primary hover:!bg-primary/20 hover:!text-primary' : '!bg-transparent !text-muted-foreground hover:!bg-primary/10 hover:!text-primary'}"
      aria-selected={layout.activeTool === tool.value}
      aria-label={tool.label}
      title={tool.label}
      onclick={() => select(tool.value)}
    >
      <ZIcon zType={tool.icon} class="size-4" />
    </ZButton>
    </span>
  {/each}
  {#if hiddenTabs.length > 0}
    <div class="flex items-center [&_button]:size-7! [&_button]:p-0 [&_button]:!bg-transparent [&_button]:!text-muted-foreground [&_button:hover]:!bg-primary/10 [&_button:hover]:!text-primary [&_button[aria-expanded=true]]:!bg-primary/15 [&_button[aria-expanded=true]]:!text-primary [&_button]:outline-none [&_button]:focus-visible:ring-0">
    <MenuButton items={menuItems} icon="lucideMenu" label="More tools" width={200} />
    </div>
  {/if}
</div>
