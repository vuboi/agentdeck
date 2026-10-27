<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import LinkMenu from '../lib/components/LinkMenu.svelte';
  import SelectionMenu from '../lib/components/SelectionMenu.svelte';
  import WindowControls from '../lib/components/WindowControls.svelte';
  import ThemePanel from '../lib/components/ThemePanel.svelte';
  import SettingsModal from '../lib/components/SettingsModal.svelte';
  import FlowEditor from '../lib/components/flow/FlowEditor.svelte';
  import { closeFlow } from '../lib/workflows.svelte';
  import { guide } from '../lib/guide.svelte';
  import { toast } from '../lib/toast';
  import { openSettings, settings } from '../lib/connectors.svelte';
  import ToolsModal from '../lib/components/ToolsModal.svelte';
  import Sidebar from '../lib/components/Sidebar.svelte';
  import SidebarPeek from '../lib/components/SidebarPeek.svelte';
  import Chat from '../lib/components/Chat.svelte';
  import Terminal from '../lib/components/Terminal.svelte';
  import RightPanel from '../lib/components/RightPanel.svelte';
  import BrowserGuide from '../lib/components/BrowserGuide.svelte';
  import DockZone from '../lib/components/DockZone.svelte';
  import Splitter from '../lib/components/Splitter.svelte';
  import { slide } from 'svelte/transition';
  import { slideWidth } from '../lib/transitions';
  import { layout, setSidebarCollapsed, setToolsOpen, type DockZone as Zone } from '../lib/layout.svelte';
  import { terminal, handleTerminalShortcut } from '../lib/terminal.svelte';
  import { browser } from '../lib/browser.svelte';
  import { inTauri } from '../lib/history';
  import { workspace } from '../lib/sessions.svelte';
  import { openNewChatTab } from '../lib/content-tabs.svelte';
  import FindBar from '../lib/components/FindBar.svelte';
  import KeyboardShortcuts from '../lib/components/KeyboardShortcuts.svelte';

  // Layout, outside in: sidebar | [left zone | (top zone / chat / bottom zone / terminal) | right zone] | tools.
  // Every pane is a fixed px size kept in layout.sizes; splitters between them change those sizes.
  // A zone takes space only while a window is dragged or while it holds tools.
  const MIN = { chat: 520, sidebar: 160, tools: 220, left: 200, right: 200, top: 120, bottom: 120, terminal: 80 };
  // Side panes shrink with the window (their width is the flex basis). The conversation keeps its own minimum,
  // so when the window is too narrow the whole shell scrolls sideways instead of panes overlapping.
  const SIDE = 'min-w-0 shrink overflow-hidden';

  function shows(zone: Zone): boolean {
    return layout.docked[zone].length > 0;
  }

  function clamp(value: number, low: number, high: number) {
    return Math.min(Math.max(value, low), Math.max(low, high));
  }

  // Splitter handlers. Left/top/sidebar grow with the pointer; right/bottom/terminal/tools grow against it.
  const grow = (key: keyof typeof layout.sizes, sign: 1 | -1, low: number, high: () => number) => (delta: number) => {
    layout.sizes[key] = clamp(layout.sizes[key] + sign * delta, low, high());
  };
  const maxW = () => window.innerWidth - 480;
  const maxH = () => window.innerHeight - 200;

  // Responsive: under 1000px only the conversation and the sidebar stay; under 720px the sidebar becomes a drawer.
  let innerW = $state(typeof window === 'undefined' ? 1280 : window.innerWidth);

  // The DOM resize event can arrive after the window has already been resized by the OS (maximise, restore),
  // so the layout would switch a step late. Tauri's own resize event is the trigger; read the width then.
  onMount(() => {
    const sync = () => {
      innerW = window.innerWidth;
    };
    let stop: (() => void) | undefined;
    if (inTauri()) {
      void import('@tauri-apps/api/window').then(({ getCurrentWindow }) =>
        getCurrentWindow()
          .onResized(() => {
            sync();
            requestAnimationFrame(sync);
          })
          .then((unlisten) => (stop = unlisten)),
      );
    }
    return () => stop?.();
  });
  const narrow = $derived(innerW < 720);
  const showSidebar = $derived(!narrow && !layout.sidebarCollapsed);
  const showZones = $derived(innerW >= 1000);
  const showTools = $derived(layout.toolsOpen && !narrow);
  const toolsAvailableWidth = $derived.by(() => {
    const sidebar = showSidebar ? Math.max(180, layout.sizes.sidebar) + 1 : 0;
    const left = showZones && shows('left') ? Math.max(MIN.left, layout.sizes.left) + 1 : 0;
    const right = showZones && shows('right') ? Math.max(MIN.right, layout.sizes.right) + 1 : 0;
    const toolsSplitter = showTools || layout.toolsWidthAnimating ? 1 : 0;
    // The conversation keeps its own minimum: dragging the tools panel wider stops there instead of covering the input.
    return Math.max(0, innerW - MIN.chat - sidebar - left - right - toolsSplitter);
  });
  const toolsPanelWidth = $derived(Math.min(layout.sizes.tools, Math.max(MIN.tools, toolsAvailableWidth)));

  $effect(() => {
    layout.toolsVisible = showTools && toolsPanelWidth > 0;
  });

  $effect(() => {
    const hidden = narrow;
    layout.sidebarHidden = hidden;
    if (!hidden) layout.sidebarDrawer = false;
  });

  // Picking a session closes the drawer, so the conversation is what is left on screen.
  $effect(() => {
    void workspace.activeId;
    untrack(() => (layout.sidebarDrawer = false));
  });

  // Developer hook (AGENTDECK_OPEN_PREVIEW): the app opens the Preview tool on a page by itself.
  onMount(() => {
    if (!import.meta.env.DEV || !inTauri()) return;
    let stop: (() => void) | undefined;
    void import('../lib/tauri-event').then(({ listen }) =>
      listen<string>('dev-preview', ({ payload }) => {
        const [address, device, then] = payload.split('|');
        if (device === 'mobile' || device === 'tablet') browser.device = device;
        browser.autostart = address;
        layout.activeTool = 'Preview';
        setToolsOpen(true);
        // `|menu`: open the page menu once the page is up, to check menus drawn over it.
        if (then === 'menu') setTimeout(() => document.querySelector<HTMLElement>('[data-page-menu] button')?.click(), 6000);
        if (then === 'search') setTimeout(() => document.querySelector<HTMLElement>('[aria-label^="Search chats"]')?.click(), 6000);
      }).then((unlisten) => (stop = unlisten)),
    );
    return () => stop?.();
  });

  let findOpen = $state(false);
  let findBar: FindBar | undefined = $state();
  let shortcutsOpen = $state(false);

  function onKey(event: KeyboardEvent) {
    handleTerminalShortcut(event);
    const mod = event.ctrlKey || event.metaKey;
    // Ctrl+F: find in the whole app, as in a browser. A file with the focus keeps its own find (FileView.svelte).
    if (mod && !event.shiftKey && !event.altKey && event.key.toLowerCase() === 'f') {
      const active = document.activeElement;
      if (active?.closest('[data-file-find], .xterm')) return;
      event.preventDefault();
      if (findOpen) void findBar?.focus();
      else findOpen = true;
    }
    // Ctrl+/ or Ctrl+? (Shift+/): the list of shortcuts.
    if (mod && !event.altKey && (event.key === '/' || event.key === '?')) {
      event.preventDefault();
      shortcutsOpen = !shortcutsOpen;
    }
    // Ctrl+B (Cmd+B): fold the sidebar away or bring it back (the drawer, in a narrow window).
    if ((event.ctrlKey || event.metaKey) && !event.shiftKey && !event.altKey && event.key.toLowerCase() === 'b') {
      event.preventDefault();
      if (narrow) layout.sidebarDrawer = !layout.sidebarDrawer;
      else setSidebarCollapsed(!layout.sidebarCollapsed);
    }
    // Ctrl+, (Cmd+,): Settings.
    if ((event.ctrlKey || event.metaKey) && event.key === ',') {
      event.preventDefault();
      if (settings.open) settings.open = false;
      else openSettings();
    }
    // Ctrl+T (Cmd+T on macOS): a new chat tab, the same as the + button.
    if ((event.ctrlKey || event.metaKey) && !event.shiftKey && event.key.toLowerCase() === 't') {
      event.preventDefault();
      openNewChatTab();
    }
  }
