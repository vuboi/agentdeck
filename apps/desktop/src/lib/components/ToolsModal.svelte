<script lang="ts">
  import { ZButton, ZIcon } from '@shival99/z-ui-svelte';
  import { dockTool, layout, type FloatingTool, type Zone } from '../layout.svelte';
  import { toolOf } from '../tools';
  import ToolView from './ToolView.svelte';

  const MIN_W = 320;
  const MIN_H = 220;
  const EDGE = 56; // px band along each workspace edge that counts as "drop on this edge"

  type Line = { x: number; y: number; w: number; h: number };
  type Target = { zone: Zone; line: Line };

  const inside = (x: number, y: number, box: DOMRect) => x >= box.left && x <= box.right && y >= box.top && y <= box.bottom;

  // Where a released window would land, and the line that shows it. Only edges are targets, never areas:
  // the workspace's left/right/top/bottom bands, the tools panel, and the lower part of the sidebar.
  function targetAt(x: number, y: number): Target | null {
    const tools = document.querySelector<HTMLElement>('[aria-label="Tools panel"]')?.getBoundingClientRect();
    if (tools && inside(x, y, tools)) return { zone: 'tools', line: { x: tools.left, y: tools.top, w: 2, h: tools.height } };

    const sidebar = document.querySelector<HTMLElement>('[data-sidebar]')?.getBoundingClientRect();
    if (sidebar && inside(x, y, sidebar) && y > sidebar.top + sidebar.height * 0.6) {
      return { zone: 'sidebar', line: { x: sidebar.left, y: sidebar.bottom - 44, w: sidebar.width, h: 2 } };
    }

    const ws = document.querySelector<HTMLElement>('[data-workspace]')?.getBoundingClientRect();
    if (!ws || !inside(x, y, ws)) return null;
    if (x - ws.left < EDGE) return { zone: 'left', line: { x: ws.left, y: ws.top, w: 2, h: ws.height } };
    if (ws.right - x < EDGE) return { zone: 'right', line: { x: ws.right - 2, y: ws.top, w: 2, h: ws.height } };
    if (y - ws.top < EDGE) return { zone: 'top', line: { x: ws.left, y: ws.top, w: ws.width, h: 2 } };
    const terminal = document.querySelector<HTMLElement>('[data-terminal]')?.getBoundingClientRect();
    const bottomY = terminal ? terminal.top : ws.bottom;
    if (bottomY - y < EDGE) return { zone: 'bottom', line: { x: ws.left, y: bottomY - 2, w: ws.width, h: 2 } };
    return null;
  }

  function showTarget(x: number, y: number) {
    const target = targetAt(x, y);
    layout.dropZone = target?.zone ?? '';
    layout.dropLine = target?.line ?? null;
  }

  // Dragging by the header moves that window only; the main window stays usable (no backdrop).
  function startDrag(event: PointerEvent, win: FloatingTool) {
    if ((event.target as HTMLElement).closest('button')) return;
    event.preventDefault();
    beginDrag(win, event.clientX - win.x, event.clientY - win.y);
  }

  // A tool pulled out of a zone: its window starts the drag at once.
  $effect(() => {
    const request = layout.dragRequest;
    if (!request) return;
    const win = layout.floating.find((item) => item.tool === request.tool);
    layout.dragRequest = null;
    if (win) beginDrag(win, request.x - win.x, request.y - win.y);
  });

  function beginDrag(win: FloatingTool, grabX: number, grabY: number) {
    layout.dragging = true;
    // While it is carried the window is small, so the drop targets stay in view; it returns to its size when let go.
    const original = { w: win.w ?? 560, h: win.h ?? 420 };
    const small = { w: 280, h: 180 };
    const wasMax = win.max;
    win.min = false;
    win.max = false;
    win.w = small.w;
    win.h = small.h;
    const gx = Math.min(grabX, small.w - 24);
    const gy = Math.min(grabY, small.h / 2);
    const move = (moved: PointerEvent) => {
      win.x = Math.min(Math.max(0, moved.clientX - gx), window.innerWidth - 120);
      win.y = Math.min(Math.max(0, moved.clientY - gy), window.innerHeight - 40);
      showTarget(moved.clientX, moved.clientY);
    };
    const up = (released: PointerEvent) => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
      const target = targetAt(released.clientX, released.clientY);
      if (target) dockTool(win.tool, target.zone);
      else {
        win.w = Math.min(original.w, window.innerWidth - win.x);
        win.h = Math.min(original.h, window.innerHeight - win.y);
        if (wasMax) toggleMax(win);
      }
      layout.dragging = false;
      layout.dropZone = '';
      layout.dropLine = null;
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
  }

  // Any edge or corner: drag to resize that window only.
  type Edge = 'n' | 's' | 'e' | 'w' | 'ne' | 'nw' | 'se' | 'sw';
  function startResize(event: PointerEvent, win: FloatingTool, edge: Edge) {
    event.preventDefault();
    event.stopPropagation();
    layout.dragging = true;
    const start = { x: event.clientX, y: event.clientY, left: win.x, top: win.y, w: win.w ?? 560, h: win.h ?? 420 };
    const move = (moved: PointerEvent) => {
      const dx = moved.clientX - start.x;
      const dy = moved.clientY - start.y;
      let { left, top, w, h } = start;
      if (edge.includes('e')) w = Math.max(MIN_W, Math.min(start.w + dx, window.innerWidth - start.left));
      if (edge.includes('s')) h = Math.max(MIN_H, Math.min(start.h + dy, window.innerHeight - start.top));
      if (edge.includes('w')) {
        w = Math.max(MIN_W, start.w - dx);
        left = Math.max(0, start.left + start.w - w);
        w = start.left + start.w - left;
      }
      if (edge.includes('n')) {
        h = Math.max(MIN_H, start.h - dy);
        top = Math.max(0, start.top + start.h - h);
        h = start.top + start.h - top;
      }
      Object.assign(win, { x: left, y: top, w, h });
    };
    const up = () => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
      layout.dragging = false;
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
  }

  function toggleMax(win: FloatingTool) {
    if (win.max && win.prev) {
      Object.assign(win, win.prev, { max: false });
      win.prev = undefined;
      return;
    }
    win.prev = { x: win.x, y: win.y, w: win.w ?? 560, h: win.h ?? 420 };
    Object.assign(win, { x: 8, y: 8, w: window.innerWidth - 16, h: window.innerHeight - 16, max: true, min: false });
  }

  const toggleMin = (win: FloatingTool) => (win.min = !win.min);

  const EDGES: { edge: Edge; cls: string }[] = [
    { edge: 'n', cls: 'top-0 right-3 left-3 h-1 cursor-ns-resize' },
    { edge: 's', cls: 'bottom-0 right-3 left-3 h-1 cursor-ns-resize' },
    { edge: 'w', cls: 'top-3 bottom-3 left-0 w-1 cursor-ew-resize' },
    { edge: 'e', cls: 'top-3 right-0 bottom-3 w-1 cursor-ew-resize' },
    { edge: 'nw', cls: 'top-0 left-0 size-3 cursor-nwse-resize' },
    { edge: 'ne', cls: 'top-0 right-0 size-3 cursor-nesw-resize' },
    { edge: 'sw', cls: 'bottom-0 left-0 size-3 cursor-nesw-resize' },
    { edge: 'se', cls: 'right-0 bottom-0 size-3 cursor-nwse-resize' },
  ];
  const headerButton =
    'grid size-7! shrink-0 cursor-pointer place-items-center rounded-md p-0 text-muted-foreground outline-none !bg-transparent hover:!bg-primary/10 hover:!text-primary focus-visible:ring-0';
