<script lang="ts">
  import { ZIcon, ZLoading } from '@shival99/z-ui-svelte';
  import { LOADER } from '../loaders';
  import { OPENAI_SVG } from '../brand-icons';
  import { ticker } from '../clock.svelte';
  import { showSession } from '../content-tabs.svelte';
  import { modelLabel } from '../models.svelte';
  import { isDelegating, liveStats } from '../sessions.svelte';
  import type { Block } from '../mock';

  // The work a message handed to other agents (`@agent` or a flow's steps), in one card like a run command: a header,
  // then a row per agent that follows its chat (what it is doing now, its state) and opens it.
  let { blocks }: { blocks: Extract<Block, { kind: 'delegate' }>[] } = $props();

  const settledOf = (block: Extract<Block, { kind: 'delegate' }>) => (block.state === 'running' && !isDelegating(block.childId) ? 'failed' : block.state);
  const done = $derived(blocks.filter((block) => settledOf(block) === 'done').length);
  const working = $derived(blocks.some((block) => settledOf(block) === 'running'));
  const flow = $derived(blocks.some((block) => block.step));

  /** What a delegated chat is doing right now (its last line), read again each second while it works. */
  function lineOf(childId: string, state: string): string {
    if (state !== 'running') return '';
    void ticker.sec;
    return liveStats(childId, Date.now())?.last ?? '';
  }
  const tone = { running: 'text-warning', done: 'text-success', failed: 'text-destructive' } as const;
</script>

<div class="block-in w-full max-w-[85%] overflow-hidden rounded-lg border border-border bg-background">
  <div class="flex items-center gap-2 border-b border-border bg-muted/40 px-3 py-1.5 text-xs">
    <ZIcon zType={flow ? 'lucideWorkflow' : 'lucideAtSign'} class="size-3.5 text-muted-foreground" />
    <span class="truncate font-medium">{flow ? 'Flow steps' : blocks.length === 1 ? 'Handed to an agent' : `Handed to ${blocks.length} agents`}</span>
    <span class="ml-auto flex shrink-0 items-center gap-1 {working ? 'text-warning' : 'text-muted-foreground'}">
      {#if working}<ZLoading zLoading zSpinner={LOADER.reasoning} zSize="2xs" zColor="current" />{/if}
      {done}/{blocks.length} done
    </span>
  </div>
  <div class="divide-y divide-border">
    {#each blocks as block (block.childId)}
      {@const settled = settledOf(block)}
      <button type="button" class="flex w-full cursor-pointer items-center gap-2 px-3 py-2 text-left text-xs outline-none hover:bg-muted/40 focus-visible:bg-muted/60" onclick={() => showSession(block.childId)}>
        {#if block.agent === 'codex'}
          <ZIcon zSvg={OPENAI_SVG} class="size-3.5 shrink-0 text-foreground" />
        {:else}
          <ZIcon zType={block.agent === 'claude' ? 'brandClaude' : 'brandGemini'} class="size-3.5 shrink-0" />
        {/if}
        {#if block.step}<span class="shrink-0 rounded bg-primary/10 px-1.5 py-px text-[10px] font-medium text-primary">{block.step}</span>{/if}
        <span class="shrink-0 font-medium">{block.agent === 'codex' ? 'Codex' : block.agent === 'claude' ? 'Claude' : 'Antigravity'}</span>
        <span class="min-w-0 flex-1 truncate text-muted-foreground">{lineOf(block.childId, settled) || modelLabel(block.model)}</span>
        <span class="inline-flex shrink-0 items-center gap-1 {tone[settled]}">
          {#if settled === 'running'}
            <ZLoading zLoading zSpinner={LOADER.reasoning} zSize="2xs" zColor="current" /> Working
          {:else if settled === 'done'}
            <ZIcon zType="lucideCircleCheck" class="size-3.5" /> Answered
          {:else}
            <ZIcon zType="lucideCircleX" class="size-3.5" /> <span class="max-w-32 truncate">{block.note ?? 'Stopped'}</span>
          {/if}
        </span>
        <ZIcon zType="lucideChevronRight" class="size-3 shrink-0 text-muted-foreground" />
      </button>
    {/each}
  </div>
</div>
