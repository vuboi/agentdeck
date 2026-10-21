<script lang="ts">
  import { ZIcon, ZLoading, ZShowMore } from '@shival99/z-ui-svelte';
  import type { Block } from '../mock';

  let { block }: { block: Block } = $props();

  const statusIcon = { running: 'lucideLoaderCircle', ok: 'lucideCircleCheck', fail: 'lucideCircleX' } as const;
  const statusTone = { running: 'text-warning', ok: 'text-success', fail: 'text-destructive' } as const;
</script>

{#if block.kind === 'thinking'}
  <details class="group rounded-lg border border-border bg-muted/30 px-3 py-2 text-xs text-muted-foreground" open={false}>
    <summary class="flex cursor-pointer list-none items-center gap-2 select-none">
      <ZIcon zType="lucideBrain" class="size-3.5" />
      <span class="font-medium">Thinking</span>
      <ZIcon zType="lucideChevronRight" class="ml-auto size-3.5 transition-transform group-open:rotate-90" />
    </summary>
    <div class="mt-2 leading-relaxed">
      <ZShowMore zMaxHeight={72} zButtonSize="sm" moreLabel="Show thinking" lessLabel="Hide">
        <p class="m-0 whitespace-pre-wrap">{block.text}</p>
      </ZShowMore>
    </div>
  </details>
{:else if block.kind === 'response'}
  <div class="text-sm leading-relaxed text-foreground">
    <ZShowMore zMaxHeight={240} zButtonSize="sm">
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
        <ZShowMore zMaxHeight={120} zButtonSize="sm">
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
