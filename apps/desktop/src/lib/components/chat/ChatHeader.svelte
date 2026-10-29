<script lang="ts">
  import { tick } from 'svelte';
  import { ZIcon } from '@shival99/z-ui-svelte';
  import type { ZDropdownMenuItem, ZIconName } from '@shival99/z-ui-svelte';
  import { windowDrag } from '../../app/platform/window-drag';
  import Menu from '../ui/Menu.svelte';
  import TabListButton from '../ui/TabListButton.svelte';
  import type { PickerItem } from '../ui/PickerPopover.svelte';
  import { closeOnScroll } from '../../app/ui/close-on-scroll';
  import { revealTab } from '../../app/ui/tab-strip';
  import { toast } from '../../app/platform/toast';
  import { layout, leavePeek, peekSidebar, pinSidebar } from '../../app/ui/layout.svelte';
  import { activateTab, closeAllTabs, closeOtherTabs, closeTab, contentTabs, openNewChatTab, saveTab, type ContentTab } from '../../app/ui/content-tabs.svelte';
  import { openWithItem, openWithOptions, type OpenWithOption } from '../../files/open-with';
  import { openInViewer } from '../../files/file-viewer.svelte';
  import { displayPath } from '../../files/paths';
  import { conversationForExport, workspace } from '../../chat/sessions.svelte';
  import { conversationMarkdown } from '../../chat/content/conversation-export';
  import { inTauri } from '../../app/platform/history';
  import ToolbarActions from '../layout/ToolbarActions.svelte';

  // The tab strip over the conversation: chats and files side by side, the folded-sidebar button, tab context menu.
  async function copyConversation(sessionId: string) {
    try {
      const { title, messages } = await conversationForExport(sessionId);
      await navigator.clipboard.writeText(conversationMarkdown(title, messages));
      toast.success('Conversation copied as Markdown');
    } catch (error) {
      toast.error(`Could not copy conversation: ${String(error)}`);
    }
  }

  async function exportConversationPath(sessionId: string) {
    try {
      const { title, messages, sourcePath } = await conversationForExport(sessionId);
      const text = conversationMarkdown(title, messages, sourcePath);
      const stem = title.trim().replace(/[^\p{L}\p{N}_-]+/gu, '-').replace(/^-+|-+$/g, '').slice(0, 60) || 'conversation';
      if (inTauri()) {
        const { invoke } = await import('@tauri-apps/api/core');
        const path = await invoke<string>('conversation_export', { name: `${stem}-${sessionId}.md`, text });
        await navigator.clipboard.writeText(path);
        toast.success('Conversation file path copied');
      } else {
        const url = URL.createObjectURL(new Blob([text], { type: 'text/markdown' }));
        const link = document.createElement('a');
        link.href = url;
        link.download = `${stem}-${sessionId}.md`;
        link.click();
        URL.revokeObjectURL(url);
        toast.success('Conversation downloaded as Markdown');
      }
    } catch (error) {
      toast.error(`Could not export conversation: ${String(error)}`);
    }
  }

  // The folded desktop sidebar: hovering its button shows the sidebar floating over the app (SidebarPeek) after a
  // short wait. A click pins it back into the layout, in the place and at the width of the peek.
  function railEnter() {
    if (!layout.sidebarCollapsed || layout.sidebarHidden) return;
    peekSidebar(layout.sidebarDrawer ? 0 : 200);
  }
  function railLeave() {
    if (layout.sidebarCollapsed && !layout.sidebarHidden) leavePeek();
  }
  // The peek covers the folded button: a press where the button is still pins the sidebar (it lands on the peek's
  // header otherwise).
  let railButton = $state<HTMLButtonElement | undefined>();
  $effect(() => {
    if (!layout.sidebarDrawer || !layout.sidebarCollapsed) return;
    const onPress = (event: PointerEvent) => {
      const box = railButton?.getBoundingClientRect();
      if (!box || event.clientX < box.left || event.clientX > box.right || event.clientY < box.top || event.clientY > box.bottom) return;
      event.preventDefault();
      event.stopPropagation();
      pinSidebar();
    };
    document.addEventListener('pointerdown', onPress, true);
    return () => document.removeEventListener('pointerdown', onPress, true);
  });

  // Right click on a tab: close it, the others, or all of them.
  let tabMenu = $state<{ x: number; y: number; id: string } | null>(null);
  let tabMenuOpen = $state(false);
  $effect(() => closeOnScroll(() => tabMenuOpen, () => (tabMenuOpen = false)));
  let tabMenuKey = $state(0);
  let openWithLoadedFor = '';
  let openWithLoadingFor = '';
  function loadTabOpenWith(path: string, id: string) {
    if (openWithLoadedFor === path || openWithLoadingFor === path) return;
    openWithLoadingFor = path;
    void openWithOptions(path).then((found) => {
      if (tabMenu?.id !== id) return;
      openWithList = found;
      openWithLoadedFor = path;
      openWithLoadingFor = '';
    });
  }
  function openTabMenu(event: MouseEvent, id: string) {
    event.preventDefault();
    tabMenuKey++;
    tabMenu = { x: event.clientX, y: event.clientY, id };
    tabMenuOpen = true;
    // Application discovery is deferred until the user opens the Open with submenu.
    openWithList = [];
    openWithLoadedFor = '';
    openWithLoadingFor = '';
  }
  let openWithList = $state<OpenWithOption[]>([]);
  // Save shows only for a file tab with unsaved edits; it saves that tab, in front or not.
  const menuTab = $derived(tabMenu ? contentTabs.tabs.find((tab) => tab.id === tabMenu?.id) : undefined);
  const tabContextItems = $derived<ZDropdownMenuItem[]>([
    ...(menuTab?.kind === 'file' && menuTab.dirty
      ? [{ label: 'Save', icon: 'lucideSave', iconSize: '12', shortcut: 'Ctrl+S', divide: 'after', class: 'text-sm! py-1', onClick: () => tabMenu && void saveTab(tabMenu.id) } as ZDropdownMenuItem]
      : []),
    ...(menuTab?.kind === 'file'
      ? [
          { label: 'Open in the side panel', icon: 'lucidePanelRight', iconSize: '12', class: 'text-sm! py-1', onClick: () => openInViewer(menuTab.path, menuTab.root, menuTab.view) } as ZDropdownMenuItem,
          ...openWithItem(menuTab.path, openWithList, () => loadTabOpenWith(menuTab.path, menuTab.id)),
          { label: 'Copy path', icon: 'lucideCopy', iconSize: '12', class: 'text-sm! py-1', divide: 'after', onClick: () => void navigator.clipboard.writeText(menuTab.path) } as ZDropdownMenuItem,
        ]
      : []),
    ...(menuTab?.kind === 'chat'
      ? [
          { label: 'Copy conversation', icon: 'lucideCopy', iconSize: '12', class: 'text-sm! py-1', onClick: () => void copyConversation(menuTab.sessionId) } as ZDropdownMenuItem,
          { label: inTauri() ? 'Copy conversation file path' : 'Download conversation file', icon: 'lucideFileText', iconSize: '12', class: 'text-sm! py-1', onClick: () => void exportConversationPath(menuTab.sessionId) } as ZDropdownMenuItem,
          ...(workspace.sessions.find((session) => session.id === menuTab.sessionId)?.path
            ? [{ label: 'Copy source transcript path', icon: 'lucideLink', iconSize: '12', class: 'text-sm! py-1', onClick: () => void navigator.clipboard.writeText(workspace.sessions.find((session) => session.id === menuTab.sessionId)?.path ?? '') } as ZDropdownMenuItem]
            : []),
        ]
      : []),
    { label: 'Close tab', icon: 'lucideX', iconSize: '12', shortcut: 'Ctrl+W', class: 'text-sm! py-1', onClick: () => tabMenu && closeTab(tabMenu.id) },
    { label: 'Close other tabs', icon: 'lucideX', iconSize: '12', shortcut: 'Ctrl+K Ctrl+O', class: 'text-sm! py-1', onClick: () => tabMenu && closeOtherTabs(tabMenu.id) },
    { label: 'Close all tabs', icon: 'lucideListX', iconSize: '12', shortcut: 'Ctrl+K Ctrl+W', divide: 'before', class: 'text-sm! py-1', onClick: () => closeAllTabs() },
  ]);

  // Tab strip: label per tab, and the dropdown that lists every tab once they overflow the strip.
  const tabLabel = (tab: ContentTab) =>
    tab.kind === 'chat' ? (workspace.sessions.find((item) => item.id === tab.sessionId)?.title ?? 'New session') : tab.name;
  let tabStrip: HTMLDivElement | undefined = $state();
  let tabStripWidth = $state(0);
  let tabsOverflow = $state(false);
  // Tabs cut off on the left: the strip is scrolled past its first tab.
  let tabScrolled = $state(false);
  $effect(() => {
    void contentTabs.tabs.length;
    void tabStripWidth;
    tabsOverflow = contentTabs.tabs.length > 1 && !!tabStrip && tabStrip.scrollWidth > tabStrip.clientWidth + 1;
  });
  // The active tab (a click, or a pick in the arrow menu) scrolls into view.
  $effect(() => {
    const id = contentTabs.active;
    void contentTabs.tabs.length;
    void tabStripWidth;
    void tick().then(() => {
      revealTab(tabStrip, id);
      tabScrolled = (tabStrip?.scrollLeft ?? 0) > 1;
    });
  });
  const tabListItems = $derived<PickerItem<string>[]>(
    contentTabs.tabs.map((tab) => ({ id: tab.id, label: tabLabel(tab), icon: (tab.kind === 'chat' ? 'lucideMessageSquare' : 'lucideFile') as ZIconName, selected: contentTabs.active === tab.id, data: tab.id })),
  );
