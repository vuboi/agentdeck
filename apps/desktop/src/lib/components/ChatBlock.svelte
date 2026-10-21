<script lang="ts">
  import { ZCard, ZIcon, ZLoading, ZShowMore } from '@shival99/z-ui-svelte';
  import type { Block } from '../mock';

  // `fade` is the surface behind the block; ZShowMore fades its clipped edge into that colour.
  let { block, fade }: { block: Block; fade?: string } = $props();

  const statusIcon = { running: 'lucideLoaderCircle', ok: 'lucideCircleCheck', fail: 'lucideCircleX' } as const;
  const statusTone = { running: 'text-warning', ok: 'text-success', fail: 'text-destructive' } as const;
</script>

{#if block.kind === 'thinking'}
  <!-- ZCard animates the open and close of its body, so Thinking expands smoothly. -->
  <ZCard collapsible collapsed zSize="sm" class="text-xs text-muted-foreground">
    {#snippet header()}
      <span class="flex items-center gap-2 font-medium">
        <ZIcon zType="lucideBrain" class="size-3.5" />
        Thinking
      </span>
    {/snippet}
    <ZShowMore zMaxHeight={160} zButtonSize="sm" zBlurColor="var(--card)" moreLabel="Show more" lessLabel="Show less">
      <p class="m-0 whitespace-pre-wrap leading-relaxed">{block.text}</p>
    </ZShowMore>
  </ZCard>
{:else if block.kind === 'response'}
  <div class="text-sm leading-relaxed text-foreground">
    <ZShowMore zMaxHeight={240} zButtonSize="sm" zBlurColor={fade ?? 'var(--background)'}>
      <p class="m-0 whitespace-pre-wrap">{block.text}</p>
    </ZShowMore>
  </div>
{:else if block.kind === 'command'}
  <div class="overflow-hidden rounded-lg border border-border bg-background">
    <div class="flex items-center gap-2 border-b border-border bg-muted/40 px-3 py-1.5 font-mono text-xs">
      <ZIcon zType="lucideTerminal" class="size-3.5 text-muted-foreground" />
      <span class="truncate">{block.command}</span>
      <span class="ml-auto flex items-center gap-1 {statusTone[block.status]}">
        {#if block.status === 'running'}
          <ZLoading zSpinner="orbit" zSize="2xs" zColor="current" />
        {:else}
          <ZIcon zType={statusIcon[block.status]} class="size-3.5" />
        {/if}
        {block.status}
      </span>
    </div>
    {#if block.output}
      <div class="px-3 py-2 font-mono text-xs text-muted-foreground">
        <ZShowMore zMaxHeight={120} zButtonSize="sm" zBlurColor="var(--background)">
          <pre class="m-0 whitespace-pre-wrap">{block.output}</pre>
        </ZShowMore>
      </div>
    {/if}
  </div>
{:else if block.kind === 'loading'}
  <div class="flex items-center gap-2 text-xs text-muted-foreground">
    <ZLoading zSpinner="orbit" zSize="xs" zColor="primary" />
    <ZLoading zSpinner="scramble" zSize="xs" zColor="primary" />
    <span>{block.label}</span>
  </div>
{/if}
