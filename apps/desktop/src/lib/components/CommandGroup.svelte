<script lang="ts">
  import { ZIcon, ZLoading, ZShimmerText } from '@shival99/z-ui-svelte';
  import { argOf, commandTitle, groupTitle, liveState, toolOf, type CommandBlock } from '../blocks';
  import { loaderFor } from '../loaders';

  // Commands of one reply folded into a single collapsed group; open it to see each one, open a row for its output.
  let { items }: { items: CommandBlock[] } = $props();

  let open = $state(false);
  let rows = $state<Record<number, boolean>>({});

  const running = $derived(items.some((item) => item.status === 'running'));
  const live = $derived(liveState(items));
  const shownCommand = (item: CommandBlock) => (toolOf(item.command) === 'Bash' ? `> ${argOf(item.command)}` : item.command);
</script>

<div class="block-in flex flex-col gap-1.5 text-xs">
  <button
    type="button"
    class="flex w-fit cursor-pointer items-center gap-1.5 rounded-md py-0.5 text-left font-medium text-muted-foreground outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/60"
    aria-expanded={open}
    onclick={() => (open = !open)}
  >
    {#if running}
      <ZLoading zLoading zSpinner={live.spinner} zSize="2xs" zColor="primary" />
      <ZShimmerText zText={live.label} />
    {:else}
      {groupTitle(items)}
    {/if}
    <ZIcon zType="lucideChevronDown" class="size-3.5 transition-transform duration-300 {open ? 'rotate-180' : ''}" />
  </button>

  <div class="grid transition-[grid-template-rows] duration-300 ease-out motion-reduce:transition-none {open ? 'grid-rows-[1fr]' : 'grid-rows-[0fr]'}">
    <div class="min-h-0 overflow-hidden">
      <div class="divide-y divide-border overflow-hidden rounded-lg border border-border bg-muted/20">
        {#each items as item, index (item.id ?? index)}
          <div>
            <button
              type="button"
              class="flex w-full cursor-pointer items-center gap-2 px-3 py-2 text-left text-muted-foreground outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/60 focus-visible:ring-inset"
              aria-expanded={rows[index] ?? false}
              onclick={() => (rows[index] = !rows[index])}
            >
              {#if item.status === 'running'}
                <ZLoading zLoading zSpinner={loaderFor(item.command)} zSize="2xs" zColor="current" />
              {/if}
              <span class={item.status === 'fail' ? 'text-destructive' : ''}>{commandTitle(item)}</span>
              <ZIcon zType="lucideChevronDown" class="size-3.5 transition-transform duration-300 {rows[index] ? 'rotate-180' : ''}" />
            </button>
            <div class="grid transition-[grid-template-rows] duration-300 ease-out motion-reduce:transition-none {rows[index] ? 'grid-rows-[1fr]' : 'grid-rows-[0fr]'}">
              <div class="min-h-0 overflow-hidden">
                <div class="flex flex-col gap-2 px-3 pb-3">
                  <pre class="m-0 overflow-x-auto rounded-md border border-border bg-background px-3 py-2 font-mono text-[11px] leading-relaxed whitespace-pre-wrap break-all">{shownCommand(item)}</pre>
                  {#if item.output}
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
