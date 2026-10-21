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
    const move = (moved: PointerEvent) => {
      win.x = Math.min(Math.max(0, moved.clientX - grabX), window.innerWidth - 120);
      win.y = Math.min(Math.max(0, moved.clientY - grabY), window.innerHeight - 40);
      showTarget(moved.clientX, moved.clientY);
    };
    const up = (released: PointerEvent) => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
      const target = targetAt(released.clientX, released.clientY);
      if (target) dockTool(win.tool, target.zone);
      layout.dragging = false;
      layout.dropZone = '';
      layout.dropLine = null;
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
  }

  // Bottom-right corner: drag to resize that window only.
  function startResize(event: PointerEvent, win: FloatingTool) {
    event.preventDefault();
    event.stopPropagation();
    const startX = event.clientX;
    const startY = event.clientY;
    const startW = win.w ?? 560;
    const startH = win.h ?? 420;
    const move = (moved: PointerEvent) => {
      win.w = Math.max(MIN_W, Math.min(startW + moved.clientX - startX, window.innerWidth - win.x));
      win.h = Math.max(MIN_H, Math.min(startH + moved.clientY - startY, window.innerHeight - win.y));
    };
    const up = () => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
  }
</script>

<!-- Floating tool windows: each one is draggable, not modal, and holds its own tool. Dropping one on the
     right panel docks it there; its dock and close buttons do the same. -->
{#each layout.floating as win (win.tool)}
  {@const current = toolOf(win.tool)}
  <div
    class="fixed z-50 flex flex-col overflow-hidden rounded-md border border-border bg-background text-foreground shadow-2xl"
    style:left="{win.x}px"
    style:top="{win.y}px"
    style:width="{win.w ?? 560}px"
    style:height="{win.h ?? 420}px"
    role="dialog"
    aria-label={current.label}
  >
    <div class="flex h-10 shrink-0 cursor-grab items-center gap-1 border-b border-border pr-1 pl-2 select-none active:cursor-grabbing" role="presentation" onpointerdown={(event) => startDrag(event, win)}>
      <span class="grid size-6 place-items-center rounded-md bg-primary/15 text-primary" aria-hidden="true">
        <ZIcon zType={current.icon} class="size-3.5" />
      </span>
      <span class="truncate text-xs font-medium">{current.label}</span>
      <span class="ml-auto"></span>
      <!-- Drag handle: press and move to carry the window; release over a zone to dock it there. -->
      <span
        class="grid h-7 w-6 cursor-grab place-items-center rounded-md text-muted-foreground hover:bg-primary/10 hover:text-primary active:cursor-grabbing"
        role="presentation"
        title="Drag to move or dock"
        onpointerdown={(event) => startDrag(event, win)}
      >
        <ZIcon zType="lucideGripVertical" class="size-3.5" />
      </span>
      <ZButton zWave={false} zType="ghost" zSize="sm" class="h-7 w-8 px-0" aria-label="Dock back" title="Dock back" onclick={() => dockTool(win.tool)}>
        <ZIcon zType="lucidePanelRightOpen" class="size-3.5" />
      </ZButton>
      <ZButton zWave={false} zType="ghost" zSize="sm" class="h-7 w-8 px-0" aria-label="Close" title="Close" onclick={() => dockTool(win.tool)}>
        <ZIcon zType="lucideX" class="size-3.5" />
      </ZButton>
    </div>
    <div class="flex min-h-0 flex-1 flex-col">
      <ToolView tool={win.tool} />
    </div>
    <div
      class="absolute right-0 bottom-0 size-4 cursor-nwse-resize"
      role="presentation"
      aria-hidden="true"
      onpointerdown={(event) => startResize(event, win)}
    ></div>
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
