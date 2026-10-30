<script lang="ts">
  import { tick } from 'svelte';
  import { ZIcon, ZLoading, zTooltip } from '@shival99/z-ui-svelte';
  import type { ZDropdownMenuItem, ZIconName } from '@shival99/z-ui-svelte';
  import { revealTab } from '../../app/ui/tab-strip';
  import { closeOnScroll } from '../../app/ui/close-on-scroll';
  import { toast } from '../../app/platform/toast';
  import { browser, closeTab, MAX_TABS, newTab, selectTab, tabsFull } from '../../browser/browser.svelte';
  import TabListButton from '../ui/TabListButton.svelte';
  import Menu from '../ui/Menu.svelte';
  import type { PickerItem } from '../ui/PickerPopover.svelte';

  // Right click on a tab: its menu (new, close, close the others, copy the address).
  let tabMenu = $state<{ x: number; y: number; id: string } | null>(null);
  let tabMenuOpen = $state(false);
  let tabMenuKey = $state(0);
  $effect(() => closeOnScroll(() => tabMenuOpen, () => (tabMenuOpen = false)));
  function openTabMenu(event: MouseEvent, id: string) {
    event.preventDefault();
    tabMenuKey++;
    tabMenu = { x: event.clientX, y: event.clientY, id };
    tabMenuOpen = true;
  }
  const menuTab = $derived(tabMenu ? browser.tabs.find((item) => item.id === tabMenu?.id) : undefined);
  const tabContextItems = $derived<ZDropdownMenuItem[]>([
    { label: 'New tab', icon: 'lucidePlus', iconSize: '12', class: 'text-sm! py-1', disabled: tabsFull(), onClick: () => addTab() },
    { label: 'Copy address', icon: 'lucideCopy', iconSize: '12', class: 'text-sm! py-1', divide: 'before', disabled: !menuTab?.url, onClick: () => menuTab?.url && void navigator.clipboard.writeText(menuTab.url) },
    { label: 'Close tab', icon: 'lucideX', iconSize: '12', shortcut: 'Ctrl+W', divide: 'before', class: 'text-sm! py-1', onClick: () => tabMenu && void closeTab(tabMenu.id) },
    { label: 'Close other tabs', icon: 'lucideX', iconSize: '12', class: 'text-sm! py-1', disabled: browser.tabs.length < 2, onClick: () => tabMenu && closeOtherTabs(tabMenu.id) },
  ]);
  function closeOtherTabs(id: string) {
    for (const item of browser.tabs) if (item.id !== id) void closeTab(item.id);
  }

  // Tabs: no scrollbar; when they do not fit an arrow lists them all, and the front one stays in view.
  let tabStrip: HTMLDivElement | undefined = $state();
  let tabStripWidth = $state(0);
  let tabsOverflow = $state(false);
  // Tabs cut off on the left: the strip is scrolled past its first tab.
  let tabScrolled = $state(false);
  $effect(() => {
    void browser.tabs.length;
    void tabStripWidth;
    void tick().then(() => {
      tabsOverflow = browser.tabs.length > 1 && !!tabStrip && tabStrip.scrollWidth > tabStrip.clientWidth + 1;
    });
  });
  $effect(() => {
    const id = browser.activeId;
    void browser.tabs.length;
    void tabStripWidth;
    void tick().then(() => {
      revealTab(tabStrip, id);
      tabScrolled = (tabStrip?.scrollLeft ?? 0) > 1;
    });
  });
  const tabTitle = (item: { title: string; url: string }) => item.title || (item.url ? item.url.replace(/^https?:\/\//, '') : 'New tab');
  const tabListItems = $derived<PickerItem<string>[]>(browser.tabs.map((item) => ({ id: item.id, label: tabTitle(item), hint: item.url ? item.url.replace(/^https?:\/\//, '') : undefined, icon: 'lucideGlobe' as ZIconName, selected: item.id === browser.activeId, data: item.id })));
  function addTab() {
    if (tabsFull()) {
      toast.info(`Tab limit reached (${MAX_TABS}). Close a tab to open another.`);
      return;
    }
    newTab();
  }
</script>

<!-- Tabs -->
<div class="flex h-10 shrink-0 items-center gap-1 border-b border-border px-1.5" role="tablist" aria-label="Browser tabs">
  <div class="relative flex min-w-0 shrink items-center">
  <div bind:this={tabStrip} bind:clientWidth={tabStripWidth} onscroll={() => (tabScrolled = (tabStrip?.scrollLeft ?? 0) > 1)} class="flex min-w-0 flex-1 items-center gap-1 overflow-hidden">
    {#each browser.tabs as item (item.id)}
      <div
        data-tab-id={item.id}
        role="tab"
        tabindex="0"
        aria-selected={item.id === browser.activeId}
        class="group/tab flex h-8 min-w-24 max-w-44 shrink-0 cursor-pointer items-center gap-1.5 rounded-md pr-1 pl-2 text-sm outline-none {item.id === browser.activeId ? 'bg-primary/15 text-primary' : 'text-muted-foreground hover:bg-primary/10 hover:text-primary'}"
        onclick={() => selectTab(item.id)}
        onkeydown={(event) => event.key === 'Enter' && selectTab(item.id)}
        oncontextmenu={(event) => openTabMenu(event, item.id)}
        title={tabTitle(item)}
      >
        <span class="grid size-3.5 shrink-0 place-items-center">
          {#if item.loading}
            <ZLoading zLoading zSpinner="spinner" zSize="2xs" zColor="current" />
          {:else if item.favicon}
            <img src={item.favicon} alt="" class="size-3.5 rounded-sm" onerror={(event) => ((event.currentTarget as HTMLElement).style.display = 'none')} />
          {:else}
            <ZIcon zType="lucideGlobe" class="size-3.5" />
          {/if}
        </span>
        <span class="min-w-0 flex-1 truncate">{tabTitle(item)}</span>
        <button type="button" class="grid size-4.5 shrink-0 cursor-pointer place-items-center rounded-full outline-none focus-visible:ring-0 hover:bg-primary/20" aria-label="Close tab" onclick={(event) => { event.stopPropagation(); void closeTab(item.id); }}>
          <ZIcon zType="lucideX" zSize="12" class="size-3" />
        </button>
      </div>
    {/each}
  </div>
  {#if tabsOverflow}
    <!-- Fade into the background, as the chat's tab bar does: a tab that is cut off does not end in a hard edge. -->
    <div class="pointer-events-none absolute inset-y-0 right-0 z-10 w-10 bg-gradient-to-r from-transparent to-background" aria-hidden="true"></div>
  {/if}
  {#if tabScrolled}
    <div class="pointer-events-none absolute inset-y-0 left-0 z-10 w-10 bg-gradient-to-r from-background to-transparent" aria-hidden="true"></div>
  {/if}
  </div>
  {#if tabsOverflow}
    <div class="flex shrink-0 items-center bg-background">
      <TabListButton items={tabListItems} onpick={selectTab} label="Open tabs" />
    </div>
  {/if}
  {#if tabMenu}
    {#key tabMenuKey}
      <Menu items={tabContextItems} bind:open={tabMenuOpen} at={{ x: tabMenu.x, y: tabMenu.y }} position="bottom-left" width={220} />
    {/key}
  {/if}
  <button type="button" class="grid size-8 shrink-0 cursor-pointer place-items-center rounded-md text-muted-foreground outline-none hover:bg-primary/10 hover:text-primary {tabsFull() ? 'opacity-40' : ''}" aria-label="New tab" aria-disabled={tabsFull()} {@attach zTooltip({ content: tabsFull() ? `Tab limit reached (${MAX_TABS})` : 'New tab', position: 'top' })} onclick={addTab}>
    <ZIcon zType="lucidePlus" class="size-4" />
  </button>
</div>
