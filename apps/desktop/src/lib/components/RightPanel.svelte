<script lang="ts">
  import { onMount } from 'svelte';
  import { ZButton, ZDropdownMenu, ZIcon } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import ToolbarActions from './ToolbarActions.svelte';

  const tabs = [
    { value: 'Preview', label: 'Preview', icon: 'lucideGlobe' as ZIconName },
    { value: 'Diff', label: 'Diff', icon: 'lucideGitCompare' as ZIconName },
    { value: 'Workflow', label: 'Workflow', icon: 'lucideWorkflow' as ZIconName },
    { value: 'Usage', label: 'Usage', icon: 'lucideActivity' as ZIconName },
    { value: 'Audit', label: 'Audit', icon: 'lucideShieldCheck' as ZIconName },
  ];
  let active = $state('Preview');
  const current = $derived(tabs.find((tab) => tab.value === active) ?? tabs[0]);

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
    if (headerWidth === 0) return tabs.length;
    const room = headerWidth - ACTIONS - 6; // contentRect already excludes the pr-33 padding
    const fit = Math.floor((room + 2) / TAB);
    return fit >= tabs.length ? tabs.length : Math.max(0, fit - 1);
  });
  const visibleTabs = $derived(tabs.slice(0, visibleCount));
  const hiddenTabs = $derived(tabs.slice(visibleCount));

  // Standard z-ui dropdown: icon + text rows, check on the current tool, library focus handling and animation.
  const menuItems = $derived(
    hiddenTabs.map((tab) => ({
      label: tab.label,
      icon: tab.icon,
      checked: tab.value === active,
      onClick: () => (active = tab.value),
    })),
  );
</script>

<aside class="flex h-full min-w-0 flex-col bg-sidebar" aria-label="Tools panel">
  <!-- Header row: icon tabs (+ hamburger when cramped), then palette / mode / collapse after a short divider. -->
  <div bind:this={header} class="flex h-10 shrink-0 items-center overflow-hidden border-b border-border pr-33" data-tauri-drag-region>
    <div class="flex items-center gap-0.5 pl-1.5" role="tablist" aria-label="Workspace tools">
      {#each visibleTabs as tab (tab.value)}
        <ZButton
          zWave={false}
          role="tab"
          zType={active === tab.value ? 'subtle' : 'ghost'}
          zSize="sm"
          class="h-7 w-8 shrink-0 px-0"
          aria-selected={active === tab.value}
          aria-label={tab.label}
          title={tab.label}
          onclick={() => (active = tab.value)}
        >
          <ZIcon zType={tab.icon} class="size-4" />
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

  <div class="flex flex-1 flex-col items-center justify-center gap-3 p-6 text-center text-muted-foreground" role="tabpanel" aria-label={current.label}>
    <ZIcon zType={current.icon} class="size-8 opacity-60" />
    <h2 class="text-sm font-medium text-foreground">{current.label}</h2>
    <p class="m-0 max-w-56 text-xs leading-normal">
      {current.value === 'Preview' ? 'Your browser preview will appear here.' : `No ${current.label.toLowerCase()} data yet.`}
    </p>
  </div>
</aside>
