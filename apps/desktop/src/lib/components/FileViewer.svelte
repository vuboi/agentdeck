<script lang="ts">
  import Menu from './Menu.svelte';
  import { ZDropdownMenu, ZIcon, zTooltip } from '@shival99/z-ui-svelte';
  import type { ZDropdownMenuItem, ZIconName } from '@shival99/z-ui-svelte';
  import { tick } from 'svelte';
  import { openFileTab, saveTab } from '../content-tabs.svelte';
  import { closeOnScroll } from '../close-on-scroll';
  import { openWithItem, openWithOptions, type OpenWithOption } from '../open-with';
  import { closeAllViewerTabs, closeOtherViewerTabs, closeViewerTab, viewer } from '../file-viewer.svelte';
  import ChangesView from './ChangesView.svelte';
  import FileExplorer from './FileExplorer.svelte';
  import FileView from './FileView.svelte';
  import Splitter from './Splitter.svelte';
  import TabListButton from './TabListButton.svelte';
  import type { PickerItem } from './PickerPopover.svelte';
  import { dirOf, relativeTo } from '../paths';

  // The Files tool: the project tree, its git changes and the open files in one place. Files open in tabs, as in the
  // Preview's tab bar: tabs that do not fit scroll, arrows appear on the side that has more, and both edges fade so a
  // cut tab never ends in a hard line. The tree (or the changes) sits beside the open file, or in its place when narrow.

  const icon = (name: string) => name as ZIconName;
  const active = $derived(viewer.tabs.find((tab) => tab.id === viewer.active) ?? viewer.tabs[0]);

  let strip: HTMLElement | undefined = $state();
  let canLeft = $state(false);
  let canRight = $state(false);

  function measure() {
    if (!strip) return;
    canLeft = strip.scrollLeft > 1;
    canRight = strip.scrollLeft + strip.clientWidth < strip.scrollWidth - 1;
  }
  const scrollBy = (by: number) => strip?.scrollBy({ left: by, behavior: 'smooth' });
  // A mouse wheel moves the strip sideways.
  function onWheel(event: WheelEvent) {
    if (!strip || Math.abs(event.deltaY) <= Math.abs(event.deltaX)) return;
    event.preventDefault();
    strip.scrollLeft += event.deltaY;
  }

  // The active tab is always in view (a file just opened from the chat lands at the end of the strip), clear of the
  // fades. Twice: the arrows that appear after the first scroll make the strip narrower.
  function reveal(id: string) {
    const el = strip?.querySelector<HTMLElement>(`[data-tab-id="${CSS.escape(id)}"]`);
    if (!strip || !el) return;
    const FADE = 32;
    const box = strip.getBoundingClientRect();
    const tab = el.getBoundingClientRect();
    if (tab.left < box.left + FADE) strip.scrollLeft -= box.left + FADE - tab.left;
    else if (tab.right > box.right - FADE) strip.scrollLeft += tab.right - (box.right - FADE);
    measure();
  }
  $effect(() => {
    const id = active?.id;
    void viewer.tabs.length;
    if (!id) return;
    void tick()
      .then(() => reveal(id))
      .then(tick)
      .then(() => reveal(id));
  });
  $effect(() => {
    if (!strip) return;
    const observer = new ResizeObserver(measure);
    observer.observe(strip);
    return () => observer.disconnect();
  });


  // Every open file, to pick one when the tabs do not all fit.
  const tabListItems = $derived<PickerItem<string>[]>(
    viewer.tabs.map((tab) => ({ id: tab.id, label: tab.name, hint: dirOf(relativeTo(tab.root, tab.path)) || undefined, icon: icon('lucideFile'), selected: tab.id === active?.id, data: tab.id })),
  );

  // Right click on a tab: the same menu the chat tabs have.
  let tabMenu = $state<{ x: number; y: number; id: string } | null>(null);
  let tabMenuOpen = $state(false);
  let tabMenuKey = $state(0);
  $effect(() => closeOnScroll(() => tabMenuOpen, () => (tabMenuOpen = false)));
  function openTabMenu(event: MouseEvent, id: string) {
    event.preventDefault();
    tabMenuKey++;
    tabMenu = { x: event.clientX, y: event.clientY, id };
    tabMenuOpen = true;
    // The editors this machine has, for "Open with": found while the menu is opening.
    openWithList = [];
    const file = viewer.tabs.find((tab) => tab.id === id);
    if (file) void openWithOptions(file.path).then((found) => tabMenu?.id === id && (openWithList = found));
  }
  let openWithList = $state<OpenWithOption[]>([]);
  const menuTab = $derived(tabMenu ? viewer.tabs.find((tab) => tab.id === tabMenu?.id) : undefined);
  const tabContextItems = $derived<ZDropdownMenuItem[]>(
    menuTab
      ? [
          ...(menuTab.dirty ? [{ label: 'Save', icon: 'lucideSave', iconSize: '12', shortcut: 'Ctrl+S', divide: 'after', class: 'text-xs! py-1', onClick: () => void saveTab(menuTab.id, menuTab) } as ZDropdownMenuItem] : []),
          { label: 'Open in a main tab', icon: 'lucideSquareArrowUpRight', iconSize: '12', class: 'text-xs! py-1', onClick: () => openFileTab(menuTab.path, menuTab.root, menuTab.view) },
          ...openWithItem(menuTab.path, openWithList),
          { label: 'Copy path', icon: 'lucideCopy', iconSize: '12', class: 'text-xs! py-1', divide: 'after', onClick: () => void navigator.clipboard.writeText(menuTab.path) },
          { label: 'Close tab', icon: 'lucideX', iconSize: '12', class: 'text-xs! py-1', onClick: () => closeViewerTab(menuTab.id) },
          { label: 'Close other tabs', icon: 'lucideX', iconSize: '12', class: 'text-xs! py-1', disabled: viewer.tabs.length < 2, onClick: () => closeOtherViewerTabs(menuTab.id) },
          { label: 'Close all tabs', icon: 'lucideListX', iconSize: '12', divide: 'before', class: 'text-xs! py-1', onClick: () => closeAllViewerTabs() },
        ]
      : [],
  );

  let width = $state(0);
  // Below this only one view fits: the file, or (on demand) the tree in its place.
  const narrow = $derived(width > 0 && width < 300);
  // Keep a user-sized tree width, clamped so the file preview retains useful space.
  const treeWidth = $derived(Math.max(140, Math.min(viewer.side.width, Math.min(480, width - 180))));
  function resizeTree(delta: number) {
    // From the width on screen (clamped), so a drag past a limit does not leave the line behind the pointer.
    viewer.side.width = Math.max(140, Math.min(480, treeWidth - delta));
  }
  // While the line is dragged the tree follows the pointer at once; the slide is only for opening and closing.
  let resizing = $state(false);
  // A tree that narrow shows its Files / Changes switch as icons only.
  const tightTree = $derived(active !== undefined && !narrow && treeWidth < 200);
  // A narrow panel has no room for both: the tree is hidden, and shown on demand in place of the file.
  let treeOnly = $state(false);
  // With no file open the tree is all there is to show.
  const sideShown = $derived(narrow ? treeOnly : !active || viewer.side.open);
  function setSide(open: boolean) {
    if (narrow) treeOnly = open;
    else viewer.side.open = open;
  }
  const sideLabel = $derived(viewer.side.mode === 'files' ? 'Project files' : 'Changes');
  let findOpen = $state(false);
  let changesFindOpen = $state(false);
  // The search opens under the button that was pressed: this one, or the search bar of the pane.
  let headFind = $state<HTMLElement>();
  let findFrom = $state<HTMLElement | null>(null);
  $effect(() => {
    if (!findOpen && !changesFindOpen) findFrom = null;
  });
  let changesCount = $state(0);
  // Nothing to search in an empty list of changes: the button waits for the files.
  const canFind = $derived(viewer.side.mode !== 'changes' || changesCount > 0);
  // The search button looks in what the side pane shows: every file, or only the changed ones.
  function findFile() {
    // Pressed again while its popover is open: it closes.
    if (findOpen || changesFindOpen) {
      findOpen = false;
      changesFindOpen = false;
      return;
    }
    setSide(true);
    findFrom = headFind ?? null;
    if (viewer.side.mode === 'changes') changesFindOpen = true;
    else findOpen = true;
  }
  // Narrow panel: opening a file (from the tree, a tab, the chat) puts the tree away to show it.
  let seen = { opened: viewer.opened, active: viewer.active };
  $effect(() => {
    const now = { opened: viewer.opened, active: viewer.active };
    if (now.opened === seen.opened && now.active === seen.active) return;
    seen = now;
    treeOnly = false;
  });

  const arrow = 'grid size-7 shrink-0 cursor-pointer place-items-center rounded-md text-muted-foreground outline-none hover:bg-primary/10 hover:text-primary';
