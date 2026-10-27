<script module lang="ts">
  // Placeholder rows shown while a list loads. They are the same size as the real rows (`ROW` is also what the real rows
  // take as their minimum height), sit in the same padding, and share the scrollbar gutter of the scroller around them (`GUTTER`; reserving it again here
  // would make them narrower than the real rows), so when the list comes in nothing moves. They fill the whole area (the rows past the bottom are cut) and fade out as the real rows come in.
  export const ROW = { catalog: 'min-h-[92px]', skill: 'min-h-[116px]', plugin: 'min-h-[58px]', mcp: 'min-h-[88px]' } as const;
  export const GUTTER = '[scrollbar-gutter:stable]';
</script>

<script lang="ts">
  let { variant = 'catalog', rows = 14 }: { variant?: keyof typeof ROW; rows?: number } = $props();
</script>

<!-- No exit transition: an outgoing block stays in the page while it fades, and the list that replaces it would render
     under it, then jump up when it is gone (the shift seen when a list comes in). -->
<ul class="m-0 flex min-h-0 flex-1 list-none flex-col gap-2 overflow-hidden p-1.5" aria-hidden="true">
  {#each Array.from({ length: rows }, (_, index) => index) as index (index)}
    <li class="flex shrink-0 animate-pulse items-start gap-2.5 border border-border {variant === 'mcp' ? 'rounded-lg bg-card/40 p-3' : 'rounded-md bg-card p-2.5'} {ROW[variant]}" style:animation-delay="{index * 60}ms">
      <div class="min-w-0 flex-1 space-y-2">
        <div class="h-3.5 w-2/5 rounded bg-muted"></div>
        {#if variant !== 'plugin'}<div class="h-3 w-4/5 rounded bg-muted/70"></div>{/if}
        <div class="h-3 w-1/4 rounded bg-muted/60"></div>
      </div>
      <div class="h-7 w-16 shrink-0 rounded-md bg-muted/70"></div>
    </li>
  {/each}
</ul>
