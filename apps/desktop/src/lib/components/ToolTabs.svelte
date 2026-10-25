<script lang="ts">
  import { ZButton, ZDropdownMenu, ZIcon } from '@shival99/z-ui-svelte';
  import { layout, setToolsOpen } from '../layout.svelte';
  import { TOOLS } from '../tools';

  // Tabs that do not fit fold into a hamburger dropdown instead of squeezing the title.
  const TAB = 30; // 28px button + 2px gap

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

<div class="flex shrink-0 items-center gap-0.5" role="tablist" aria-label="Workspace tools">
  {#each visibleTabs as tool (tool.value)}
    <ZButton
      zWave={false}
      role="tab"
      zType="ghost"
      zSize="sm"
      class="size-7! shrink-0 p-0 {layout.activeTool === tool.value ? 'bg-primary/15 text-primary hover:bg-primary/20' : '!bg-transparent !text-muted-foreground hover:!bg-primary/10 hover:!text-primary'}"
      aria-selected={layout.activeTool === tool.value}
      aria-label={tool.label}
      title={tool.label}
      onclick={() => select(tool.value)}
    >
      <ZIcon zType={tool.icon} class="size-4" />
    </ZButton>
  {/each}
  {#if hiddenTabs.length > 0}
    <div class="flex items-center [&_button]:size-7! [&_button]:p-0 [&_button]:!bg-transparent [&_button]:!text-muted-foreground [&_button:hover]:!bg-primary/10 [&_button:hover]:!text-primary [&_button[aria-expanded=true]]:!bg-primary/15 [&_button[aria-expanded=true]]:!text-primary [&_button]:outline-none [&_button]:focus-visible:ring-0">
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
