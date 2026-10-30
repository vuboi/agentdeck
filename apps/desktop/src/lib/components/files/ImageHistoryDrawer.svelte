<script lang="ts">
  import { untrack } from 'svelte';
  import { ZDrawer, ZPopover, createVirtualizer } from '@shival99/z-ui-svelte';
  import type { ImageHistoryRun } from '../../chat/agent';
  import { gen, openRun } from '../../files/image-gen.svelte';
  import DevEmpty from '../browser/DevEmpty.svelte';
  import { dayLabel, statusLabel, timeLabel } from './image-generator-options';

  let { open = $bindable(false) }: { open?: boolean } = $props();

  const history = $derived(gen.history);
  const activeRunId = $derived(gen.activeRunId);

  // The full prompt of a history row that is cut off, shown in a popover while the pointer rests on the row.
  let peek = $state<ImageHistoryRun | null>(null);
  let peekAnchor = $state<HTMLElement | null>(null);
  let peekOpen = $state(false);
  let peekTimer: ReturnType<typeof setTimeout> | undefined;

  function peekRun(run: ImageHistoryRun, row: HTMLElement) {
    clearTimeout(peekTimer);
    const label = row.querySelector('span');
    if (!label || label.scrollWidth <= label.clientWidth) {
      peekOpen = false;
      return;
    }
    peek = run;
    peekAnchor = row;
    peekOpen = true;
  }

  function leavePeek() {
    clearTimeout(peekTimer);
    peekTimer = setTimeout(() => (peekOpen = false), 150);
  }

  function keepPeek() {
    clearTimeout(peekTimer);
  }

  // History grouped by day: a header row, then the runs of that day. Rows have fixed heights so the list can be windowed.
  type HistoryItem = { kind: 'day'; key: string; label: string } | { kind: 'run'; key: string; run: ImageHistoryRun };
  const DAY_HEIGHT = 28;
  const RUN_HEIGHT = 52;
  const historyItems = $derived.by(() => {
    const items: HistoryItem[] = [];
    let day = '';
    for (const run of history) {
      const label = dayLabel(run.created_at);
      if (label !== day) {
        day = label;
        items.push({ kind: 'day', key: `day:${label}`, label });
      }
      items.push({ kind: 'run', key: run.id, run });
    }
    return items;
  });
  // TanStack Virtual (through z-ui), as the chat uses: only the rows in view are mounted.
  let historyScroller = $state<HTMLDivElement | undefined>();
  const historyKey = (index: number) => historyItems[index]?.key ?? index;
  const historyHeight = (index: number) => (historyItems[index]?.kind === 'day' ? DAY_HEIGHT : RUN_HEIGHT);
  const historyVirtualizer = createVirtualizer<HTMLDivElement, HTMLDivElement>({
    count: 0,
    getScrollElement: () => historyScroller ?? null,
    estimateSize: historyHeight,
    getItemKey: historyKey,
    overscan: 6,
  });
  $effect(() => {
    const count = historyItems.length;
    untrack(() => $historyVirtualizer.setOptions({ count, getScrollElement: () => historyScroller ?? null, estimateSize: historyHeight, getItemKey: historyKey, overscan: 6 }));
  });
  const historyRows = $derived($historyVirtualizer.getVirtualItems());

  async function openHistoryRun(run: ImageHistoryRun) {
    open = false;
    clearTimeout(peekTimer);
    peekOpen = false;
    await openRun(run);
  }
</script>

<ZDrawer bind:open zPlacement="right" zWidth="320px" zFloating zTitle="History" zBodyClass="p-0">
  <div class="flex h-full min-h-0 flex-col">
    {#if historyItems.length === 0}
      <DevEmpty icon="lucideImages" title="No history yet" hint="Each run you generate is kept here." />
    {:else}
      <div bind:this={historyScroller} class="min-h-0 flex-1 overflow-y-auto" aria-label="Image history">
        <div class="relative w-full" style:height="{$historyVirtualizer.getTotalSize()}px">
          {#each historyRows as row (row.key)}
            {@const item = historyItems[row.index]}
            {#if item?.kind === 'day'}
              <div class="absolute top-0 left-0 flex w-full items-center gap-2 px-2 text-[10px] font-medium tracking-wider text-muted-foreground uppercase" style:height="{row.size}px" style:transform="translateY({row.start}px)">
                <span class="shrink-0">{item.label}</span>
                <span class="h-px flex-1 bg-border" aria-hidden="true"></span>
              </div>
            {:else if item}
              <div class="absolute top-0 left-0 w-full px-1 py-0.5" style:height="{row.size}px" style:transform="translateY({row.start}px)">
                <button
                  type="button"
                  class="flex h-full w-full cursor-pointer flex-col justify-center gap-0.5 rounded-md px-2 text-left outline-none transition-colors hover:bg-accent focus-visible:bg-accent {activeRunId === item.run.id ? 'bg-primary/10' : ''}"
                  onclick={() => void openHistoryRun(item.run)}
                  onpointerenter={(event) => peekRun(item.run, event.currentTarget)}
                  onpointerleave={leavePeek}
                >
                  <span class="truncate text-sm">{item.run.prompt}</span>
                  <span class="flex min-w-0 items-center gap-1.5 text-[10px] text-muted-foreground">
                    <span class="shrink-0">{timeLabel(item.run.created_at)}</span>
                    <span class="truncate">· {item.run.models.length} model{item.run.models.length === 1 ? '' : 's'} · {item.run.images.length} image{item.run.images.length === 1 ? '' : 's'}</span>
                    {#if statusLabel(item.run.status)}<span class="shrink-0 text-warning">· {statusLabel(item.run.status)}</span>{/if}
                  </span>
                </button>
              </div>
            {/if}
          {/each}
        </div>
      </div>
    {/if}
  </div>
</ZDrawer>

<ZPopover
  anchor={peekAnchor}
  bind:open={peekOpen}
  zTrigger="manual"
  zPosition="left-top"
  zOffset={6}
  zWidth={256}
>
  {#snippet children()}
    <!-- The handlers sit on the content: the popover host overrides its own pointer handlers. -->
    <div class="max-h-28 overflow-y-auto p-2 text-sm leading-snug break-words whitespace-normal" role="presentation" onpointerenter={keepPeek} onpointerleave={leavePeek}>{peek?.prompt}</div>
  {/snippet}
</ZPopover>
