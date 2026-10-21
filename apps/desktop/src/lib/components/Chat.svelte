<script lang="ts">
  import { tick, untrack } from 'svelte';
  import { ZChatNavigator, ZIcon, createVirtualizer } from '@shival99/z-ui-svelte';
  import { messages } from '../mock';
  import { layout } from '../layout.svelte';
  import ChatBlock from './ChatBlock.svelte';
  import Composer from './Composer.svelte';
  import ToolbarActions from './ToolbarActions.svelte';

  let viewportEl: HTMLDivElement | undefined = $state();

  // ---------- virtual list (same approach as ZChat) ----------
  // Pinned to the bottom, follows new rows only when already there, so a reader scrolled into
  // history is never yanked down by an incoming reply.
  const virtualOptions = () => ({
    count: messages.length,
    getScrollElement: () => viewportEl ?? null,
    estimateSize: () => 96,
    getItemKey: (index: number) => messages[index]?.id ?? index,
    anchorTo: 'end' as const,
    followOnAppend: true,
    scrollEndThreshold: 80,
    overscan: 8,
    paddingStart: 8,
    paddingEnd: 8,
  });
  const virtualizer = createVirtualizer<HTMLDivElement, HTMLDivElement>(virtualOptions());

  $effect(() => {
    void viewportEl;
    const options = virtualOptions();
    untrack(() => $virtualizer.setOptions(options));
  });

  /** One scroll is not enough on a cold list: sizes move as rows are measured, so re-assert per frame. */
  let pinToken = 0;
  function pinToNewest() {
    const token = ++pinToken;
    let attemptsLeft = 12;
    const step = () => {
      if (token !== pinToken || attemptsLeft-- <= 0 || !viewportEl || messages.length === 0) return;
      if ($virtualizer.isAtEnd(1)) return;
      $virtualizer.scrollToEnd();
      requestAnimationFrame(step);
    };
    void tick().then(() => requestAnimationFrame(step));
  }

  $effect(() => {
    if (viewportEl) untrack(pinToNewest);
  });

  function measureRow(node: HTMLDivElement) {
    $virtualizer.measureElement(node);
  }

  // ---------- navigator: one tick per user question ----------
  const turnRows = $derived(messages.flatMap((item, index) => (item.role === 'user' ? [index] : [])));
  const navigatorTurns = $derived(
    turnRows.map((index) => {
      const answer = messages[index + 1];
      const firstText = (blocks: typeof messages[number]['blocks']) =>
        blocks.find((block) => block.kind === 'response');
      const question = firstText(messages[index].blocks);
      const reply = answer?.role === 'assistant' ? firstText(answer.blocks) : undefined;
      return {
        question: question?.kind === 'response' ? question.text : '',
        answer: reply?.kind === 'response' ? reply.text : undefined,
      };
    }),
  );

  /** The turn being read: the last question at or above the first visible row (the last one at the end). */
  const activeTurn = $derived.by(() => {
    if (turnRows.length === 0) return 0;
    if ($virtualizer.isAtEnd(1)) return turnRows.length - 1;
    const first = $virtualizer.range?.startIndex ?? 0;
    return Math.max(0, turnRows.filter((row) => row <= first).length - 1);
  });

  function jumpToTurn(turn: number) {
    const row = turnRows[turn];
    if (row !== undefined) $virtualizer.scrollToIndex(row, { align: 'start', behavior: 'smooth' });
  }
</script>

<main class="flex h-full min-h-0 min-w-0 flex-col" aria-label="Conversation">
  <header class="flex h-10 shrink-0 items-center border-b border-border {layout.toolsOpen ? '' : 'pr-33'}" data-tauri-drag-region>
    <!-- Title shares the message column, so it lines up with the messages below. -->
    <div class="mx-auto flex w-full max-w-3xl min-w-0 flex-1 items-center px-6" data-tauri-drag-region>
      <h1 class="min-w-0 truncate text-xs font-medium" data-tauri-drag-region>Build the settings panel</h1>
    </div>
    <!-- Actions stay pinned to the right edge (next to the window controls), not to the centered column. -->
    {#if !layout.toolsOpen}<ToolbarActions divider={false} />{/if}
  </header>

  <div class="relative flex min-h-0 flex-1 flex-col">
    <ZChatNavigator
      class="absolute top-1/2 left-1 z-10 w-6 -translate-y-1/2"
      turns={navigatorTurns}
      activeIndex={activeTurn}
      onSelect={jumpToTurn}
    />

    <div bind:this={viewportEl} class="min-h-0 flex-1 overflow-y-auto overflow-x-hidden">
      <div class="relative w-full" style:height="{$virtualizer.getTotalSize()}px" role="log" aria-live="polite" aria-label="Conversation">
        {#each $virtualizer.getVirtualItems() as row (row.key)}
          {@const message = messages[row.index]}
          {#if message}
            <div class="absolute top-0 left-0 w-full" data-index={row.index} style:transform="translateY({row.start}px)" {@attach measureRow}>
              <article class="mx-auto flex w-full max-w-3xl gap-3 px-6 py-3">
                {#if message.role === 'assistant'}
                  <div class="grid size-7 shrink-0 place-items-center rounded-lg bg-primary text-primary-foreground">
                    <ZIcon zType="lucideSparkles" class="size-3.5" />
                  </div>
                {:else}
                  <div class="grid size-7 shrink-0 place-items-center rounded-full bg-muted text-[10px]">JD</div>
                {/if}
                <div class="flex min-w-0 flex-1 flex-col gap-3">
                  <div class="text-xs font-semibold">
                    {message.role === 'assistant' ? 'Claude' : 'You'}
                    <span class="ml-1.5 font-normal text-muted-foreground">{message.time}</span>
                  </div>
                  {#each message.blocks as block}
                    <ChatBlock {block} />
                  {/each}
                </div>
              </article>
            </div>
          {/if}
        {/each}
      </div>
    </div>
  </div>

  <div class="shrink-0 px-6 pt-2 pb-4">
    <div class="mx-auto w-full max-w-3xl">
      <Composer />
      <p class="mt-2 text-center text-[10px] text-muted-foreground">AgentDeck can make mistakes. Review important changes.</p>
    </div>
  </div>
</main>
