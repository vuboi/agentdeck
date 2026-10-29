<script lang="ts">
  import { ZButton, ZButtonGroup, ZButtonGroupItem, ZIcon, ZInput, ZModal, zToastService } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { tick } from 'svelte';
  import { inTauri } from '../../app/platform/history';
  import { dirOf, nameOf } from '../../files/paths';
  import { ACTIVE_ON, TRIGGER_ON } from '../../app/ui/popover';
  import { toast } from '../../app/platform/toast';
  import PickerPopover, { type PickerItem } from '../ui/PickerPopover.svelte';
  import { SHOT_ICON } from '../../files/screenshot-icons';
  import { theme } from '../../app/ui/theme.svelte';

  type Point = { x: number; y: number };
  type Tool = 'pan' | 'pen' | 'arrow' | 'box' | 'blur' | 'text' | 'erase';
  type NoteStyle = 'pointer' | 'bubble' | 'square';
  type PenWeight = 'thin' | 'medium' | 'thick' | 'bold' | 'heavy' | 'marker';
  type Presentation = 'original' | 'spotlight';
  type Mark =
    | { kind: 'pen'; points: Point[]; width: number; color: string }
    | { kind: 'arrow'; from: Point; to: Point; color: string }
    | { kind: 'box'; from: Point; to: Point; color: string; dashed?: boolean }
    | { kind: 'blur'; from: Point; to: Point }
    | { kind: 'text'; at: Point; text: string; style: NoteStyle; fontSize: number; color: string; number?: number };
  let { open = $bindable(false), imageUrl, filename, onadd }: { open?: boolean; imageUrl: string; filename: string; /** Puts the finished picture (a PNG data URL) into the chat message. */ onadd?: (dataUrl: string) => void } = $props();
  let canvas: HTMLCanvasElement | undefined = $state();
  let imageViewport: HTMLDivElement | undefined = $state();
  let viewWidth = $state(0);
  let viewHeight = $state(0);
  let tool = $state<Tool>('pan');
  let penWeight = $state<PenWeight>('medium');
  let noteStyle = $state<NoteStyle>('pointer');
  let noteFontSize = $state(28);
  let numbered = $state(false);
  let boxDashed = $state(false);
  let color = $state('#ef4444');
  let presentation = $state<Presentation>('original');
  let noteText = $state('');
  let marks = $state<Mark[]>([]);
  let past = $state<Mark[][]>([]);
  let undone = $state<Mark[][]>([]);
  let rotation = $state(0);
  let zoom = $state(1);
  let sourceWidth = $state(0);
  let sourceHeight = $state(0);
  let loadedImage: HTMLImageElement | undefined;
  let activeMark: Mark | undefined;
  let textAnchor = $state<Point | null>(null);
  let caretVisible = $state(true);
  let drawing = false;
  let erasing = false;
  let panning: { x: number; y: number; left: number; top: number } | null = $state(null);
  let erasedInStroke = false;
  /** Where each note was last drawn (source px), for the eraser to hit-test. */
  const noteBounds = new Map<string, { x: number; y: number; w: number; h: number }>();
  /** Drawing the saved image: editor-only guides (the pixelate outline) are left out. */
  let exporting = false;

  const framePadding = $derived(presentation === 'spotlight' ? 24 : 4);
  const rotatedSize = $derived.by(() => {
    const swap = rotation === 90 || rotation === 270;
    return { width: swap ? sourceHeight : sourceWidth, height: swap ? sourceWidth : sourceHeight };
  });
  const fitScale = $derived.by(() => {
    if (!rotatedSize.width || !viewWidth || !viewHeight) return 1;
    return Math.min(1, Math.max(1, viewWidth - 36 - framePadding * 2) / rotatedSize.width, Math.max(1, viewHeight - 36 - framePadding * 2) / rotatedSize.height);
  });
  const displayScale = $derived(fitScale * zoom);

  function sizeCanvas() {
    if (!canvas || !loadedImage) return;
    const swap = rotation === 90 || rotation === 270;
    const width = swap ? sourceHeight : sourceWidth;
    const height = swap ? sourceWidth : sourceHeight;
    if (canvas.width !== width) canvas.width = width;
    if (canvas.height !== height) canvas.height = height;
  }

  function wrapText(context: CanvasRenderingContext2D, text: string, maxWidth: number): string[] {
    const lines: string[] = [];
    for (const paragraph of text.split('\n')) {
      const words = paragraph.split(/\s+/).filter(Boolean);
      let line = '';
      for (const word of words) {
        const candidate = line ? `${line} ${word}` : word;
        if (line && context.measureText(candidate).width > maxWidth) {
          lines.push(line);
          line = word;
        } else line = candidate;
      }
      lines.push(line);
    }
    return lines.length ? lines : [''];
  }

  /** Readable text colour on top of a filled swatch. */
  function ink(value: string): string {
    const n = parseInt(value.slice(1), 16);
    return 0.299 * (n >> 16) + 0.587 * ((n >> 8) & 255) + 0.114 * (n & 255) > 170 ? '#111827' : '#fff';
  }

  function drawText(context: CanvasRenderingContext2D, mark: Extract<Mark, { kind: 'text' }>) {
    const size = mark.fontSize;
    const padding = size * 0.42;
    const lineHeight = size * 1.28;
    context.font = `600 ${size}px system-ui, sans-serif`;
    const maxTextWidth = Math.min(canvas!.width * 0.42, size * 18);
    const lines = wrapText(context, mark.text, maxTextWidth);
    const textWidth = Math.max(...lines.map((line) => context.measureText(line).width), size);
    const boxWidth = textWidth + padding * 2;
    const boxHeight = lines.length * lineHeight + padding * 2;
    const filled = mark.style === 'pointer';
    const tail = filled ? size * 0.55 : 0;
    const left = Math.max(0, Math.min(canvas!.width - boxWidth, mark.at.x));
    const above = mark.at.y - boxHeight - tail >= 0;
    const top = above ? mark.at.y - boxHeight - tail : Math.min(canvas!.height - boxHeight - tail, mark.at.y + tail);
    const radius = mark.style === 'bubble' ? size * 0.6 : 6;
    noteBounds.set(`${mark.at.x},${mark.at.y}`, { x: left, y: top, w: boxWidth, h: boxHeight + tail });
    const surface = theme.dark ? 'rgba(24,26,31,.96)' : 'rgba(255,255,255,.97)';
    const textColor = filled ? ink(mark.color) : theme.dark ? '#f4f4f5' : '#111827';
    context.save();
    context.shadowColor = 'rgba(0,0,0,.35)';
    context.shadowBlur = size * 0.3;
    context.shadowOffsetY = size * 0.1;
    context.fillStyle = filled ? mark.color : surface;
    context.beginPath();
    context.roundRect(left, top, boxWidth, boxHeight, radius);
    context.fill();
    context.shadowColor = 'transparent';
    context.strokeStyle = mark.color;
    context.lineWidth = Math.max(2, size / 14);
    context.stroke();
    context.fillStyle = mark.color;
    if (mark.style === 'bubble') {
      const tailX = left + Math.min(boxWidth - radius * 1.5, Math.max(radius * 1.5, mark.at.x - left + size * 0.35));
      const tailY = above ? top + boxHeight : top;
      context.beginPath();
      context.moveTo(tailX - size * 0.35, tailY);
      context.lineTo(tailX, tailY + (above ? size * 0.45 : -size * 0.45));
      context.lineTo(tailX + size * 0.55, tailY);
      context.closePath();
      context.fill();
    }
    if (filled) {
      const tipX = Math.min(left + boxWidth - size, Math.max(left + size, mark.at.x));
      const edgeY = above ? top + boxHeight : top;
      context.beginPath();
      context.moveTo(tipX - tail, edgeY);
      context.lineTo(tipX, edgeY + (above ? tail : -tail));
      context.lineTo(tipX + tail, edgeY);
      context.closePath();
      context.fill();
    }
    context.fillStyle = textColor;
    context.textBaseline = 'top';
    lines.forEach((line, index) => context.fillText(line, left + padding, top + padding + index * lineHeight));
    if (textAnchor && caretVisible && mark.at === textAnchor) {
      const lastLine = lines.at(-1) ?? '';
      const caretX = left + padding + context.measureText(lastLine).width + 1;
      const caretY = top + padding + (lines.length - 1) * lineHeight;
      context.fillRect(caretX, caretY, Math.max(2, size / 14), lineHeight);
    }
    context.restore();
    if (!mark.number) return;

    const badgeRadius = Math.max(14, size * 0.68);
    context.save();
    context.shadowColor = 'rgba(0,0,0,.45)';
    context.shadowBlur = size * 0.24;
    context.fillStyle = mark.color;
    context.strokeStyle = '#fff';
    context.lineWidth = Math.max(2, size / 10);
    context.beginPath();
    context.arc(mark.at.x, mark.at.y, badgeRadius, 0, Math.PI * 2);
    context.fill();
    context.stroke();
    context.shadowColor = 'transparent';
    context.fillStyle = ink(mark.color);
    context.font = `700 ${Math.max(14, badgeRadius * 1.1)}px system-ui, sans-serif`;
    context.textAlign = 'center';
    context.textBaseline = 'middle';
    context.fillText(String(mark.number), mark.at.x, mark.at.y + 1);
    context.restore();
  }

  /** Tapered arrow: hairline tail swelling into the shaft, then a swept-back head with curved sides. */
  function drawArrow(context: CanvasRenderingContext2D, from: Point, to: Point, fill: string, scale: number) {
    const length = Math.hypot(to.x - from.x, to.y - from.y);
    if (length < 6) return;
    const headLen = Math.min(length * 0.55, Math.max(26, scale * 1.9));
    const halfHead = headLen * 0.44;
    const halfShaft = halfHead * 0.34;
    const neck = length - headLen * 0.8;
    const barb = length - headLen;
    context.save();
    context.translate(from.x, from.y);
    context.rotate(Math.atan2(to.y - from.y, to.x - from.x));
    context.beginPath();
    context.moveTo(0, 0);
    context.quadraticCurveTo(neck * 0.5, halfShaft * 0.2, neck, halfShaft);
    context.lineTo(barb, halfHead);
    context.quadraticCurveTo(length - headLen * 0.42, halfHead * 0.5, length, 0);
    context.quadraticCurveTo(length - headLen * 0.42, -halfHead * 0.5, barb, -halfHead);
    context.lineTo(neck, -halfShaft);
    context.quadraticCurveTo(neck * 0.5, -halfShaft * 0.2, 0, 0);
    context.closePath();
    context.shadowColor = 'rgba(0,0,0,.45)';
    context.shadowBlur = headLen * 0.22;
    context.shadowOffsetY = headLen * 0.1;
    context.fillStyle = fill;
    context.fill();
    context.shadowColor = 'transparent';
    context.lineJoin = 'round';
    context.lineWidth = Math.max(1.5, headLen * 0.05);
    context.strokeStyle = 'rgba(255,255,255,.92)';
    context.stroke();
    context.restore();
  }

  function draw() {
    if (!canvas || !loadedImage) return;
    sizeCanvas();
    const context = canvas.getContext('2d');
    if (!context) return;
    context.clearRect(0, 0, canvas.width, canvas.height);
    const sourceWidth = loadedImage.naturalWidth;
    const sourceHeight = loadedImage.naturalHeight;
    if (rotation === 90) {
      context.translate(canvas.width, 0);
      context.rotate(Math.PI / 2);
    } else if (rotation === 180) {
      context.translate(canvas.width, canvas.height);
      context.rotate(Math.PI);
    } else if (rotation === 270) {
      context.translate(0, canvas.height);
      context.rotate(-Math.PI / 2);
    }
    context.drawImage(loadedImage, 0, 0, sourceWidth, sourceHeight);
    context.lineCap = 'round';
    context.lineJoin = 'round';
    const visibleMarks: Mark[] = [...marks, ...(activeMark ? [activeMark] : []), ...(textAnchor ? [{ kind: 'text' as const, at: textAnchor, text: noteText, style: noteStyle, fontSize: noteFontSize, color, number: nextNumber() }] : [])];
    for (const mark of visibleMarks) {
      if (mark.kind === 'pen') {
        const pts = mark.points;
        if (!pts.length) continue;
        context.strokeStyle = mark.color;
        context.fillStyle = mark.color;
        context.lineWidth = mark.width;
        if (pts.length === 1) {
          context.beginPath();
          context.arc(pts[0].x, pts[0].y, mark.width / 2, 0, Math.PI * 2);
          context.fill();
          continue;
        }
        context.beginPath();
        context.moveTo(pts[0].x, pts[0].y);
        for (let i = 1; i < pts.length - 1; i++) context.quadraticCurveTo(pts[i].x, pts[i].y, (pts[i].x + pts[i + 1].x) / 2, (pts[i].y + pts[i + 1].y) / 2);
        context.lineTo(pts[pts.length - 1].x, pts[pts.length - 1].y);
        context.stroke();
      } else if (mark.kind === 'box') {
        context.strokeStyle = mark.color;
        context.lineWidth = Math.max(3, sourceWidth / 600);
        context.beginPath();
        context.roundRect(Math.min(mark.from.x, mark.to.x), Math.min(mark.from.y, mark.to.y), Math.abs(mark.to.x - mark.from.x), Math.abs(mark.to.y - mark.from.y), 4);
        if (mark.dashed) context.setLineDash([context.lineWidth * 3, context.lineWidth * 2]);
        context.stroke();
        context.setLineDash([]);
      } else if (mark.kind === 'blur') {
        blurRegion(context, mark);
      } else if (mark.kind === 'arrow') {
        drawArrow(context, mark.from, mark.to, mark.color, Math.max(sourceWidth / 45, 16));
      } else if (mark.kind === 'text') drawText(context, mark);
    }
    context.setTransform(1, 0, 0, 1, 0, 0);
  }

  function blurRegion(context: CanvasRenderingContext2D, mark: Extract<Mark, { kind: 'blur' }>) {
    const sourceToCanvas = (point: Point): Point => {
      if (rotation === 90) return { x: canvas!.width - point.y, y: point.x };
      if (rotation === 180) return { x: canvas!.width - point.x, y: canvas!.height - point.y };
      if (rotation === 270) return { x: point.y, y: canvas!.height - point.x };
      return point;
    };
    const corners = [mark.from, { x: mark.to.x, y: mark.from.y }, mark.to, { x: mark.from.x, y: mark.to.y }].map(sourceToCanvas);
    const left = Math.max(0, Math.floor(Math.min(...corners.map((point) => point.x))));
    const top = Math.max(0, Math.floor(Math.min(...corners.map((point) => point.y))));
    const right = Math.min(canvas!.width, Math.ceil(Math.max(...corners.map((point) => point.x))));
    const bottom = Math.min(canvas!.height, Math.ceil(Math.max(...corners.map((point) => point.y))));
    const width = right - left;
    const height = bottom - top;
    if (width < 2 || height < 2) return;

    // Mosaic: average the area into small square cells, drawn crisp.
    const block = Math.max(6, Math.round(canvas!.width / 200));
    const small = document.createElement('canvas');
    small.width = Math.max(1, Math.ceil(width / block));
    small.height = Math.max(1, Math.ceil(height / block));
    const smallContext = small.getContext('2d');
    if (!smallContext) return;
    smallContext.imageSmoothingQuality = 'high';
    smallContext.drawImage(canvas!, left, top, width, height, 0, 0, small.width, small.height);
    context.save();
    context.setTransform(1, 0, 0, 1, 0, 0);
    context.beginPath();
    context.roundRect(left, top, width, height, 4);
    context.clip();
    context.imageSmoothingEnabled = false;
    context.drawImage(small, left, top, small.width * block, small.height * block);
    context.imageSmoothingEnabled = true;
    context.restore();
    if (exporting) return;
    // While editing, a dashed outline shows the covered area (left out of the saved PNG).
    const line = Math.max(1.5, canvas!.width / 900);
    context.save();
    context.setTransform(1, 0, 0, 1, 0, 0);
    context.lineWidth = line;
    context.setLineDash([line * 4, line * 3]);
    context.strokeStyle = 'rgba(0,0,0,.55)';
    context.strokeRect(left + line / 2, top + line / 2, width - line, height - line);
    context.lineDashOffset = line * 3.5;
    context.strokeStyle = 'rgba(255,255,255,.9)';
    context.strokeRect(left + line / 2, top + line / 2, width - line, height - line);
    context.restore();
  }

  $effect(() => {
    if (!open || !imageUrl || !canvas) return;
    const image = new Image();
    image.onload = () => {
      loadedImage = image;
      sourceWidth = image.naturalWidth;
      sourceHeight = image.naturalHeight;
      canvas!.width = sourceWidth;
      canvas!.height = sourceHeight;
      marks = [];
      past = [];
      undone = [];
      textAnchor = null;
      rotation = 0;
      zoom = 1;
      presentation = 'original';
      draw();
    };
    image.src = imageUrl;
  });

  $effect(() => {
    void rotation;
    void noteText;
    void noteStyle;
    void noteFontSize;
    void numbered;
    void color;
    void theme.dark;
    if (canvas && loadedImage) draw();
  });

  $effect(() => {
    if (!open || !textAnchor) return;
    caretVisible = true;
    const timer = setInterval(() => {
      caretVisible = !caretVisible;
      draw();
    }, 520);
    return () => clearInterval(timer);
  });

  $effect(() => {
    const element = canvas;
    if (!element) return;
    element.addEventListener('wheel', zoomWithWheel, { passive: false });
    return () => element.removeEventListener('wheel', zoomWithWheel);
  });

  function point(event: PointerEvent): Point {
    const rect = canvas!.getBoundingClientRect();
    const outX = ((event.clientX - rect.left) / rect.width) * canvas!.width;
    const outY = ((event.clientY - rect.top) / rect.height) * canvas!.height;
    if (rotation === 90) return { x: outY, y: loadedImage!.naturalHeight - outX };
    if (rotation === 180) return { x: loadedImage!.naturalWidth - outX, y: loadedImage!.naturalHeight - outY };
    if (rotation === 270) return { x: loadedImage!.naturalWidth - outY, y: outX };
    return { x: outX, y: outY };
  }

  function begin(event: PointerEvent) {
    if (!canvas || !loadedImage) return;
    canvas.setPointerCapture(event.pointerId);
    if (tool === 'pan') {
      if (imageViewport) panning = { x: event.clientX, y: event.clientY, left: imageViewport.scrollLeft, top: imageViewport.scrollTop };
      return;
    }
    const at = point(event);
    if (tool === 'text') {
      textAnchor = at;
      caretVisible = true;
      noteText = '';
      canvas.focus();
      draw();
      return;
    }
    if (tool === 'erase') {
      erasing = true;
      erasedInStroke = false;
      eraseAt(at);
      return;
    }
    drawing = true;
    if (tool === 'pen') {
      const mult = penWeights.find((item) => item.id === penWeight)?.size ?? 1;
      activeMark = { kind: 'pen', points: [at], width: Math.max(3, sourceWidth / 700) * mult, color };
    } else if (tool === 'blur') activeMark = { kind: 'blur', from: at, to: at };
    else if (tool === 'box') activeMark = { kind: 'box', from: at, to: at, color, dashed: boxDashed };
    else activeMark = { kind: 'arrow', from: at, to: at, color };
    draw();
  }

  function move(event: PointerEvent) {
    if (panning && imageViewport) {
      imageViewport.scrollLeft = panning.left - (event.clientX - panning.x);
      imageViewport.scrollTop = panning.top - (event.clientY - panning.y);
      return;
    }
    if (erasing) {
      eraseAt(point(event));
      return;
    }
    if (!drawing || !activeMark) return;
    const at = point(event);
    if (activeMark.kind === 'pen') activeMark.points.push(at);
    else if (activeMark.kind !== 'text') activeMark.to = at;
    draw();
  }

  function finish() {
    panning = null;
    erasing = false;
    if (!drawing || !activeMark) return;
    drawing = false;
    commit([...marks, activeMark]);
    activeMark = undefined;
    draw();
  }

  /** A new state of the marks: the old one goes on the undo stack and redo is cleared. */
  function commit(next: Mark[]) {
    past = [...past, marks];
    marks = next;
    undone = [];
  }

  function segmentDistance(p: Point, a: Point, b: Point): number {
    const dx = b.x - a.x;
    const dy = b.y - a.y;
    const lengthSq = dx * dx + dy * dy;
    const t = lengthSq ? Math.max(0, Math.min(1, ((p.x - a.x) * dx + (p.y - a.y) * dy) / lengthSq)) : 0;
    return Math.hypot(p.x - (a.x + t * dx), p.y - (a.y + t * dy));
  }

  function hits(mark: Mark, at: Point, tolerance: number): boolean {
    if (mark.kind === 'pen') {
      const pts = mark.points;
      if (pts.length === 1) return Math.hypot(at.x - pts[0].x, at.y - pts[0].y) <= mark.width / 2 + tolerance;
      for (let i = 1; i < pts.length; i++) if (segmentDistance(at, pts[i - 1], pts[i]) <= mark.width / 2 + tolerance) return true;
      return false;
    }
    if (mark.kind === 'arrow') return segmentDistance(at, mark.from, mark.to) <= tolerance + Math.max(sourceWidth / 120, 8);
    if (mark.kind === 'text') {
      const box = noteBounds.get(`${mark.at.x},${mark.at.y}`);
      const inBox = box ? at.x >= box.x - tolerance && at.x <= box.x + box.w + tolerance && at.y >= box.y - tolerance && at.y <= box.y + box.h + tolerance : false;
      return inBox || Math.hypot(at.x - mark.at.x, at.y - mark.at.y) <= Math.max(14, mark.fontSize * 0.68) + tolerance;
    }
    const left = Math.min(mark.from.x, mark.to.x);
    const right = Math.max(mark.from.x, mark.to.x);
    const top = Math.min(mark.from.y, mark.to.y);
    const bottom = Math.max(mark.from.y, mark.to.y);
    const inside = at.x >= left && at.x <= right && at.y >= top && at.y <= bottom;
    if (mark.kind === 'blur') return inside;
    const nearEdge = at.x >= left - tolerance && at.x <= right + tolerance && at.y >= top - tolerance && at.y <= bottom + tolerance && !(at.x > left + tolerance && at.x < right - tolerance && at.y > top + tolerance && at.y < bottom - tolerance);
    return nearEdge;
  }

  /** The eraser removes every mark under the pointer; one drag is one undo step. */
  function eraseAt(at: Point) {
    const tolerance = 10 / Math.max(displayScale, 0.05);
    const kept = marks.filter((mark) => !hits(mark, at, tolerance));
    if (kept.length === marks.length) return;
    if (!erasedInStroke) {
      past = [...past, marks];
      undone = [];
      erasedInStroke = true;
    }
    marks = kept;
    draw();
  }

  function undo() {
    if (textAnchor) {
      textAnchor = null;
      noteText = '';
      draw();
      return;
    }
    const previous = past.at(-1);
    if (!previous) return;
    undone = [...undone, marks];
    marks = previous;
    past = past.slice(0, -1);
    draw();
  }

  function redo() {
    const previous = undone.at(-1);
    if (!previous) return;
    past = [...past, marks];
    marks = previous;
    undone = undone.slice(0, -1);
    draw();
  }

  /** The step badge for a new note: only when numbering is on, counting the numbered notes so far. */
  function nextNumber(): number | undefined {
    return numbered ? marks.filter((mark) => mark.kind === 'text' && mark.number).length + 1 : undefined;
  }

  function commitText() {
    if (!textAnchor || !noteText.trim()) return;
    commit([...marks, { kind: 'text', at: textAnchor, text: noteText.trim(), style: noteStyle, fontSize: noteFontSize, color, number: nextNumber() }]);
    textAnchor = null;
    draw();
  }

  function onCanvasKey(event: KeyboardEvent) {
    if (!textAnchor || tool !== 'text') return;
    if (event.key === 'Enter' && event.shiftKey) {
      event.preventDefault();
      noteText += '\n';
      return;
    }
    if (event.key === 'Enter') {
      event.preventDefault();
      commitText();
      return;
    }
    if (event.key === 'Escape') {
      textAnchor = null;
      noteText = '';
      draw();
      return;
    }
    if (event.key === 'Backspace') {
      event.preventDefault();
      noteText = noteText.slice(0, -1);
      return;
    }
    if (event.key.length === 1 && !event.ctrlKey && !event.metaKey && !event.altKey) noteText += event.key;
  }

  function onWindowKey(event: KeyboardEvent) {
    if (!open || !(event.ctrlKey || event.metaKey)) return;
    if ((event.key.toLowerCase() === 'z' && event.shiftKey) || event.key.toLowerCase() === 'y') {
      event.preventDefault();
      redo();
      return;
    }
    if (event.key.toLowerCase() === 'z') {
      event.preventDefault();
      undo();
    }
  }

  // The canvas is redrawn turned at once; the frame starts turned back by the step and eases to 0, so the image swings round.
  let spin = $state(0);
  let spinning = $state(false);
  let spinTimer: ReturnType<typeof setTimeout> | undefined;
  function rotate(step: number) {
    rotation = (rotation + step + 360) % 360;
    clearTimeout(spinTimer);
    spinning = false;
    spin = -step;
    requestAnimationFrame(() =>
      requestAnimationFrame(() => {
        spinning = true;
        spin = 0;
        spinTimer = setTimeout(() => (spinning = false), 360);
      }),
    );
  }

  function setZoom(value: number) {
    zoom = Math.max(0.25, Math.min(3, Math.round(value * 100) / 100));
  }

  function zoomWithWheel(event: WheelEvent) {
    if (!canvas || !imageViewport) return;
    event.preventDefault();
    const before = canvas.getBoundingClientRect();
    const anchorX = Math.max(0, Math.min(1, (event.clientX - before.left) / before.width));
    const anchorY = Math.max(0, Math.min(1, (event.clientY - before.top) / before.height));
    const next = zoom * Math.exp(-event.deltaY * 0.0015);
    if (Math.abs(next - zoom) < 0.005) return;
    setZoom(next);
    void tick().then(() => {
      if (!canvas || !imageViewport) return;
      const after = canvas.getBoundingClientRect();
      imageViewport.scrollLeft += after.left + anchorX * after.width - event.clientX;
      imageViewport.scrollTop += after.top + anchorY * after.height - event.clientY;
    });
  }

  /** The finished picture: marks drawn in, editor-only guides left out. */
  function finished(): HTMLCanvasElement {
    if (textAnchor) commitText();
    exporting = true;
    draw();
    const output = renderPresentation();
    exporting = false;
    draw();
    return output;
  }

  function addToChat() {
    if (!canvas || !onadd) return;
    onadd(finished().toDataURL('image/png'));
    open = false;
  }

  /** Save As: the user picks where; the editor stays open and a note says where it went, with a way to open the folder. */
  async function download() {
    if (!canvas) return;
    const output = finished();
    if (!inTauri()) {
      output.toBlob((blob) => {
        if (!blob) return;
        const url = URL.createObjectURL(blob);
        const link = document.createElement('a');
        link.href = url;
        link.download = filename;
        link.click();
        setTimeout(() => URL.revokeObjectURL(url), 1000);
      }, 'image/png');
      return;
    }
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      const saved = await invoke<string | null>('image_save_as', { name: filename, data: output.toDataURL('image/png').split(',')[1] });
      if (!saved) return;
      const folder = dirOf(saved);
      zToastService.success(`Saved ${nameOf(saved)}`, {
        action: { label: 'Show in folder', onClick: () => void invoke('reveal_path', { path: folder }) },
      });
    } catch (error) {
      toast.error(`Could not save: ${error}`);
    }
  }

  function renderPresentation(): HTMLCanvasElement {
    const padding = presentation === 'spotlight' ? 72 : 0;
    const output = document.createElement('canvas');
    output.width = canvas!.width + padding * 2;
    output.height = canvas!.height + padding * 2;
    const context = output.getContext('2d')!;
    if (presentation === 'original') {
      context.drawImage(canvas!, 0, 0);
      return output;
    }
    const gradient = context.createLinearGradient(0, 0, output.width, output.height);
    gradient.addColorStop(0, '#f97316');
    gradient.addColorStop(0.48, '#8b5cf6');
    gradient.addColorStop(1, '#172033');
    context.fillStyle = gradient;
    context.fillRect(0, 0, output.width, output.height);
    context.shadowColor = 'rgba(0,0,0,.42)';
    context.shadowBlur = 40;
    context.shadowOffsetY = 18;
    context.drawImage(canvas!, padding, padding);
    return output;
  }

  const tools: { id: Tool; label: string; svg: string }[] = [
    { id: 'pan', label: 'Hand (drag to scroll)', svg: SHOT_ICON.pan },
    { id: 'arrow', label: 'Arrow', svg: SHOT_ICON.arrow },
    { id: 'pen', label: 'Pen', svg: SHOT_ICON.pen },
    { id: 'box', label: 'Rectangle', svg: SHOT_ICON.box },
    { id: 'text', label: 'Text note', svg: SHOT_ICON.text },
    { id: 'blur', label: 'Pixelate area', svg: SHOT_ICON.blur },
    { id: 'erase', label: 'Eraser', svg: SHOT_ICON.erase },
  ];
  const noteStyles: { id: NoteStyle; label: string; icon: ZIconName }[] = [
    { id: 'pointer', label: 'Pointer note', icon: 'lucideMessageSquareMore' },
    { id: 'bubble', label: 'Bubble note', icon: 'lucideMessageCircle' },
    { id: 'square', label: 'Outline note', icon: 'lucideSquare' },
  ];
  /** `size` scales the stroke against the image; `width` is the preview bar in px. */
  const penWeights: { id: PenWeight; label: string; width: number; size: number }[] = [
    { id: 'thin', label: 'Thin', width: 1, size: 0.6 },
    { id: 'medium', label: 'Medium', width: 2.5, size: 1 },
    { id: 'thick', label: 'Thick', width: 4, size: 1.8 },
    { id: 'bold', label: 'Bold', width: 6, size: 3 },
    { id: 'heavy', label: 'Heavy', width: 8, size: 4.6 },
    { id: 'marker', label: 'Marker', width: 11, size: 7 },
  ];
  let fontTrigger: HTMLElement | null = $state(null);
  let fontOpen = $state(false);
  const fontItems = $derived<PickerItem<number>[]>([16, 20, 24, 28, 32, 40, 48, 64, 80].map((size) => ({ id: String(size), label: `${size}px`, selected: size === noteFontSize, data: size })));
  const palette = ['#ef4444', '#f97316', '#facc15', '#22c55e', '#14b8a6', '#3b82f6', '#6366f1', '#a855f7', '#ec4899', '#ffffff', '#9ca3af', '#111827'];
