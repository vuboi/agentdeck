<script lang="ts">
  import { untrack } from 'svelte';
  import { ZIcon, zTooltip } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import type { PickedElement } from '../browser.svelte';

  // A frozen screenshot of the page to mark up. Click an element to comment on it, or draw: line, box, arrow, text.
  // The toolbar floats at the bottom; the check mark hands the marked-up PNG and the notes to the chat.
  type Note = { n: number; text: string; x: number; y: number; selector?: string; html?: string; box?: PickedElement['box'] };
  let {
    shot,
    inspect,
    oncancel,
    ondone,
  }: {
    shot: { data: string; w: number; h: number };
    inspect: (x: number, y: number) => Promise<PickedElement | null>;
    oncancel: () => void;
    ondone: (png: string, notes: Note[]) => void;
  } = $props();

  type Tool = 'select' | 'pen' | 'shape' | 'text';
  type Shape =
    | { t: 'arrow' | 'rect'; color: string; x1: number; y1: number; x2: number; y2: number }
    | { t: 'pen'; color: string; pts: [number, number][] }
    | { t: 'hl'; color: string; x: number; y: number; w: number; h: number }
    | { t: 'note'; color: string; x: number; y: number; n: number; text: string; element?: PickedElement };

  const COLORS = ['#ef4444', '#f59e0b', '#22c55e', '#3b82f6'];

  let tool = $state<Tool>('select');
  let shapeKind = $state<'rect' | 'arrow'>('rect');
  let shapeMenu = $state(false);
  let color = $state(COLORS[0]);
  let shapes = $state<Shape[]>([]);
  let draft = $state<Shape | null>(null);
  let canvas: HTMLCanvasElement | undefined = $state();
  let noteAt = $state<{ x: number; y: number; px: number; py: number; element?: PickedElement } | null>(null);
  let noteText = $state('');
  let noteInput: HTMLInputElement | undefined = $state();
  let image: HTMLImageElement | undefined;
  let ready = $state(false);
  let status = $state('');

  const LINE = $derived(Math.max(3, Math.round(shot.w / 280)));

  $effect(() => {
    const img = new Image();
    img.onload = () => {
      image = img;
      ready = true;
    };
    img.src = shot.data;
  });

  function draw(context: CanvasRenderingContext2D, list: Shape[]) {
    context.clearRect(0, 0, shot.w, shot.h);
    if (image) context.drawImage(image, 0, 0, shot.w, shot.h);
    for (const shape of list) {
      context.strokeStyle = shape.color;
      context.fillStyle = shape.color;
      context.lineWidth = LINE;
      context.lineCap = 'round';
      context.lineJoin = 'round';
      if (shape.t === 'rect') {
        context.strokeRect(shape.x1, shape.y1, shape.x2 - shape.x1, shape.y2 - shape.y1);
      } else if (shape.t === 'hl') {
        context.globalAlpha = 0.14;
        context.fillRect(shape.x, shape.y, shape.w, shape.h);
        context.globalAlpha = 1;
        context.setLineDash([LINE * 2, LINE]);
        context.strokeRect(shape.x, shape.y, shape.w, shape.h);
        context.setLineDash([]);
      } else if (shape.t === 'arrow') {
        const angle = Math.atan2(shape.y2 - shape.y1, shape.x2 - shape.x1);
        const head = LINE * 5;
        context.beginPath();
        context.moveTo(shape.x1, shape.y1);
        context.lineTo(shape.x2, shape.y2);
        context.moveTo(shape.x2, shape.y2);
        context.lineTo(shape.x2 - head * Math.cos(angle - 0.45), shape.y2 - head * Math.sin(angle - 0.45));
        context.moveTo(shape.x2, shape.y2);
        context.lineTo(shape.x2 - head * Math.cos(angle + 0.45), shape.y2 - head * Math.sin(angle + 0.45));
        context.stroke();
      } else if (shape.t === 'pen') {
        context.beginPath();
        shape.pts.forEach(([x, y], index) => (index === 0 ? context.moveTo(x, y) : context.lineTo(x, y)));
        context.stroke();
      } else if (shape.t === 'note') {
        const radius = LINE * 4;
        context.beginPath();
        context.arc(shape.x, shape.y, radius, 0, Math.PI * 2);
        context.fill();
        context.fillStyle = '#fff';
        context.font = `bold ${Math.round(radius * 1.2)}px sans-serif`;
        context.textAlign = 'center';
        context.textBaseline = 'middle';
        context.fillText(String(shape.n), shape.x, shape.y + 1);
      }
    }
  }

  $effect(() => {
    const list = draft ? [...shapes, draft] : [...shapes];
    void ready;
    void list.length;
    const context = canvas?.getContext('2d');
    if (context) untrack(() => draw(context, list));
  });

  /** Pointer position in the screenshot's own pixels. */
  function at(event: PointerEvent): [number, number] {
    const rect = canvas!.getBoundingClientRect();
    return [((event.clientX - rect.left) / rect.width) * shot.w, ((event.clientY - rect.top) / rect.height) * shot.h];
  }

  function openNote(x: number, y: number, event: PointerEvent, element?: PickedElement) {
    const rect = canvas!.getBoundingClientRect();
    noteAt = { x, y, px: event.clientX - rect.left, py: event.clientY - rect.top, element };
    noteText = '';
    queueMicrotask(() => noteInput?.focus());
  }

  async function down(event: PointerEvent) {
    if (!ready || noteAt) return;
    shapeMenu = false;
    const [x, y] = at(event);
    if (tool === 'text') return openNote(x, y, event);
    if (tool === 'select') {
      status = 'Finding the element…';
      try {
        const element = await inspect(x, y);
        if (!element) return;
        draft = null;
        shapes = [...shapes, { t: 'hl', color, x: element.box.x, y: element.box.y, w: element.box.w, h: element.box.h }];
        openNote(element.box.x + element.box.w / 2, element.box.y + element.box.h / 2, event, element);
      } catch {
        // The page did not answer: the click is ignored.
      } finally {
        status = '';
      }
      return;
    }
    canvas!.setPointerCapture(event.pointerId);
    draft = tool === 'pen' ? { t: 'pen', color, pts: [[x, y]] } : { t: shapeKind, color, x1: x, y1: y, x2: x, y2: y };
  }

  function move(event: PointerEvent) {
    if (!draft) return;
    const [x, y] = at(event);
    if (draft.t === 'pen') draft.pts.push([x, y]);
    else if (draft.t === 'rect' || draft.t === 'arrow') ((draft.x2 = x), (draft.y2 = y));
    draft = { ...draft } as Shape;
  }

  function up() {
    if (!draft) return;
    const tiny = (draft.t === 'rect' || draft.t === 'arrow') && Math.hypot(draft.x2 - draft.x1, draft.y2 - draft.y1) < 6;
    if (!tiny) shapes = [...shapes, draft];
    draft = null;
  }

  function commitNote() {
    const text = noteText.trim();
    if (noteAt) {
      if (text) {
        const n = shapes.filter((shape) => shape.t === 'note').length + 1;
        shapes = [...shapes, { t: 'note', color, x: noteAt.x, y: noteAt.y, n, text, element: noteAt.element }];
      } else if (noteAt.element) {
        // An element with no comment leaves no mark behind.
        shapes = shapes.slice(0, -1);
      }
    }
    noteAt = null;
  }

  const notes = $derived<Note[]>(
    shapes.flatMap((shape) =>
      shape.t === 'note'
        ? [{ n: shape.n, text: shape.text, x: Math.round(shape.x), y: Math.round(shape.y), selector: shape.element?.selector, html: shape.element?.html, box: shape.element?.box }]
        : [],
    ),
  );

  function done() {
    if (!canvas) return;
    const context = canvas.getContext('2d');
    if (context) draw(context, shapes);
    ondone(canvas.toDataURL('image/png'), notes);
  }

  function onKey(event: KeyboardEvent) {
    if (event.key === 'Escape') (noteAt ? (noteAt = null) : oncancel());
    else if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'z') shapes = shapes.slice(0, -1);
  }

  const button = (active: boolean) =>
    `grid size-8 cursor-pointer place-items-center rounded-lg outline-none ${active ? 'bg-primary/15 text-primary' : 'text-muted-foreground hover:bg-primary/10 hover:text-primary'}`;