</script>

{#snippet sidePane()}
  <div class="flex h-10 shrink-0 items-center border-b border-border px-1.5">
    <div class="relative grid h-8 w-full grid-cols-2 rounded-md border border-border bg-muted p-0.5 text-xs" role="radiogroup" aria-label="Show">
      <span class="pointer-events-none absolute inset-y-0.5 left-0.5 w-[calc(50%-2px)] rounded bg-background shadow-sm transition-transform duration-200 ease-out" style:transform={viewer.side.mode === 'changes' ? 'translateX(100%)' : 'translateX(0)'} aria-hidden="true"></span>
      {#each [{ value: 'files', label: 'Files', glyph: 'lucideFolder' }, { value: 'changes', label: 'Changes', glyph: 'lucideGitCompare' }] as item (item.value)}
        <button type="button" role="radio" aria-checked={viewer.side.mode === item.value} aria-label={item.label} title={tightTree ? item.label : undefined} class="relative z-10 inline-flex cursor-pointer items-center justify-center gap-1.5 rounded outline-none transition-colors {viewer.side.mode === item.value ? 'text-primary' : 'text-muted-foreground hover:text-foreground'}" onclick={() => (viewer.side.mode = item.value as 'files' | 'changes')}>
          <ZIcon zType={icon(item.glyph)} zSize="14" /> {#if !tightTree}{item.label}{/if}
        </button>
      {/each}
    </div>
  </div>
  <!-- Both stay mounted: switching keeps the tree's open folders and the changes already read. -->
  <div class="min-h-0 flex-1 flex-col {viewer.side.mode === 'files' ? 'flex' : 'hidden'}"><FileExplorer bind:findOpen from={findFrom} /></div>
  <div class="min-h-0 flex-1 flex-col {viewer.side.mode === 'changes' ? 'flex' : 'hidden'}"><ChangesView bind:findOpen={changesFindOpen} bind:count={changesCount} from={findFrom} /></div>
{/snippet}

<div bind:clientWidth={width} class="flex h-full min-h-0 w-full flex-1 flex-col bg-background" aria-label="Files">
  <div class="flex h-10 shrink-0 items-center gap-1 border-b border-border px-1.5">
    {#if viewer.tabs.length === 0}
      <span class="min-w-0 flex-1 truncate px-1.5 text-xs font-medium text-foreground">Files</span>
    {:else}
      <div class="relative flex min-w-0 flex-1 items-center">
        <div bind:this={strip} class="flex min-w-0 flex-1 items-center gap-1 overflow-hidden" role="tablist" aria-label="Open files" tabindex="-1" onscroll={measure} onwheel={onWheel}>
          {#each viewer.tabs as tab (tab.id)}
            {@const selected = tab.id === active?.id}
            <div
              data-tab-id={tab.id}
              role="tab"
              tabindex="0"
              aria-selected={selected}
              class="group/tab flex h-7 max-w-48 shrink-0 cursor-pointer items-center gap-1 rounded-md pr-1 pl-2.5 text-xs outline-none {selected ? 'bg-primary/15 text-primary' : 'text-muted-foreground hover:bg-primary/10 hover:text-primary'}"
              title={tab.path}
              onclick={() => (viewer.active = tab.id)}
              onauxclick={(event) => event.button === 1 && closeViewerTab(tab.id)}
              oncontextmenu={(event) => openTabMenu(event, tab.id)}
              onkeydown={(event) => event.key === 'Enter' && (viewer.active = tab.id)}
            >
              <span class="min-w-0 flex-1 truncate">{tab.name}</span>
              <button type="button" class="grid size-4.5 shrink-0 cursor-pointer place-items-center rounded-full outline-none hover:bg-primary/20" aria-label="Close {tab.name}" onclick={(event) => { event.stopPropagation(); closeViewerTab(tab.id); }}>
                {#if tab.dirty}
                  <span class="size-2 rounded-full bg-current group-hover/tab:hidden" aria-label="Unsaved"></span>
                  <ZIcon zType={icon('lucideX')} zSize="12" class="hidden size-3 group-hover/tab:block" />
                {:else}
                  <ZIcon zType={icon('lucideX')} zSize="12" class="size-3" />
                {/if}
              </button>
            </div>
          {/each}
        </div>
        {#if canLeft}
          <div class="pointer-events-none absolute inset-y-0 left-0 z-10 w-8 bg-gradient-to-r from-background to-transparent" aria-hidden="true"></div>
        {/if}
        {#if canRight}
          <div class="pointer-events-none absolute inset-y-0 right-0 z-10 w-8 bg-gradient-to-r from-transparent to-background" aria-hidden="true"></div>
        {/if}
      </div>
      {#if canLeft || canRight}
        <!-- Every tab in a list, to jump to one: styled like the other ghost icon buttons. -->
        <TabListButton items={tabListItems} onpick={(id) => (viewer.active = id)} label="Open files" />
      {/if}
    {/if}
    <!-- Find a file by name: the tree's own search, opened from here too. -->
    {#if canFind}
      <button bind:this={headFind} type="button" class="{arrow} {findOpen || changesFindOpen ? '!bg-primary/15 !text-primary hover:!bg-primary/20 hover:!text-primary' : ''}" aria-label={viewer.side.mode === 'changes' ? 'Find a changed file' : 'Find a file'} {@attach zTooltip({ content: viewer.side.mode === 'changes' ? 'Find a changed file' : 'Find a file', position: 'top' })} onclick={findFile}>
        <ZIcon zType={icon('lucideSearch')} class="size-4" />
      </button>
    {/if}
    {#if active || narrow}
      <button type="button" class={arrow} aria-pressed={sideShown} aria-label={sideShown ? 'Hide the file tree' : 'Show the file tree'} {@attach zTooltip({ content: sideShown ? 'Hide the file tree' : 'Show the file tree', position: 'top' })} onclick={() => setSide(!sideShown)}>
        <ZIcon zType={icon(sideShown ? 'lucideFolderOpen' : 'lucideFolderClosed')} class="size-4" />
      </button>
    {/if}
  </div>

  <div class="relative flex min-h-0 flex-1">
    <!-- Keep the file view visible when the last tab closes; show an empty landing area in its place. -->
    <div class="min-w-0 flex-1 flex-col {narrow && treeOnly ? 'hidden' : 'flex'}">
      <div class="min-h-0 flex-1">
        {#if active}
          {#key active.id}
            <FileView file={active} />
          {/key}
        {:else}
          <div class="flex h-full min-h-0 flex-col items-center justify-center gap-3 bg-background p-6 text-center" aria-label="No file open">
            <ZIcon zType={icon('lucideFolder')} zSize="28" class="size-7 text-muted-foreground/70" />
            <div class="flex flex-col gap-1">
              <p class="m-0 text-sm font-medium text-foreground">Open files appear here</p>
              <p class="m-0 max-w-56 text-xs leading-relaxed text-muted-foreground">Pick a file in the tree, or click a file path in the conversation.</p>
            </div>
          </div>
        {/if}
      </div>
    </div>
    {#if narrow}
      <!-- A narrow panel shows one view at a time: the tree replaces the empty/file view on demand. -->
      <div class="min-h-0 flex-1 flex-col bg-sidebar {treeOnly ? 'flex' : 'hidden'}" aria-label={sideLabel}>
        {@render sidePane()}
      </div>
    {:else}
      <!-- The tree sits beside the file view and stays mounted: opening and closing slide its width, like a drawer, instead
           of removing it. The inner width is fixed, so the content does not squeeze while the wrapper moves. -->
      <!-- `inert` while shut: the tree stays mounted, so without it Tab would still walk into the invisible list. -->
      <div class="flex min-h-0 shrink-0 overflow-hidden {resizing ? '' : 'transition-[width] duration-200 ease-out motion-reduce:transition-none'}" style:width="{sideShown ? treeWidth + 1 : 0}px" aria-hidden={!sideShown} inert={!sideShown}>
        <Splitter axis="x" label="Resize file explorer" onDelta={resizeTree} bind:dragging={resizing} />
        <div class="flex min-h-0 shrink-0 flex-col bg-sidebar" style:width="{treeWidth}px" aria-label={sideLabel}>
          {@render sidePane()}
        </div>
      </div>
    {/if}
  </div>
</div>

{#if tabMenu}
  {#key tabMenuKey}
    <Menu items={tabContextItems} bind:open={tabMenuOpen} at={{ x: tabMenu.x, y: tabMenu.y }} position="bottom-left" width={220} />
  {/key}
{/if}