</script>

<svelte:window onkeydown={onWindowKey} />

{#snippet railButton(label: string, svg: string, onclick: () => void, active = false, disabled = false)}
  <ZButton zWave={false} zType="ghost" zSize="sm" class="size-9! shrink-0 p-0 {active ? ACTIVE_ON : ''}" title={label} aria-label={label} aria-pressed={active} {disabled} {onclick}>
    <ZIcon zSvg={svg} class="size-5" />
  </ZButton>
{/snippet}

<ZModal bind:open class="gap-0! py-0!" zWidth="min(1200px, calc(100vw - 24px))" zBodyClass="min-h-0! p-0! py-0!" zHideHeader zHideFooter zClosable={false}>
  <div class="flex h-[min(78dvh,900px)] min-h-[320px] overflow-hidden bg-background">
    <div class="flex w-12 shrink-0 flex-col items-center gap-1 border-r border-border bg-background py-2">
      {#each tools as item (item.id)}
        {@render railButton(item.label, item.svg, () => (tool = item.id), tool === item.id)}
      {/each}
      <span class="my-1 h-px w-6 shrink-0 bg-border"></span>
      {@render railButton('Undo (Ctrl+Z)', SHOT_ICON.undo, undo, false, !past.length && !textAnchor)}
      {@render railButton('Redo (Ctrl+Shift+Z)', SHOT_ICON.redo, redo, false, !undone.length)}
      <span class="my-1 h-px w-6 shrink-0 bg-border"></span>
      {@render railButton('Rotate left', SHOT_ICON.rotateLeft, () => rotate(-90))}
      {@render railButton('Rotate right', SHOT_ICON.rotateRight, () => rotate(90))}
    </div>

    <div class="flex min-w-0 flex-1 flex-col">
      <div class="flex min-h-11 shrink-0 flex-wrap items-center gap-x-3 gap-y-1 border-b border-border bg-background py-1.5 pr-1.5 pl-3">
        {#if tool !== 'blur' && tool !== 'erase' && tool !== 'pan'}
          <div class="w-40 shrink-0">
            <ZInput zType="color" zSize="sm" aria-label="Mark color" value={color} zColorConfig={{ mode: 'picker', savedColors: palette, cacheKey: 'screenshot-marks', showAlphaSlider: false }} onValueChange={(next) => typeof next === 'string' && next && (color = next)} />
          </div>
        {/if}
        {#if tool === 'pen'}
          <span class="h-5 w-px bg-border"></span>
          <div class="flex items-center gap-1">
            {#each penWeights as weight (weight.id)}
              <ZButton zWave={false} zType="ghost" zSize="sm" class="size-8! shrink-0 p-0 {penWeight === weight.id ? ACTIVE_ON : ''}" title={weight.label} aria-label={`${weight.label} pen`} aria-pressed={penWeight === weight.id} onclick={() => (penWeight = weight.id)}>
                <span class="w-4 rounded-full bg-current" style:height={`${weight.width}px`}></span>
              </ZButton>
            {/each}
          </div>
        {:else if tool === 'box'}
          <span class="h-5 w-px bg-border"></span>
          <div class="flex items-center gap-1">
            {#each [false, true] as dashed (dashed)}
              <ZButton zWave={false} zType="ghost" zSize="sm" class="size-8! shrink-0 p-0 {boxDashed === dashed ? ACTIVE_ON : ''}" title={dashed ? 'Dashed border' : 'Solid border'} aria-label={dashed ? 'Dashed border' : 'Solid border'} aria-pressed={boxDashed === dashed} onclick={() => (boxDashed = dashed)}>
                <span class="size-4 rounded-[3px] border-2 border-current {dashed ? 'border-dashed' : ''}"></span>
              </ZButton>
            {/each}
          </div>
        {:else if tool === 'text'}
          <span class="h-5 w-px bg-border"></span>
          <div class="flex items-center gap-1">
            {#each noteStyles as style (style.id)}
              <ZButton zWave={false} zType="ghost" zSize="sm" class="size-8! shrink-0 p-0 {noteStyle === style.id ? ACTIVE_ON : ''}" title={style.label} aria-label={style.label} aria-pressed={noteStyle === style.id} onclick={() => (noteStyle = style.id)}>
                <ZIcon zType={style.icon} class="size-3.5" />
              </ZButton>
            {/each}
            <ZButton zWave={false} zType="ghost" zSize="sm" class="size-8! shrink-0 p-0 {numbered ? ACTIVE_ON : ''}" title="Step number badge" aria-label="Step number badge" aria-pressed={numbered} onclick={() => (numbered = !numbered)}>
              <ZIcon zType="lucideListOrdered" class="size-3.5" />
            </ZButton>
            <ZButton bind:ref={fontTrigger} zWave={false} zType="ghost" zSize="sm" class="h-8! shrink-0 gap-1 px-1.5 text-sm tabular-nums {fontOpen ? TRIGGER_ON : ''}" title="Text size" aria-label="Text size" aria-haspopup="listbox" aria-expanded={fontOpen} onclick={() => (fontOpen = !fontOpen)}>
              <ZIcon zType="lucideType" class="size-3.5" />{noteFontSize}px<ZIcon zType="lucideChevronDown" class="size-3 text-muted-foreground" />
            </ZButton>
          </div>
          <PickerPopover anchor={fontTrigger} bind:open={fontOpen} items={fontItems} local fitContent keys size="sm" width={120} label="Text size" initialActive={false} onpick={(item) => { noteFontSize = item.data; fontOpen = false; }} />
        {:else if tool === 'pan'}
          <span class="text-sm text-muted-foreground">Drag the image to scroll it. Zoom in first with + or the mouse wheel.</span>
        {:else if tool === 'erase'}
          <span class="text-sm text-muted-foreground">Click or drag over a mark to remove it.</span>
        {:else if tool === 'blur'}
          <span class="text-sm text-muted-foreground">Drag over the area to hide.</span>
        {/if}

        <div class="ml-auto flex items-center gap-1.5">
          <ZButtonGroup>
            <ZButtonGroupItem zWave={false} zType="outline" zSize="sm" class="size-8! p-0" title="Zoom out" aria-label="Zoom out" disabled={zoom <= 0.25} onclick={() => setZoom(zoom - 0.25)}><ZIcon zType="lucideMinus" class="size-3.5" /></ZButtonGroupItem>
            <ZButtonGroupItem zWave={false} zType="outline" zSize="sm" class="h-8! w-12 px-0 text-[0.6875rem] tabular-nums text-muted-foreground" title="Reset to 100%" aria-label="Reset zoom" onclick={() => setZoom(1)}>{Math.round(zoom * 100)}%</ZButtonGroupItem>
            <ZButtonGroupItem zWave={false} zType="outline" zSize="sm" class="size-8! p-0" title="Zoom in" aria-label="Zoom in" disabled={zoom >= 3} onclick={() => setZoom(zoom + 0.25)}><ZIcon zType="lucidePlus" class="size-3.5" /></ZButtonGroupItem>
          </ZButtonGroup>
          <ZButton zWave={false} zType="ghost" zSize="sm" class="size-8! shrink-0 p-0" title="Fit to window" aria-label="Fit to window" onclick={() => setZoom(1)}><ZIcon zType="lucideMinimize2" class="size-3.5" /></ZButton>
          {#if onadd}
            <ZButton zWave={false} zType="ghost" zSize="sm" class="h-8! shrink-0 gap-1.5 border border-primary/30 bg-primary/10! px-2.5 text-sm text-primary! hover:bg-primary/20!" title="Add to chat" aria-label="Add the screenshot to the chat message" onclick={addToChat}><ZIcon zType="lucideMessageSquarePlus" class="size-3.5" />Add to chat</ZButton>
          {/if}
          <ZButton zWave={false} zType="ghost" zSize="sm" class="size-8! shrink-0 p-0" title="Save as…" aria-label="Save screenshot as PNG" onclick={download}><ZIcon zType="lucideDownload" class="size-3.5" /></ZButton>
          <span class="h-5 w-px bg-border"></span>
          <ZButton zWave={false} zType="ghost" zSize="sm" class="size-8! shrink-0 p-0" title="Close" aria-label="Close" onclick={() => (open = false)}><ZIcon zType="lucideX" class="size-3.5" /></ZButton>
        </div>
      </div>

      <div bind:this={imageViewport} bind:clientWidth={viewWidth} bind:clientHeight={viewHeight} class="screenshot-image-viewport flex min-h-0 flex-1 overflow-auto bg-background p-4">
        <div style:transform={`rotate(${spin}deg)`} class="m-auto shrink-0 overflow-hidden shadow-2xl {spinning ? 'transition-[transform,width,height] duration-300 ease-[cubic-bezier(.2,.8,.2,1)] motion-reduce:transition-none' : ''} {presentation === 'spotlight' ? 'rounded-2xl bg-linear-to-br from-orange-500 via-violet-500 to-slate-800 p-6' : 'rounded-md border border-border bg-background p-1'}" style:width={`${rotatedSize.width * displayScale + framePadding * 2 + 2}px`} style:height={`${rotatedSize.height * displayScale + framePadding * 2 + 2}px`}>
          <canvas bind:this={canvas} tabindex="0" class="block touch-none outline-none {tool === 'text' ? 'cursor-text' : tool === 'erase' ? 'cursor-cell' : tool === 'pan' ? (panning ? 'cursor-grabbing' : 'cursor-grab') : 'cursor-crosshair'}" style:width={`${rotatedSize.width * displayScale}px`} style:height={`${rotatedSize.height * displayScale}px`} onpointerdown={begin} onpointermove={move} onpointerup={finish} onpointercancel={finish} onkeydown={onCanvasKey} aria-label="Screenshot markup canvas. Select the text tool, click the image, then type a note. Use the mouse wheel to zoom."></canvas>
        </div>
      </div>
    </div>
  </div>
</ZModal>

<style>
  .screenshot-image-viewport {
    scrollbar-width: none;
    -ms-overflow-style: none;
  }

  .screenshot-image-viewport::-webkit-scrollbar {
    display: none;
  }
</style>
