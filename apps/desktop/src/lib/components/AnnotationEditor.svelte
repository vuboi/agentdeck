<script lang="ts">
  import { untrack } from 'svelte';
  import { ZButton, ZIcon } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';

  // A frozen screenshot of the page to mark up: arrows, boxes, freehand lines and numbered notes.
  // "Add to chat" gives back the marked-up PNG and the notes, so the agent sees what you point at.
  let {
    shot,
    oncancel,
    ondone,
  }: {
    shot: { data: string; w: number; h: number };
    oncancel: () => void;
    ondone: (png: string, notes: { n: number; text: string; x: number; y: number }[]) => void;
  } = $props();

  type Tool = 'arrow' | 'rect' | 'pen' | 'note';
  type Shape =
    | { t: 'arrow' | 'rect'; color: string; x1: number; y1: number; x2: number; y2: number }
    | { t: 'pen'; color: string; pts: [number, number][] }
    | { t: 'note'; color: string; x: number; y: number; n: number; text: string };

  const TOOLS: { value: Tool; icon: ZIconName; label: string }[] = [
    { value: 'arrow', icon: 'lucideMoveUpRight' as ZIconName, label: 'Arrow' },
    { value: 'rect', icon: 'lucideSquare' as ZIconName, label: 'Box' },
    { value: 'pen', icon: 'lucidePenLine' as ZIconName, label: 'Line' },
    { value: 'note', icon: 'lucideMessageSquarePlus' as ZIconName, label: 'Note' },
  ];
  const COLORS = ['#ef4444', '#f59e0b', '#22c55e', '#3b82f6'];

  let tool = $state<Tool>('arrow');
  let color = $state(COLORS[0]);
  let shapes = $state<Shape[]>([]);
  let draft = $state<Shape | null>(null);
  let canvas: HTMLCanvasElement | undefined = $state();
  let noteAt = $state<{ x: number; y: number; px: number; py: number } | null>(null);
  let noteText = $state('');
  let noteInput: HTMLInputElement | undefined = $state();
  let image: HTMLImageElement | undefined;
  let ready = $state(false);

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

  function down(event: PointerEvent) {
    if (!ready || noteAt) return;
    const [x, y] = at(event);
    if (tool === 'note') {
      const rect = canvas!.getBoundingClientRect();
      noteAt = { x, y, px: event.clientX - rect.left, py: event.clientY - rect.top };
      noteText = '';
      queueMicrotask(() => noteInput?.focus());
      return;
    }
    canvas!.setPointerCapture(event.pointerId);
    draft = tool === 'pen' ? { t: 'pen', color, pts: [[x, y]] } : { t: tool, color, x1: x, y1: y, x2: x, y2: y };
  }

  function move(event: PointerEvent) {
    if (!draft) return;
    const [x, y] = at(event);
    if (draft.t === 'pen') draft.pts.push([x, y]);
    else if (draft.t !== 'note') ((draft.x2 = x), (draft.y2 = y));
    draft = { ...draft } as Shape;
  }

  function up() {
    if (!draft) return;
    const tiny = draft.t !== 'pen' && draft.t !== 'note' && Math.hypot(draft.x2 - draft.x1, draft.y2 - draft.y1) < 6;
    if (!tiny) shapes = [...shapes, draft];
    draft = null;
  }

  function commitNote() {
    const text = noteText.trim();
    if (noteAt && text) {
      const n = shapes.filter((shape) => shape.t === 'note').length + 1;
      shapes = [...shapes, { t: 'note', color, x: noteAt.x, y: noteAt.y, n, text }];
    }
    noteAt = null;
  }

  const notes = $derived(shapes.flatMap((shape) => (shape.t === 'note' ? [{ n: shape.n, text: shape.text, x: Math.round(shape.x), y: Math.round(shape.y) }] : [])));

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
</script>

<svelte:window onkeydown={onKey} />

<div class="flex h-full min-h-0 flex-col bg-muted/30" role="dialog" aria-label="Annotate screenshot">
  <div class="flex shrink-0 flex-wrap items-center gap-1 border-b border-border bg-background px-2 py-1.5">
    {#each TOOLS as item (item.value)}
      <ZButton
        zWave={false}
        zType="ghost"
        zSize="sm"
        class="size-7! p-0 {tool === item.value ? 'bg-primary/15 text-primary' : 'text-muted-foreground'}"
        aria-label={item.label}
        aria-pressed={tool === item.value}
        title={item.label}
        onclick={() => (tool = item.value)}
      >
        <ZIcon zType={item.icon} class="size-4" />
      </ZButton>
    {/each}
    <span class="mx-1 h-4 w-px bg-border" aria-hidden="true"></span>
    {#each COLORS as swatch (swatch)}
      <button
        type="button"
        class="size-5 cursor-pointer rounded-full border-2 {color === swatch ? 'border-foreground' : 'border-transparent'}"
        style:background={swatch}
        aria-label="Colour {swatch}"
        onclick={() => (color = swatch)}
      ></button>
    {/each}
    <span class="mx-1 h-4 w-px bg-border" aria-hidden="true"></span>
    <ZButton zWave={false} zType="ghost" zSize="sm" class="size-7! p-0 text-muted-foreground" aria-label="Undo" title="Undo (Ctrl+Z)" disabled={shapes.length === 0} onclick={() => (shapes = shapes.slice(0, -1))}>
      <ZIcon zType={'lucideUndo2' as ZIconName} class="size-4" />
    </ZButton>
    <ZButton zWave={false} zType="ghost" zSize="sm" class="size-7! p-0 text-muted-foreground" aria-label="Clear" title="Clear" disabled={shapes.length === 0} onclick={() => (shapes = [])}>
      <ZIcon zType="lucideTrash2" class="size-4" />
    </ZButton>
    <div class="ml-auto flex items-center gap-1">
      <ZButton zWave={false} zType="ghost" zSize="sm" class="h-7 px-2 text-xs" onclick={oncancel}>Cancel</ZButton>
      <ZButton zWave={false} zType="default" zSize="sm" class="h-7 px-3 text-xs" onclick={done}>Add to chat</ZButton>
    </div>
  </div>

  <div class="relative grid min-h-0 flex-1 place-items-center overflow-auto p-3">
    <div class="relative max-h-full max-w-full" style:aspect-ratio="{shot.w} / {shot.h}" style:width="min(100%, {shot.w}px)">
      <canvas
        bind:this={canvas}
        width={shot.w}
        height={shot.h}
        class="block h-full w-full touch-none rounded-md border border-border bg-background shadow-sm {tool === 'note' ? 'cursor-text' : 'cursor-crosshair'}"
        onpointerdown={down}
        onpointermove={move}
        onpointerup={up}
      ></canvas>
      {#if noteAt}
        <input
          bind:this={noteInput}
          bind:value={noteText}
          class="absolute z-10 w-52 rounded-md border border-border bg-background px-2 py-1 text-xs shadow-md outline-none focus:border-primary"
          style:left="{noteAt.px}px"
          style:top="{noteAt.py}px"
          placeholder="Note for the agent…"
          onkeydown={(event) => {
            if (event.key === 'Enter') commitNote();
            event.stopPropagation();
            if (event.key === 'Escape') noteAt = null;
          }}
          onblur={commitNote}
        />
      {/if}
    </div>
  </div>
  {#if notes.length > 0}
    <ol class="m-0 shrink-0 list-none border-t border-border bg-background px-3 py-1.5 text-xs text-muted-foreground">
      {#each notes as note (note.n)}<li><span class="font-medium text-foreground">{note.n}.</span> {note.text}</li>{/each}
    </ol>
  {/if}
</div>