</script>

<svelte:head><title>AgentDeck</title><meta name="description" content="Your multi-agent desktop workspace" /></svelte:head>
<svelte:window bind:innerWidth={innerW} onkeydown={onKey} />

<div class="relative flex h-screen overflow-x-hidden overflow-y-hidden bg-background text-foreground min-[720px]:overflow-x-auto">
  {#if !narrow}
    <div
      class="sidebar-shell flex h-full {SIDE} [&>*]:h-full [&>*]:min-w-0 [&>*]:flex-1"
      class:sidebar-shell-animating={layout.sidebarWidthAnimating}
      class:sidebar-shell-visible={showSidebar}
      style:width="{showSidebar ? layout.sizes.sidebar : 0}px"
      style:minWidth={showSidebar ? '180px' : '0px'}
      style:opacity={showSidebar ? 1 : 0}
      aria-hidden={!showSidebar}
      inert={!showSidebar}
      data-sidebar
    >
      <Sidebar />
    </div>
    {#if showSidebar}<Splitter axis="x" label="Resize sidebar" onDelta={grow('sidebar', 1, MIN.sidebar, maxW)} />{/if}
  {/if}

  <!-- Workspace: left zone | conversation column | right zone -->
  <div class="flex h-full min-w-0 flex-1 min-[720px]:min-w-[280px]" data-workspace>
    {#if showZones && shows('left')}
      <div transition:slideWidth class="flex h-full min-w-[200px] {SIDE} [&>*]:h-full [&>*]:min-w-0 [&>*]:flex-1" style:width="{layout.sizes.left}px">
        <DockZone zone="left" />
      </div>
      <Splitter axis="x" label="Resize left zone" onDelta={grow('left', 1, MIN.left, maxW)} />
    {/if}

    <div class="flex h-full flex-1 flex-col" style:min-width="{MIN.chat}px">
      {#if showZones && shows('top')}
        <div transition:slide={{ duration: 240 }} class="flex flex-col {SIDE} w-full [&>*]:w-full [&>*]:min-h-0 [&>*]:flex-1" style:height="{layout.sizes.top}px">
          <DockZone zone="top" />
        </div>
        <Splitter axis="y" label="Resize top zone" onDelta={grow('top', 1, MIN.top, maxH)} />
      {/if}

      <div class="flex min-h-[240px] flex-1 flex-col">
        <Chat />
      </div>

      {#if showZones && shows('bottom')}
        <Splitter axis="y" label="Resize bottom zone" onDelta={grow('bottom', -1, MIN.bottom, maxH)} />
        <div transition:slide={{ duration: 240 }} class="flex flex-col {SIDE} w-full [&>*]:w-full [&>*]:min-h-0 [&>*]:flex-1" style:height="{layout.sizes.bottom}px">
          <DockZone zone="bottom" />
        </div>
      {/if}

      {#if terminal.open && showZones}
        <Splitter axis="y" label="Resize terminal" onDelta={grow('terminal', -1, MIN.terminal, maxH)} />
        <div transition:slide={{ duration: 240 }} class="flex flex-col {SIDE} w-full [&>*]:w-full [&>*]:min-h-0 [&>*]:flex-1" style:height="{layout.sizes.terminal}px" data-terminal>
          <Terminal />
        </div>
      {/if}
    </div>

    {#if showZones && shows('right')}
      <Splitter axis="x" label="Resize right zone" onDelta={grow('right', -1, MIN.right, maxW)} />
      <div transition:slideWidth class="flex h-full min-w-[200px] {SIDE} [&>*]:h-full [&>*]:min-w-0 [&>*]:flex-1" style:width="{layout.sizes.right}px">
        <DockZone zone="right" />
      </div>
    {/if}
  </div>

  {#if !narrow}
    {#if showTools && toolsPanelWidth >= MIN.tools}<Splitter axis="x" label="Resize tools panel" onDelta={grow('tools', -1, MIN.tools, () => toolsAvailableWidth)} />{/if}
    <div
      class="tools-shell flex h-full min-w-0 shrink-0 overflow-hidden [&>*]:h-full [&>*]:min-w-0 [&>*]:flex-1"
      class:tools-shell-animating={layout.toolsWidthAnimating}
      class:tools-shell-visible={showTools && toolsPanelWidth > 0}
      style:width="{showTools ? toolsPanelWidth : 0}px"
      style:minWidth="{showTools ? Math.min(MIN.tools, toolsPanelWidth) : 0}px"
      style:opacity={showTools && toolsPanelWidth > 0 ? 1 : 0}
      aria-hidden={!showTools || toolsPanelWidth === 0}
      inert={!showTools || toolsPanelWidth === 0}
    >
      {#if (showTools && toolsPanelWidth > 0) || layout.toolsWidthAnimating}
        <RightPanel />
      {/if}
    </div>
  {/if}

  <!-- The folded sidebar (or, in a narrow window, the hidden one), floating over the app. -->
  <SidebarPeek />

  <LinkMenu />
  <SelectionMenu />
  <ThemePanel />
  <SettingsModal />
  <FindBar bind:this={findBar} bind:open={findOpen} />
  <KeyboardShortcuts bind:open={shortcutsOpen} />
  <!-- A fault while the editor draws would leave a click that does nothing: it is reported and the editor is put away. -->
  <svelte:boundary
    onerror={(error) => {
      console.error('flow editor failed', error);
      toast.error(`The flow editor could not open: ${String((error as Error)?.message ?? error)}`);
      closeFlow();
    }}
  >
    <FlowEditor />
  </svelte:boundary>
  <ToolsModal />
  <BrowserGuide bind:open={guide.open} />
  <WindowControls />
</div>
