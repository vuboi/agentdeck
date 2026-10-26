import { getPref, setPref } from './prefs';
import { TOOLS } from './tools';

// Shared shell state. The page reacts to `toolsOpen` by resizing the columns.
// `themeAnchor` is the palette button on screen, so the theme popover has a position the moment it opens.
export type FloatingTool = {
  tool: string;
  x: number;
  y: number;
  w?: number;
  h?: number;
  /** Fills the screen; `prev` is where to go back to. */
  max?: boolean;
  /** Folded to its title bar. */
  min?: boolean;
  prev?: { x: number; y: number; w: number; h: number };
};

export const layout = $state({
  toolsOpen: true,
  themePanelOpen: false,
  themeAnchor: null as HTMLElement | null,
  /** Tool shown in the right panel. */
  activeTool: 'Explore',
  /** Tools torn out into their own floating windows. Each one is independent of the right panel. */
  floating: [] as FloatingTool[],
  /** Tools docked around the workspace: under the sidebar list, on the four sides of the conversation, and under the tools panel's own tool. */
  docked: { sidebar: [] as string[], left: [] as string[], right: [] as string[], top: [] as string[], bottom: [] as string[], toolsBottom: [] as string[] },
  /** Pane sizes in px. Dock panes and the terminal are resized with splitters and kept across reloads. */
  sizes: { sidebar: 260, sidebarZone: 176, toolsBottom: 240, tools: 360, left: 320, right: 320, top: 240, bottom: 240, terminal: 240 },
  /** A docked tool was grabbed: the floating window picks the drag up at this pointer position. */
  dragRequest: null as null | { tool: string; x: number; y: number },
  /** A floating window is being dragged; zones show their drop outline while it is. */
  dragging: false,
  /** Zone under the pointer while dragging, and the thin line that shows where the tool will land. */
  dropZone: '' as '' | 'tools' | DockZone,
  dropLine: null as null | { x: number; y: number; w: number; h: number },
  /** Narrow window: the sidebar is not in the layout and opens as a floating drawer instead. */
  sidebarHidden: false,
  /** The tools panel is on screen (open and wide enough). Decides how much room the window controls need. */
  toolsVisible: false,
  sidebarDrawer: false,
  /** The user folded the sidebar away (wide window). Kept between runs. */
  sidebarCollapsed: false,
  /** Keep the sidebar instance mounted while folded so its tree state survives. */
  sidebarWidthAnimating: false,
  /** Keep the tools panel mounted while its desktop slide animation runs. */
  toolsWidthAnimating: false,
});

/** Shared expansion state for the desktop sidebar and its narrow-window drawer. */
export const sidebarTree = $state({
  expanded: [] as string[],
  hydrated: false,
  hasSavedState: false,
  initialized: false,
});

getPref<string[]>('sidebar.treeExpanded').then((saved) => {
  if (Array.isArray(saved)) {
    sidebarTree.expanded = saved;
    sidebarTree.hasSavedState = true;
  }
  sidebarTree.hydrated = true;
});

$effect.root(() => {
  $effect(() => {
    const initialized = sidebarTree.initialized;
    const expanded = [...sidebarTree.expanded];
    if (initialized) setPref('sidebar.treeExpanded', expanded);
  });
});

export type DockZone = 'sidebar' | 'left' | 'right' | 'top' | 'bottom' | 'toolsBottom';
export type Zone = 'tools' | DockZone;

