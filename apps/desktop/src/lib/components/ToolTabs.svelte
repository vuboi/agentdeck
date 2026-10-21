<script lang="ts">
  import { ZButton, ZDropdownMenu, ZIcon } from '@shival99/z-ui-svelte';
  import { layout } from '../layout.svelte';
  import { TOOLS } from '../tools';

  // Tabs that do not fit fold into a hamburger dropdown instead of squeezing the title.
  const TAB = 34; // 32px button + 2px gap

  // Room (px) this group may use. Undefined until the header is measured: show every tab.
  let { room }: { room: number | undefined } = $props();

  const visibleCount = $derived.by(() => {
    if (room === undefined) return TOOLS.length;
    const fit = Math.floor(room / TAB);
    // Everything fits, or one slot goes to the hamburger.
    return fit >= TOOLS.length ? TOOLS.length : Math.max(0, fit - 1);
  });
  const visibleTabs = $derived(TOOLS.slice(0, visibleCount));
  const hiddenTabs = $derived(TOOLS.slice(visibleCount));

  // Toggle: the open tool's button closes the panel; any other button opens the panel on that tool.
  function select(value: string) {
    if (layout.toolsOpen && layout.activeTool === value) {
      layout.toolsOpen = false;
      return;
    }
    layout.activeTool = value;
    layout.toolsOpen = true;
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

<div class="flex shrink-0 items-center gap-0.5" role="tablist" aria-label="Workspace tools">
  {#each visibleTabs as tool (tool.value)}
    <ZButton
      zWave={false}
      role="tab"
      zType="ghost"
      zSize="sm"
      class="h-7 w-8 shrink-0 px-0 {layout.activeTool === tool.value ? 'bg-primary/15 text-primary hover:bg-primary/20' : '!bg-transparent !text-muted-foreground hover:!bg-primary/10 hover:!text-primary'}"
      aria-selected={layout.activeTool === tool.value}
      aria-label={tool.label}
      title={tool.label}
      onclick={() => select(tool.value)}
    >
      <ZIcon zType={tool.icon} class="size-4" />
    </ZButton>
  {/each}
  {#if hiddenTabs.length > 0}
    <div class="flex items-center [&_button]:h-7 [&_button]:w-8 [&_button]:bg-transparent [&_button]:text-muted-foreground [&_button:hover]:bg-primary/10 [&_button:hover]:text-primary">
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
    </div>
  {/if}
</div>