</script>

<!-- Floating tool windows: each one is draggable, not modal, and holds its own tool. Dropping one on the
     right panel docks it there; its dock and close buttons do the same. -->
{#each layout.floating as win (win.tool)}
  {@const current = toolOf(win.tool)}
  <div
    class="fixed z-50 flex flex-col overflow-hidden rounded-md border border-border bg-background text-foreground shadow-2xl transition-[width,height] duration-150 {layout.dragging ? 'transition-none' : ''}"
    style:left="{win.x}px"
    style:top="{win.y}px"
    style:width="{win.w ?? 560}px"
    style:height={win.min ? '40px' : `${win.h ?? 420}px`}
    role="dialog"
    aria-label={current.label}
  >
    <div class="flex h-10 shrink-0 cursor-grab items-center gap-1 border-b border-border pr-1.5 pl-2 select-none active:cursor-grabbing" role="presentation" onpointerdown={(event) => startDrag(event, win)} ondblclick={() => toggleMax(win)}>
      <span class="grid size-6 place-items-center rounded-md bg-primary/15 text-primary" aria-hidden="true">
        <ZIcon zType={current.icon} class="size-3.5" />
      </span>
      <span class="truncate text-xs font-medium">{current.label}</span>
      <span class="ml-auto"></span>
      <!-- Drag handle: press and move to carry the window; release over a zone to dock it there. -->
      <span class="{headerButton} cursor-grab active:cursor-grabbing" role="presentation" title="Drag to move or dock" onpointerdown={(event) => startDrag(event, win)}>
        <ZIcon zType="lucideGripVertical" class="size-4" />
      </span>
      <ZButton zWave={false} zType="ghost" zSize="sm" class={headerButton} aria-label={win.min ? 'Restore' : 'Minimize'} title={win.min ? 'Restore' : 'Minimize'} onclick={() => toggleMin(win)}>
        <ZIcon zType="lucideMinus" class="size-4" />
      </ZButton>
      <ZButton zWave={false} zType="ghost" zSize="sm" class={headerButton} aria-label={win.max ? 'Restore size' : 'Maximize'} title={win.max ? 'Restore size' : 'Maximize'} onclick={() => toggleMax(win)}>
        <ZIcon zType={win.max ? 'lucideMinimize2' : 'lucideMaximize2'} class="size-3.5" />
      </ZButton>
      <ZButton zWave={false} zType="ghost" zSize="sm" class={headerButton} aria-label="Dock back" title="Dock back" onclick={() => dockTool(win.tool)}>
        <ZIcon zType="lucidePanelRightOpen" class="size-4" />
      </ZButton>
      <ZButton zWave={false} zType="ghost" zSize="sm" class={headerButton} aria-label="Close" title="Close" onclick={() => dockTool(win.tool)}>
        <ZIcon zType="lucideX" class="size-4" />
      </ZButton>
    </div>
    {#if !win.min}
      <div class="flex min-h-0 flex-1 flex-col">
        <ToolView tool={win.tool} />
      </div>
      {#each EDGES as item (item.edge)}
        <div class="absolute z-10 {item.cls}" role="presentation" aria-hidden="true" onpointerdown={(event) => startResize(event, win, item.edge)}></div>
      {/each}
    {/if}
  </div>
{/each}

<!-- Where the window will land: a thin primary line on the edge, nothing else. -->
{#if layout.dropLine}
  <div
    class="pointer-events-none fixed z-40 rounded-full bg-primary shadow-[0_0_10px_var(--primary)]"
    style:left="{layout.dropLine.x}px"
    style:top="{layout.dropLine.y}px"
    style:width="{layout.dropLine.w}px"
    style:height="{layout.dropLine.h}px"
  ></div>
{/if}