</script>

<svelte:window onkeydown={onKey} />

<div class="relative flex h-full min-h-0 flex-col bg-muted/30" role="dialog" aria-label="Annotate screenshot">
  <div class="relative grid min-h-0 flex-1 place-items-center overflow-auto p-3 pb-20">
    <div class="relative max-h-full max-w-full" style:aspect-ratio="{shot.w} / {shot.h}" style:width="min(100%, {shot.w}px)">
      <canvas
        bind:this={canvas}
        width={shot.w}
        height={shot.h}
        class="block h-full w-full touch-none rounded-md border border-border bg-background shadow-sm {tool === 'text' ? 'cursor-text' : tool === 'select' ? 'cursor-pointer' : 'cursor-crosshair'}"
        onpointerdown={down}
        onpointermove={move}
        onpointerup={up}
      ></canvas>
      {#if noteAt}
        <div class="absolute z-10 flex w-64 flex-col gap-1 rounded-lg border border-border bg-popover p-1.5 text-xs shadow-md" style:left="{noteAt.px}px" style:top="{noteAt.py + 8}px">
          {#if noteAt.element}<code class="truncate px-1 text-[10px] text-muted-foreground" title={noteAt.element.selector}>{noteAt.element.selector}</code>{/if}
          <input
            bind:this={noteInput}
            bind:value={noteText}
            class="h-7 rounded-md border border-input bg-background px-2 outline-none focus:border-primary"
            placeholder={noteAt.element ? 'Comment on this element…' : 'Note for the agent…'}
            onkeydown={(event) => {
              if (event.key === 'Enter') commitNote();
              event.stopPropagation();
              if (event.key === 'Escape') noteAt = null;
            }}
          />
        </div>
      {/if}
    </div>
  </div>

  {#if status}<div class="absolute top-2 left-1/2 -translate-x-1/2 rounded-md bg-popover px-2 py-1 text-xs text-muted-foreground shadow" role="status">{status}</div>{/if}

  <!-- The floating toolbar: select, pen, shape, text | colour, undo | done, close. -->
  <div class="absolute bottom-4 left-1/2 flex -translate-x-1/2 items-center gap-0.5 rounded-xl border border-border bg-popover p-1 shadow-lg">
    <button type="button" class={button(tool === 'select')} aria-label="Select an element" aria-pressed={tool === 'select'} {@attach zTooltip({ content: 'Select an element and comment on it', position: 'top' })} onclick={() => (tool = 'select')}>
      <ZIcon zType={'lucideMousePointer2' as ZIconName} class="size-4" />
    </button>
    <button type="button" class={button(tool === 'pen')} aria-label="Draw a line" aria-pressed={tool === 'pen'} {@attach zTooltip({ content: 'Draw a line', position: 'top' })} onclick={() => (tool = 'pen')}>
      <ZIcon zType={'lucidePencil' as ZIconName} class="size-4" />
    </button>
    <div class="relative flex items-center">
      <button type="button" class={button(tool === 'shape')} aria-label="Shape" aria-pressed={tool === 'shape'} {@attach zTooltip({ content: shapeKind === 'rect' ? 'Draw a box' : 'Draw an arrow', position: 'top' })} onclick={() => (tool = 'shape')}>
        <ZIcon zType={(shapeKind === 'rect' ? 'lucideSquare' : 'lucideMoveUpRight') as ZIconName} class="size-4" />
      </button>
      <button type="button" class="grid h-8 w-4 cursor-pointer place-items-center rounded-lg text-muted-foreground outline-none hover:bg-primary/10 hover:text-primary" aria-label="Shape type" aria-haspopup="menu" aria-expanded={shapeMenu} onclick={() => (shapeMenu = !shapeMenu)}>
        <ZIcon zType="lucideChevronDown" class="size-3" />
      </button>
      {#if shapeMenu}
        <div class="absolute bottom-10 left-0 flex flex-col rounded-lg border border-border bg-popover p-1 text-xs shadow-md" role="menu">
          {#each [['rect', 'Box', 'lucideSquare'], ['arrow', 'Arrow', 'lucideMoveUpRight']] as [value, label, icon] (value)}
            <button type="button" role="menuitem" class="flex h-7 cursor-pointer items-center gap-2 rounded-md px-2 hover:bg-primary/10 {shapeKind === value ? 'text-primary' : ''}" onclick={() => ((shapeKind = value as 'rect' | 'arrow'), (tool = 'shape'), (shapeMenu = false))}>
              <ZIcon zType={icon as ZIconName} class="size-3.5" />{label}
            </button>
          {/each}
        </div>
      {/if}
    </div>
    <button type="button" class={button(tool === 'text')} aria-label="Add a note" aria-pressed={tool === 'text'} {@attach zTooltip({ content: 'Add a note', position: 'top' })} onclick={() => (tool = 'text')}>
      <ZIcon zType={'lucideType' as ZIconName} class="size-4" />
    </button>
    <span class="mx-1 h-5 w-px bg-border" aria-hidden="true"></span>
    <button type="button" class={button(false)} aria-label="Colour" {@attach zTooltip({ content: 'Colour', position: 'top' })} onclick={() => (color = COLORS[(COLORS.indexOf(color) + 1) % COLORS.length])}>
      <span class="size-4 rounded-full border-2 border-background ring-1 ring-border" style:background={color}></span>
    </button>
    <button type="button" class={button(false)} aria-label="Undo" disabled={shapes.length === 0} {@attach zTooltip({ content: 'Undo (Ctrl+Z)', position: 'top' })} onclick={() => (shapes = shapes.slice(0, -1))}>
      <ZIcon zType={'lucideUndo2' as ZIconName} class="size-4" />
    </button>
    <span class="mx-1 h-5 w-px bg-border" aria-hidden="true"></span>
    <button type="button" class="grid size-8 cursor-pointer place-items-center rounded-lg bg-primary text-primary-foreground outline-none hover:bg-primary/90" aria-label="Add to chat" {@attach zTooltip({ content: 'Add to chat', position: 'top' })} onclick={done}>
      <ZIcon zType="lucideCheck" class="size-4" />
    </button>
    <button type="button" class={button(false)} aria-label="Close" {@attach zTooltip({ content: 'Close', position: 'top' })} onclick={oncancel}>
      <ZIcon zType="lucideX" class="size-4" />
    </button>
  </div>
</div>
