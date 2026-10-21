<script lang="ts">
  import { ZIcon, ZLoading, ZShowMore } from '@shival99/z-ui-svelte';
  import type { Block } from '../mock';

  // `fade` is the surface behind the block; ZShowMore fades its clipped edge into that colour.
  let { block, fade }: { block: Block; fade?: string } = $props();

  // Thinking starts folded. The body animates by growing its grid row from 0fr to 1fr.
  let thinkingOpen = $state(false);

  // Each kind of work gets its own loader: searching, running a command and fetching look different.
  const SEARCH_TOOLS = /^(Read|Grep|Glob|LS|WebSearch)\b/;
  const FETCH_TOOLS = /^(WebFetch|Fetch)\b/;
  function spinnerFor(command: string): 'scan' | 'comet' | 'conic' {
    if (SEARCH_TOOLS.test(command)) return 'scan';
    if (FETCH_TOOLS.test(command)) return 'comet';
    return 'conic';
  }

  const statusIcon = { running: 'lucideLoaderCircle', ok: 'lucideCircleCheck', fail: 'lucideCircleX' } as const;
  const statusTone = { running: 'text-warning', ok: 'text-success', fail: 'text-destructive' } as const;
</script>

{#if block.kind === 'thinking' && block.text.trim()}
  <!-- Same flat style as the command blocks; the body slides open and shut. -->
  <div class="rounded-lg border border-border bg-muted/30 text-xs text-muted-foreground">
    <button
      type="button"
      class="flex w-full cursor-pointer items-center gap-2 rounded-lg px-3 py-2 text-left font-medium outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
      aria-expanded={thinkingOpen}
      onclick={() => (thinkingOpen = !thinkingOpen)}
    >
      <ZIcon zType="lucideBrain" class="size-3.5" />
      {#if block.streaming}<ZLoading zLoading zSpinner="pulse" zSize="2xs" zColor="primary" />{/if}
      Thinking
      <ZIcon zType="lucideChevronDown" class="ml-auto size-3.5 transition-transform duration-300 {thinkingOpen ? 'rotate-180' : ''}" />
    </button>
    <div class="grid transition-[grid-template-rows] duration-300 ease-out motion-reduce:transition-none {thinkingOpen ? 'grid-rows-[1fr]' : 'grid-rows-[0fr]'}">
      <div class="min-h-0 overflow-hidden">
        <div class="px-3 pb-2.5">
          <ZShowMore zMaxHeight={160} zButtonSize="sm" zBlurColor="var(--background)" moreLabel="Show more" lessLabel="Show less">
            <p class="m-0 whitespace-pre-wrap leading-relaxed">{block.text}</p>
          </ZShowMore>
        </div>
      </div>
    </div>
  </div>
{:else if block.kind === 'response'}
  <div class="text-sm leading-relaxed text-foreground">
    <ZShowMore zMaxHeight={240} zButtonSize="sm" zBlurColor={fade ?? 'var(--background)'}>
      <p class="m-0 whitespace-pre-wrap">{block.text}{#if block.streaming}<span class="ml-0.5 inline-block h-4 w-[2px] translate-y-[3px] animate-pulse bg-foreground" aria-hidden="true"></span>{/if}</p>
    </ZShowMore>
  </div>
{:else if block.kind === 'command'}
  <div class="overflow-hidden rounded-lg border border-border bg-background">
    <div class="flex items-center gap-2 border-b border-border bg-muted/40 px-3 py-1.5 font-mono text-xs">
      <ZIcon zType="lucideTerminal" class="size-3.5 text-muted-foreground" />
      <span class="truncate">{block.command}</span>
      <span class="ml-auto flex items-center gap-1 {statusTone[block.status]}">
        {#if block.status === 'running'}
          <ZLoading zLoading zSpinner={spinnerFor(block.command)} zSize="2xs" zColor="current" />
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
    <ZLoading zLoading zSpinner="ring" zSize="xs" zColor="primary" />
    <span>{block.label}</span>
  </div>
{/if}
