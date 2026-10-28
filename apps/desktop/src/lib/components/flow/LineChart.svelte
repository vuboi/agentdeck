<script lang="ts">
  // A small line over the runs of a flow, oldest to newest: one dot per run, green when the run was right and red when it
  // was not, a dashed line at the mean. Hover a dot for its value. No axis lines: the top value and zero are written beside it.
  let { points, format, title }: { points: { value: number; ok: boolean; label: string }[]; format: (value: number) => string; title: string } = $props();

  const W = 300;
  const H = 64;
  const PAD = 8;
  const max = $derived(Math.max(...points.map((point) => point.value), 0.000001));
  const mean = $derived(points.reduce((sum, point) => sum + point.value, 0) / Math.max(points.length, 1));
  const x = (index: number) => (points.length === 1 ? W / 2 : PAD + (index * (W - PAD * 2)) / (points.length - 1));
  const y = (value: number) => H - PAD - (value / max) * (H - PAD * 2);
  const path = $derived(points.map((point, index) => `${index === 0 ? 'M' : 'L'}${x(index).toFixed(1)} ${y(point.value).toFixed(1)}`).join(' '));
</script>

<figure class="m-0 flex flex-col gap-0.5">
  <figcaption class="flex items-baseline gap-2 text-[10px] text-muted-foreground">
    <span class="font-medium tracking-wide uppercase">{title}</span>
    <span class="ml-auto tabular-nums">mean {format(mean)} · top {format(max)}</span>
  </figcaption>
  <svg viewBox="0 0 {W} {H}" class="block h-16 w-full overflow-visible" role="img" aria-label="{title} per run">
    <line x1="0" x2={W} y1={H - PAD} y2={H - PAD} class="stroke-border" stroke-width="1" />
    {#if points.length > 1}
      <line x1="0" x2={W} y1={y(mean)} y2={y(mean)} class="stroke-muted-foreground/50" stroke-width="1" stroke-dasharray="3 3" />
      <path d={path} fill="none" class="stroke-primary/70" stroke-width="1.5" stroke-linejoin="round" stroke-linecap="round" />
    {/if}
    {#each points as point, index (index)}
      <circle cx={x(index)} cy={y(point.value)} r="3.5" class={point.ok ? 'fill-emerald-500' : 'fill-destructive'}>
        <title>{point.label}: {format(point.value)}{point.ok ? '' : ' (wrong)'}</title>
      </circle>
    {/each}
  </svg>
</figure>