// Floating windows and their positions survive a reload. Hydrated once, saved on every change.
let floatingHydrated = false;
getPref<{ floating: FloatingTool[]; activeTool: string; docked?: typeof layout.docked }>('layout.floating').then((saved) => {
  if (saved) {
    // Audit was folded into Tasks: a saved Audit window, tab or dock becomes Tasks.
    // Tools that were merged into another keep working from a saved layout.
    const renamed = (tool: string) => (tool === 'Audit' ? 'Tasks' : tool === 'Diff' || tool === 'Files' ? 'Explore' : tool);
    layout.floating = (saved.floating ?? []).map((win) => ({ ...win, tool: renamed(win.tool) }));
    layout.activeTool = renamed(saved.activeTool ?? layout.activeTool);
    if (saved.docked) {
      const docked = structuredClone(saved.docked);
      for (const zone of Object.keys(docked) as (keyof typeof docked)[]) docked[zone] = [...new Set(docked[zone].map(renamed))];
      layout.docked = { ...layout.docked, ...docked };
    }
  }
  floatingHydrated = true;
});

let collapsedHydrated = false;
getPref<boolean>('layout.sidebarCollapsed').then((saved) => {
  if (typeof saved === 'boolean') layout.sidebarCollapsed = saved;
  collapsedHydrated = true;
});
$effect.root(() => {
  $effect(() => {
    const collapsed = layout.sidebarCollapsed;
    if (collapsedHydrated) setPref('layout.sidebarCollapsed', collapsed);
  });
});

let sidebarAnimationTimer: ReturnType<typeof setTimeout> | undefined;
/** Fold or restore the desktop sidebar with a short slide animation. */
export function setSidebarCollapsed(collapsed: boolean): void {
  if (layout.sidebarCollapsed === collapsed) return;
  clearTimeout(sidebarAnimationTimer);
  layout.sidebarWidthAnimating = true;
  layout.sidebarCollapsed = collapsed;
  sidebarAnimationTimer = setTimeout(() => (layout.sidebarWidthAnimating = false), 260);
}

// ---------- the sidebar peek: the folded sidebar shown floating over the app, as Claude does ----------
// `layout.sidebarDrawer` is whether it is on screen. Hover opens it (the folded sidebar's button, after a short wait)
// and leaving both that button and the panel puts it away; in a narrow window a click opens it instead.
let peekTimer: ReturnType<typeof setTimeout> | undefined;
/** Shown at once when set: pinning puts the docked sidebar where the peek was, with no exit animation in between. */
export const peekMotion = $state({ instant: false });

export function peekSidebar(delay = 0) {
  clearTimeout(peekTimer);
  if (delay === 0) layout.sidebarDrawer = true;
  else peekTimer = setTimeout(() => (layout.sidebarDrawer = true), delay);
}
/** The pointer left the button or the panel: put it away unless it comes back in a moment. */
export function leavePeek() {
  clearTimeout(peekTimer);
  peekTimer = setTimeout(() => (layout.sidebarDrawer = false), 280);
}
/** The pointer is back over the button or the panel: keep it. */
export function holdPeek() {
  clearTimeout(peekTimer);
}
export function closePeek() {
  clearTimeout(peekTimer);
  layout.sidebarDrawer = false;
}
/** Pin the folded sidebar back into the layout. From the peek, it takes the peek's place and width with no animation. */
export function pinSidebar() {
  clearTimeout(peekTimer);
  if (layout.sidebarDrawer && layout.sidebarCollapsed) {
    peekMotion.instant = true;
    layout.sidebarDrawer = false;
    layout.sidebarCollapsed = false;
    requestAnimationFrame(() => (peekMotion.instant = false));
    return;
  }
  layout.sidebarDrawer = false;
  setSidebarCollapsed(false);
}

let sizesHydrated = false;
getPref<typeof layout.sizes>('layout.sizes').then((saved) => {
  if (saved) layout.sizes = { ...layout.sizes, ...saved };
  sizesHydrated = true;
});
$effect.root(() => {
  $effect(() => {
    const snapshot = { ...layout.sizes };
    if (sizesHydrated) setPref('layout.sizes', snapshot);
  });
});
$effect.root(() => {
  $effect(() => {
    const snapshot = {
      floating: layout.floating.map((win) => ({ ...win })),
      activeTool: layout.activeTool,
      docked: structuredClone($state.snapshot(layout.docked)),
    };
    if (floatingHydrated) setPref('layout.floating', snapshot);
  });
});

