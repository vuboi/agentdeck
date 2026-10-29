<script lang="ts">
  // A candidate that is still being painted: a grid of dots that flicker like noise, with a soft wave of the theme's
  // primary colour passing through them, the way a diffusion model denoises. It claims no progress it does not know:
  // only the real time since the run started. With reduced motion the grid is drawn once and stays still.
  let { index = 0, label = '', variant = 0, ms = null }: {
    /** Its place in the grid: staggers the entrance and offsets the wave. */
    index?: number;
    /** The model it waits on, and which of its variants. Empty: a saved run being read back. */
    label?: string;
    variant?: number;
    /** Time since the run started. `null`: no clock (reading a saved run). */
    ms?: number | null;
  } = $props();

  const clock = $derived(ms === null ? '' : `${Math.floor(ms / 60000)}:${String(Math.floor(ms / 1000) % 60).padStart(2, '0')}`);

  const GAP = 9;
  const RADIUS = 1.1;
  const FPS = 30;

  function dots(canvas: HTMLCanvasElement) {
    const context = canvas.getContext('2d');
    if (!context) return;
    const still = matchMedia('(prefers-reduced-motion: reduce)').matches;
    // The colours come from the theme through CSS (`text-primary`, `text-muted-foreground` on the probes).
    const probe = (name: string) => {
      const span = document.createElement('span');
      span.className = name;
      canvas.parentElement?.append(span);
      const colour = getComputedStyle(span).color;
      span.remove();
      return colour;
    };
    const primary = probe('text-primary');
    const muted = probe('text-muted-foreground');
    // A fixed random phase per dot, so each one flickers on its own.
    let phases: number[] = [];
    let cols = 0;
    let rows = 0;
    const size = () => {
      const ratio = devicePixelRatio || 1;
      const { width, height } = canvas.getBoundingClientRect();
      canvas.width = Math.round(width * ratio);
      canvas.height = Math.round(height * ratio);
      context.setTransform(ratio, 0, 0, ratio, 0, 0);
      cols = Math.ceil(width / GAP);
      rows = Math.ceil(height / GAP);
      phases = Array.from({ length: cols * rows }, () => Math.random() * Math.PI * 2);
    };
    size();
    const observer = new ResizeObserver(size);
    observer.observe(canvas);

    let frame = 0;
    let last = 0;
    const draw = (now: number) => {
      frame = requestAnimationFrame(draw);
      if (now - last < 1000 / FPS) return;
      last = now;
      const t = now / 1000 + index * 0.9;
      const { width, height } = canvas.getBoundingClientRect();
      context.clearRect(0, 0, width, height);
      // The wave: a band that moves diagonally across, wrapping around.
      const span = cols + rows;
      const front = ((t * 9) % (span + 16)) - 8;
      for (let row = 0; row < rows; row++) {
        for (let col = 0; col < cols; col++) {
          const phase = phases[row * cols + col];
          const flicker = 0.5 + 0.5 * Math.sin(t * 2.2 + phase);
          const distance = Math.abs(col + row - front);
          const wave = Math.max(0, 1 - distance / 6);
          const x = col * GAP + GAP / 2;
          const y = row * GAP + GAP / 2;
          context.globalAlpha = wave > 0.05 ? 0.25 + 0.75 * wave * (0.6 + 0.4 * flicker) : 0.12 + 0.28 * flicker;
          context.fillStyle = wave > 0.05 ? primary : muted;
          context.beginPath();
          context.arc(x, y, RADIUS + wave * 0.9, 0, Math.PI * 2);
          context.fill();
        }
      }
      if (still) cancelAnimationFrame(frame);
    };
    frame = requestAnimationFrame(draw);
    return () => {
      cancelAnimationFrame(frame);
      observer.disconnect();
    };
  }
</script>

<div class="tile overflow-hidden rounded-lg border border-border bg-background shadow-sm" style:--i={index}>
  <div class="relative aspect-square overflow-hidden bg-muted/30">
    <canvas class="absolute inset-0 size-full" {@attach dots}></canvas>
    <span class="absolute bottom-1.5 left-1/2 -translate-x-1/2 rounded-full bg-background/70 px-2 py-0.5 text-[10px] font-medium text-muted-foreground backdrop-blur-sm">{ms === null ? 'Loading' : 'Generating'}<span class="ellipsis" aria-hidden="true"><span>.</span><span>.</span><span>.</span></span></span>
  </div>
  <div class="flex items-center gap-2 px-2 py-1.5 text-[10px] text-muted-foreground">
    <span class="min-w-0 flex-1 truncate">{label ? `${label} · #${variant}` : 'Saved candidate'}</span>
    {#if clock}<span class="shrink-0 tabular-nums">{clock}</span>{/if}
  </div>
</div>

<style>
  .tile {
    animation: enter 380ms cubic-bezier(0.2, 0.8, 0.2, 1) both;
    animation-delay: calc(var(--i) * 70ms);
  }
  @keyframes enter {
    from { opacity: 0; transform: translateY(6px); }
    to { opacity: 1; transform: none; }
  }
  /* Three dots that light up one after another, on a loop. */
  .ellipsis span {
    animation: dot 1.2s ease-in-out infinite;
  }
  .ellipsis span:nth-child(2) { animation-delay: 0.2s; }
  .ellipsis span:nth-child(3) { animation-delay: 0.4s; }
  @keyframes dot {
    0%, 60%, 100% { opacity: 0.2; }
    30% { opacity: 1; }
  }
  @media (prefers-reduced-motion: reduce) {
    .tile, .ellipsis span { animation: none; }
  }
</style>
