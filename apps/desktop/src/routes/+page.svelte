<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import { ZDrawer } from '@shival99/z-ui-svelte';
  import LinkMenu from '../lib/components/LinkMenu.svelte';
  import WindowControls from '../lib/components/WindowControls.svelte';
  import ThemePanel from '../lib/components/ThemePanel.svelte';
  import ToolsModal from '../lib/components/ToolsModal.svelte';
  import Sidebar from '../lib/components/Sidebar.svelte';
  import Chat from '../lib/components/Chat.svelte';
  import Terminal from '../lib/components/Terminal.svelte';
  import RightPanel from '../lib/components/RightPanel.svelte';
  import DockZone from '../lib/components/DockZone.svelte';
  import Splitter from '../lib/components/Splitter.svelte';
  import { slide } from 'svelte/transition';
  import { slideWidth } from '../lib/transitions';
  import { layout, type DockZone as Zone } from '../lib/layout.svelte';
  import { terminal, handleTerminalShortcut } from '../lib/terminal.svelte';
  import { browser } from '../lib/browser.svelte';
  import { inTauri } from '../lib/history';
  import { newSession, workspace } from '../lib/sessions.svelte';
  import { openChatTab } from '../lib/content-tabs.svelte';

  // Layout, outside in: sidebar | [left zone | (top zone / chat / bottom zone / terminal) | right zone] | tools.
  // Every pane is a fixed px size kept in layout.sizes; splitters between them change those sizes.
  // A zone takes space only while a window is dragged or while it holds tools.
  const MIN = { sidebar: 160, tools: 220, left: 200, right: 200, top: 120, bottom: 120, terminal: 80 };
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
    if ('__TAURI_INTERNALS__' in window) {
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
  const showSidebar = $derived(innerW >= 720);
  const showZones = $derived(innerW >= 1000);
  const showTools = $derived(layout.toolsOpen && innerW >= 1000);

  $effect(() => {
    layout.toolsVisible = showTools;
  });

  $effect(() => {
    const hidden = !showSidebar;
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
    void import('@tauri-apps/api/event').then(({ listen }) =>
      listen<string>('dev-preview', ({ payload }) => {
        const [address, device] = payload.split('|');
        if (device === 'mobile' || device === 'tablet') browser.device = device;
        browser.autostart = address;
        layout.activeTool = 'Preview';
        layout.toolsOpen = true;
      }).then((unlisten) => (stop = unlisten)),
    );
    return () => stop?.();
  });

  function onKey(event: KeyboardEvent) {
    handleTerminalShortcut(event);
    if (event.key === 'Escape') layout.sidebarDrawer = false;
    // Ctrl+T (Cmd+T on macOS): a new chat tab, the same as the + button.
    if ((event.ctrlKey || event.metaKey) && !event.shiftKey && event.key.toLowerCase() === 't') {
      event.preventDefault();
      newSession(undefined, { reuseDraft: false });
      openChatTab(workspace.activeId);
    }
  }
</script>

<svelte:head><title>AgentDeck</title><meta name="description" content="Your multi-agent desktop workspace" /></svelte:head>
<svelte:window bind:innerWidth={innerW} onkeydown={onKey} />

<div class="relative flex h-screen overflow-x-hidden overflow-y-hidden bg-background text-foreground min-[720px]:overflow-x-auto">
  {#if showSidebar}
    <div class="flex h-full min-w-[180px] {SIDE} [&>*]:h-full [&>*]:min-w-0 [&>*]:flex-1" style:width="{layout.sizes.sidebar}px" data-sidebar>
      <Sidebar />
    </div>
    <Splitter axis="x" label="Resize sidebar" onDelta={grow('sidebar', 1, MIN.sidebar, maxW)} />
  {/if}

  <!-- Workspace: left zone | conversation column | right zone -->
  <div class="flex h-full min-w-0 flex-1 min-[720px]:min-w-[480px]" data-workspace>
    {#if showZones && shows('left')}
      <div transition:slideWidth class="flex h-full min-w-[200px] {SIDE} [&>*]:h-full [&>*]:min-w-0 [&>*]:flex-1" style:width="{layout.sizes.left}px">
        <DockZone zone="left" />
      </div>
      <Splitter axis="x" label="Resize left zone" onDelta={grow('left', 1, MIN.left, maxW)} />
    {/if}

    <div class="flex h-full min-w-0 flex-1 flex-col">
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

  {#if showTools}
    <Splitter axis="x" label="Resize tools panel" onDelta={grow('tools', -1, MIN.tools, maxW)} />
    <div transition:slideWidth class="flex h-full min-w-[240px] {SIDE} [&>*]:h-full [&>*]:min-w-0 [&>*]:flex-1" style:width="{layout.sizes.tools}px">
      <RightPanel />
    </div>
  {/if}

  <!-- Narrow window: the sidebar as a floating drawer over the conversation. -->
  <ZDrawer bind:open={layout.sidebarDrawer} zPlacement="left" zFloating zHideHeader zHideFooter zBodyClass="!p-0 flex h-full min-h-0 flex-col" zScrollMode="inner" zWidth="min(320px, 86vw)">
    <!-- The drawer panel is the full viewport height: fill it, so the session list gets the room between header and footer. -->
    <div class="flex h-screen min-h-0 w-full flex-col">
      <Sidebar />
    </div>
  </ZDrawer>

  <LinkMenu />
  <ThemePanel />
  <ToolsModal />
  <WindowControls />
</div>
