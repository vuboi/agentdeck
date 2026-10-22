<script lang="ts">
  import { ZIcon, ZLoading, ZShimmerText } from '@shival99/z-ui-svelte';
  import { ticker } from '../clock.svelte';
  import { argOf, describeStep, elapsed, liveState, summarize, toolOf, totalChange, type CommandBlock } from '../blocks';

  // The steps an agent took, as one quiet line: "Created 2 files, ran 8 commands  +222 −0". While the agent
  // is still working that line carries the only loader, with the current step and a running clock; open it
  // for the list of steps, open a step for its command, output or diff.
  let { items, live = false, startedAt }: { items: CommandBlock[]; live?: boolean; startedAt?: number } = $props();

  let open = $state(false);
  let rows = $state<Record<number, boolean>>({});

  const current = $derived(liveState(items));
  const total = $derived(totalChange(items));
  const seconds = $derived(elapsed(ticker.sec - (startedAt ?? ticker.sec)));
  const shown = (item: CommandBlock) => (toolOf(item.command) === 'Bash' ? `> ${argOf(item.command)}` : item.command);
  const diffLines = (item: CommandBlock) => {
    const change = item.change!;
    const side = (text: string, sign: '+' | '-') => (text ? text.split('\n').map((line) => ({ sign, line })) : []);
    return [...side(change.before, '-'), ...side(change.after, '+')];
  };
</script>

{#snippet chips(change: { added: number; removed: number })}
  <span class="flex shrink-0 items-center gap-1 font-mono text-[10px] leading-none">
    <span class="rounded bg-success/15 px-1 py-0.5 text-success">+{change.added}</span>
    <span class="rounded bg-destructive/15 px-1 py-0.5 text-destructive">−{change.removed}</span>
  </span>
{/snippet}

<div class="block-in flex flex-col gap-2 text-xs">
  <button
    type="button"
    class="flex w-fit max-w-full cursor-pointer items-center gap-2 rounded-md py-0.5 text-left text-muted-foreground outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/60"
    aria-expanded={open}
    onclick={() => (open = !open)}
  >
    {#if live}
      {#key current.spinner}
        <ZLoading zLoading zSpinner={current.spinner} zSize="xs" zColor="primary" />
      {/key}
      <ZShimmerText zText={current.label} />
      <span class="tabular-nums">{seconds}</span>
    {:else}
      <span class="truncate font-medium">{summarize(items)}</span>
      {#if total}{@render chips(total)}{/if}
    {/if}
    <ZIcon zType="lucideChevronDown" class="size-3.5 shrink-0 transition-transform duration-300 {open ? 'rotate-180' : ''}" />
  </button>

  <div class="grid transition-[grid-template-rows] duration-300 ease-out motion-reduce:transition-none {open ? 'grid-rows-[1fr]' : 'grid-rows-[0fr]'}">
    <div class="min-h-0 overflow-hidden">
      <div class="divide-y divide-border overflow-hidden rounded-xl border border-border">
        {#each items as item, index (item.id ?? index)}
          {@const step = describeStep(item)}
          <div>
            <button
              type="button"
              class="flex w-full cursor-pointer items-center gap-2 px-3 py-2 text-left outline-none hover:bg-muted/40 focus-visible:ring-2 focus-visible:ring-ring/60 focus-visible:ring-inset"
              aria-expanded={rows[index] ?? false}
              onclick={() => (rows[index] = !rows[index])}
            >
              <span class="shrink-0 {item.status === 'fail' ? 'text-destructive' : 'text-muted-foreground'}">{step.verb}</span>
              {#if item.status === 'running'}
                <ZShimmerText class="min-w-0 truncate font-medium" zText={step.subject || '…'} />
              {:else}
                <span class="min-w-0 truncate font-medium text-foreground">{step.subject}</span>
              {/if}
              {#if step.change}{@render chips(step.change)}{/if}
              <ZIcon zType="lucideChevronRight" class="ml-auto size-3.5 shrink-0 text-muted-foreground transition-transform duration-300 {rows[index] ? 'rotate-90' : ''}" />
            </button>
            <div class="grid transition-[grid-template-rows] duration-300 ease-out motion-reduce:transition-none {rows[index] ? 'grid-rows-[1fr]' : 'grid-rows-[0fr]'}">
              <div class="min-h-0 overflow-hidden">
                <div class="flex flex-col gap-2 px-3 pb-3">
                  {#if item.change}
                    <div class="max-h-60 overflow-auto rounded-md border border-border bg-background py-1 font-mono text-[11px] leading-relaxed">
                      <div class="px-3 pb-1 text-muted-foreground">{item.change.path}</div>
                      {#each diffLines(item) as row}
                        <div class="px-3 whitespace-pre-wrap break-all {row.sign === '+' ? 'bg-success/10 text-success' : 'bg-destructive/10 text-destructive'}">{row.sign} {row.line}</div>
                      {/each}
                    </div>
                  {:else}
                    <pre class="m-0 overflow-x-auto rounded-md border border-border bg-background px-3 py-2 font-mono text-[11px] leading-relaxed whitespace-pre-wrap break-all">{shown(item)}</pre>
                  {/if}
                  {#if item.output && !item.change}
                    <pre class="m-0 max-h-60 overflow-auto px-1 font-mono text-[11px] leading-relaxed whitespace-pre-wrap break-all text-muted-foreground">{item.output}</pre>
                  {/if}
                </div>
              </div>
            </div>
          </div>
        {/each}
      </div>
    </div>
  </div>
</div>
