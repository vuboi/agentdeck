<script lang="ts">
  import type { Snippet } from 'svelte';
  import { ZButton, ZIcon, ZInput, zTooltip } from '@shival99/z-ui-svelte';
  import type { ZDropdownMenuItem, ZIconName } from '@shival99/z-ui-svelte';
  import MenuButton from './MenuButton.svelte';

  /** One icon button of the bar. When the bar gets narrow the last ones move, in order, into a menu. */
  export type BarAction = { id: string; label: string; icon: string; onclick: () => void; disabled?: boolean; busy?: boolean };

  // The sub-header of every Store tab, one height for all four: what is listed can be searched, then the tab's own
  // controls, then its icon-only actions (add, refresh, …). It is a size container: what sits in it can show its
  // labels only while there is room (`@[560px]:inline`), and as it narrows the actions fold into a menu one by one.
  let {
    query = $bindable(''),
    placeholder,
    lead,
    filters,
    actions = [],
  }: {
    query?: string;
    placeholder: string;
    /** Before the search box (the Installed tab's kind switch). */
    lead?: Snippet;
    /** Between the search box and the actions (Discover's two filters). */
    filters?: Snippet;
    actions?: BarAction[];
  } = $props();

  const SLOT = 44; // an icon button and its gap
  const SEARCH_MIN = 120;
  let width = $state(0);
  let leadWidth = $state(0);
  let filtersWidth = $state(0);

  // How many actions fit beside the search box. When not all of them do, the menu takes one slot itself.
  const room = $derived(Math.max(0, Math.floor((width - 24 - leadWidth - filtersWidth - SEARCH_MIN) / SLOT)));
  // The first action (add) always stays on the bar; the others fold into the menu from the last one.
  const shown = $derived(room >= actions.length ? actions.length : Math.min(actions.length, Math.max(1, room - 1)));
  const folded = $derived(actions.slice(shown));
  const menu = $derived<ZDropdownMenuItem[]>(
    folded.map((action) => ({ label: action.label, icon: action.icon as ZIconName, disabled: action.disabled || action.busy, onClick: action.onclick })),
  );
</script>

<div class="@container shrink-0 border-b border-border" bind:clientWidth={width}>
  <div class="flex h-14 items-center gap-2 px-3">
    {#if lead}<div class="flex shrink-0" bind:clientWidth={leadWidth}>{@render lead()}</div>{/if}
    <ZInput bind:value={query} zPlaceholder={placeholder} class="h-9 min-w-24 flex-1" />
    {#if filters}<div class="flex shrink-0 items-center gap-2" bind:clientWidth={filtersWidth}>{@render filters()}</div>{/if}
    {#each actions.slice(0, shown) as action (action.id)}
      <ZButton zWave={false} zType="outline" zSize="sm" class="size-9! shrink-0 p-0 text-muted-foreground" disabled={action.disabled || action.busy} aria-label={action.label} aria-busy={action.busy} {@attach zTooltip({ content: action.label, position: 'bottom' })} onclick={action.onclick}>
        <ZIcon zType={(action.busy ? 'lucideLoaderCircle' : action.icon) as ZIconName} class="size-4 {action.busy ? 'animate-spin' : ''}" />
      </ZButton>
    {/each}
    {#if folded.length > 0}
      <MenuButton items={menu} icon="lucideMenu" label="More" class="size-9!" />
    {/if}
  </div>
</div>