</script>

<header class="relative z-40 flex h-10 shrink-0 items-center border-b border-border bg-background {layout.toolsVisible && !layout.toolsOnLeft ? 'pr-[7px]' : 'pr-[106px]'}" {@attach windowDrag}>
  <!-- Browser-style tabs: chats and files side by side, each with a close button; + starts a new chat.
       No scrollbar: when the tabs overflow, an arrow opens the full list. -->
  {#if layout.sidebarHidden || layout.sidebarCollapsed}
    <button
      bind:this={railButton}
      type="button"
      class="ml-2 grid size-8 shrink-0 cursor-pointer place-items-center rounded-md text-muted-foreground hover:bg-primary/10 hover:text-primary"
      aria-label={layout.sidebarHidden ? 'Open sessions' : 'Show sidebar (Ctrl+B)'}
      title={layout.sidebarHidden ? 'Sessions' : 'Show sidebar (Ctrl+B)'}
      data-sidebar-rail
      onpointerenter={railEnter}
      onpointerleave={railLeave}
      onclick={() => (layout.sidebarHidden ? (layout.sidebarDrawer = !layout.sidebarDrawer) : pinSidebar())}
    >
      <ZIcon zType={layout.sidebarHidden ? 'lucideMenu' : 'lucidePanelLeft'} class="size-4" />
    </button>
  {/if}
  <div class="flex min-w-0 flex-1 items-center gap-1 pl-2">
    <div class="relative flex min-w-0 shrink items-center">
      <div bind:this={tabStrip} bind:clientWidth={tabStripWidth} onscroll={() => (tabScrolled = (tabStrip?.scrollLeft ?? 0) > 1)} class="flex min-w-0 flex-1 items-center gap-1 overflow-hidden" role="tablist" aria-label="Open tabs">
        {#each contentTabs.tabs as tab (tab.id)}
          {@const active = contentTabs.active === tab.id}
          {@const label = tabLabel(tab)}
          <div
            role="tab"
            tabindex="0"
            data-tab-id={tab.id}
            aria-selected={active}
            title={tab.kind === 'file' ? displayPath(tab.root, tab.path) : label}
            data-tip-clip
            class="flex h-7 max-w-56 shrink-0 cursor-pointer items-center justify-between gap-1 rounded-md py-0 pr-2 pl-2 text-sm outline-none focus-visible:ring-0 {active ? 'bg-primary/15 text-primary' : 'text-muted-foreground hover:bg-primary/10 hover:text-primary'}"
            onclick={() => activateTab(tab.id)}
            oncontextmenu={(event) => openTabMenu(event, tab.id)}
            onkeydown={(event) => event.key === 'Enter' && activateTab(tab.id)}
          >
            <span class="truncate">{label}</span>
            {#if tab.kind === 'file' && tab.dirty}<span class="size-1.5 shrink-0 rounded-full bg-current" title="Unsaved changes" aria-label="Unsaved changes"></span>{/if}
            <button type="button" class="grid size-4.5 shrink-0 cursor-pointer place-items-center rounded-full outline-none focus-visible:ring-0 hover:bg-primary/20" aria-label="Close {label}" onclick={(event) => { event.stopPropagation(); closeTab(tab.id); }}>
              <ZIcon zType="lucideX" zSize="12" class="size-3" />
            </button>
          </div>
        {/each}
      </div>
      {#if tabsOverflow}
        <!-- Fade into a solid backing: the strip continues under it, the arrow sits on solid background. -->
        <div class="pointer-events-none absolute inset-y-0 right-0 z-10 w-10 bg-gradient-to-r from-transparent to-background" aria-hidden="true"></div>
      {/if}
      {#if tabScrolled}
        <div class="pointer-events-none absolute inset-y-0 left-0 z-10 w-10 bg-gradient-to-r from-background to-transparent" aria-hidden="true"></div>
      {/if}
    </div>
    {#if tabsOverflow}
      <div class="-mr-0.5 flex shrink-0 items-center bg-background">
        <TabListButton items={tabListItems} onpick={activateTab} label="Open tabs" />
      </div>
    {/if}
    {#if contentTabs.tabs.some((tab) => tab.kind === 'chat')}
      <button
        type="button"
        class="grid size-8 shrink-0 cursor-pointer place-items-center rounded-md text-muted-foreground outline-none focus-visible:ring-0 hover:bg-primary/10 hover:text-primary"
        aria-label="New tab"
        title="New tab"
        onclick={() => openNewChatTab()}
      >
        <ZIcon zType="lucidePlus" class="size-4" />
      </button>
    {/if}
  </div>
  <div class="flex shrink-0 items-center">
    <ToolbarActions divider={false} />
  </div>
</header>

{#if tabMenu}
  {#key tabMenuKey}
    <Menu items={tabContextItems} bind:open={tabMenuOpen} at={{ x: tabMenu.x, y: tabMenu.y }} position="bottom-left" width={260} />
  {/key}
{/if}