// Restore the tools panel state saved last session, then keep saving it.
let layoutHydrated = false;
getPref<boolean>('layout.toolsOpen').then((saved) => {
  if (typeof saved === 'boolean') layout.toolsOpen = saved;
  layoutHydrated = true;
});
$effect.root(() => {
  $effect(() => {
    const open = layout.toolsOpen;
    if (layoutHydrated) setPref('layout.toolsOpen', open);
  });
});

let toolsAnimationTimer: ReturnType<typeof setTimeout> | undefined;
/** Open or fold the tools panel with a short slide animation. */
export function setToolsOpen(open: boolean): void {
  if (layout.toolsOpen === open) return;
  clearTimeout(toolsAnimationTimer);
  layout.toolsWidthAnimating = true;
  layout.toolsOpen = open;
  toolsAnimationTimer = setTimeout(() => (layout.toolsWidthAnimating = false), 260);
}

export function toggleTools() {
  setToolsOpen(!layout.toolsOpen);
}

export function toggleThemePanel() {
  layout.themePanelOpen = !layout.themePanelOpen;
}

export const isFloating = (tool: string) => layout.floating.some((win) => win.tool === tool);
/** A tool that is not in the tools panel because it floats or sits in a dock zone. */
export const isDocked = (tool: string) => Object.values(layout.docked).some((list) => list.includes(tool));
export const isAway = (tool: string) => isFloating(tool) || isDocked(tool);

/** Tear the active tool out into its own window. Each tool floats at most once. */
export function detachTools() {
  const tool = layout.activeTool;
  if (isFloating(tool)) return;
  const offset = layout.floating.length * 24;
  layout.floating.push({ tool, x: Math.max(16, window.innerWidth - 600) - offset, y: 72 + offset, w: 560, h: 420 });
}

/** Where a tool lives: its own window, the tools panel, or one of the docked zones. A tool sits in one place. */
function removeFromZones(tool: string) {
  for (const zone of Object.keys(layout.docked) as DockZone[]) {
    layout.docked[zone] = layout.docked[zone].filter((item) => item !== tool);
  }
}

/** Pull a docked tool out of its zone into a floating window under the pointer, and keep dragging it. */
export function floatFromZone(tool: string, x: number, y: number) {
  removeFromZones(tool);
  layout.floating = layout.floating.filter((win) => win.tool !== tool);
  layout.floating.push({ tool, x: x - 40, y: y - 16, w: 560, h: 420 });
  layout.dragRequest = { tool, x, y };
}

/** The developer tools sit in the bottom zone, under the conversation: a fixed place that never resizes the page (a native layer). */
export const DEVTOOLS = 'DevTools';
export const devtoolsOpen = () => isAway(DEVTOOLS);
export function setDevtools(open: boolean) {
  if (!open) {
    layout.floating = layout.floating.filter((win) => win.tool !== DEVTOOLS);
    removeFromZones(DEVTOOLS);
  } else if (!isAway(DEVTOOLS)) {
    layout.docked.bottom.push(DEVTOOLS);
  }
}

export function dockTool(tool: string, zone: Zone = 'tools') {
  // The developer tools are not a panel tool: "dock back" and "close" both just close them.
  if (tool === DEVTOOLS && zone === 'tools') return setDevtools(false);
  layout.floating = layout.floating.filter((win) => win.tool !== tool);
  removeFromZones(tool);
  if (zone === 'tools') {
    layout.activeTool = tool;
    setToolsOpen(true);
  } else {
    layout.docked[zone].push(tool);
    // Under the tools panel's own tool: the panel above shows another tool, not "open elsewhere".
    if (zone === 'toolsBottom' && layout.activeTool === tool) layout.activeTool = TOOLS.find((item) => !isAway(item.value))?.value ?? tool;
  }
}

/** Take a zone tool back to the tools panel. */
export function undockTool(tool: string) {
  dockTool(tool, 'tools');
}
