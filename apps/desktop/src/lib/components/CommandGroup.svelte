<script lang="ts">
  import { slide } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { ZIcon, ZLoading, ZShimmerText } from '@shival99/z-ui-svelte';
  import { ticker } from '../clock.svelte';
  import { argOf, describeStep, elapsed, liveState, summarize, toolOf, totalChange, type CommandBlock, type Counts, type ThinkingBlock } from '../blocks';
  import { openImages } from '../preview.svelte';
  import { imagePathsIn, loadImagePath } from '../attachments';

  // A screenshot step: click its picture to see it large.
  const showShot = (event: Event, src: string) => {
    event.stopPropagation();
    openImages([{ src, name: 'screenshot.png' }], 0);
  };

  // The steps an agent took, as one quiet line: "Created 2 files, ran 8 commands  +222 −0". While the agent
  // is still working that line carries the only loader, with the current step and a running clock; open it
  // for the list of steps, open a step for its command, output or diff.
  let { items, thoughts = [], live = false, startedAt }: { items: CommandBlock[]; thoughts?: ThinkingBlock[]; live?: boolean; startedAt?: number } = $props();

  // A tool that saved its picture and returned only the file path: the picture is read from disk once the step is done.
  let fromPaths = $state<Record<string, string>>({});
  const pathsOf = (item: CommandBlock) => (item.image || item.status === 'running' || toolOf(item.command) === 'Bash' && /(rm|del|Remove-Item)/.test(item.command) ? [] : imagePathsIn(item.output ?? ''));
  $effect(() => {
    for (const path of items.flatMap(pathsOf)) {
      if (path in fromPaths) continue;
      fromPaths[path] = '';
      void loadImagePath(path).then((src) => {
        if (src) fromPaths[path] = src;
      });
    }
  });
  const shots = $derived(
    items.flatMap((item, index) =>
      item.image
        ? [{ src: item.image, name: `screenshot-${index + 1}.png` }]
        : pathsOf(item).flatMap((path) => (fromPaths[path] ? [{ src: fromPaths[path], name: path.split(/[\/]/).pop() ?? 'image.png' }] : [])),
    ),
  );
  let open = $state(false);
  let thinkingOpen = $state(false);
  // Reasoning between the steps, in the order it came, as one text.
  const thinkingText = $derived(thoughts.map((thought) => thought.text.trim()).join('\n\n'));
  const thinkingNow = $derived(thoughts.some((thought) => thought.streaming));
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

{#snippet chips(counts: Counts)}
  <!-- Only the non-zero parts: files created or deleted, then lines added or removed. -->
  <span class="flex shrink-0 items-center gap-1 font-mono text-[10px] leading-none">
    {#if counts.created}<span class="rounded bg-emerald-500/15 px-1 py-0.5 text-emerald-500">+{counts.created} file{counts.created === 1 ? '' : 's'}</span>{/if}
    {#if counts.deleted}<span class="rounded bg-destructive/15 px-1 py-0.5 text-destructive">−{counts.deleted} file{counts.deleted === 1 ? '' : 's'}</span>{/if}
    {#if counts.added}<span class="rounded bg-emerald-500/15 px-1 py-0.5 text-emerald-500">+{counts.added}</span>{/if}
    {#if counts.removed}<span class="rounded bg-destructive/15 px-1 py-0.5 text-destructive">−{counts.removed}</span>{/if}
  </span>
{/snippet}

<div class="block-in flex flex-col text-xs">
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
      {#if thoughts.length > 0}<ZIcon zType="lucideBrain" class="size-3 shrink-0" />{/if}
      {#if items.some((item) => item.status === 'running')}
        <ZShimmerText class="truncate font-medium" zText={summarize(items)} />
      {:else}
        <span class="truncate font-medium">{summarize(items)}</span>
      {/if}
      {#if total}{@render chips(total)}{/if}
    {/if}
    <ZIcon zType="lucideChevronDown" class="size-3 shrink-0 transition-transform duration-300 {open ? 'rotate-180' : ''}" />
  </button>

  <div class="grid transition-[grid-template-rows] duration-300 ease-out motion-reduce:transition-none {open ? 'grid-rows-[1fr]' : 'grid-rows-[0fr]'}">
    <div class="min-h-0 overflow-hidden">
      <div class="mt-1.5 divide-y divide-border overflow-hidden rounded-xl border border-border">
        {#if thoughts.length > 0}
          <div>
            <button type="button" class="flex h-9 w-full cursor-pointer items-center gap-2 px-3 text-left outline-none hover:bg-muted/40 focus-visible:ring-2 focus-visible:ring-ring/60 focus-visible:ring-inset" aria-expanded={thinkingOpen} onclick={() => (thinkingOpen = !thinkingOpen)}>
              <ZIcon zType="lucideBrain" class="size-3.5 shrink-0 text-muted-foreground" />
              <span class="font-medium text-foreground">Thinking</span>
              <span class="text-muted-foreground">{thoughts.length === 1 ? '' : `${thoughts.length} times`}</span>
              {#if thinkingNow}<ZLoading zLoading zSpinner="orbit" zSize="2xs" zColor="primary" />{/if}
              <ZIcon zType="lucideChevronRight" class="ml-auto size-3.5 shrink-0 text-muted-foreground transition-transform duration-300 {thinkingOpen ? 'rotate-90' : ''}" />
            </button>
            <div class="grid transition-[grid-template-rows] duration-300 ease-out motion-reduce:transition-none {thinkingOpen ? 'grid-rows-[1fr]' : 'grid-rows-[0fr]'}">
              <div class="min-h-0 overflow-hidden">
                <p class="m-0 max-h-72 overflow-auto px-3 pt-1 pb-3 leading-relaxed whitespace-pre-wrap text-muted-foreground">{thinkingText}</p>
              </div>
            </div>
          </div>
        {/if}
        {#each items as item, index (item.id ?? index)}
          {@const step = describeStep(item)}
          <div in:slide={{ duration: 220, easing: cubicOut }}>
            <button
              type="button"
              class="flex h-9 w-full cursor-pointer items-center gap-2 px-3 text-left outline-none hover:bg-muted/40 focus-visible:ring-2 focus-visible:ring-ring/60 focus-visible:ring-inset"
              aria-expanded={rows[index] ?? false}
              onclick={() => (rows[index] = !rows[index])}
            >
              {#if item.status === 'running'}
                <ZShimmerText class="shrink-0" zText={step.verb} />
                <ZShimmerText class="min-w-0 truncate font-medium" zText={step.subject || '…'} />
              {:else}
                <span class="shrink-0 {item.status === 'fail' ? 'text-destructive' : 'text-muted-foreground'}">{step.verb}</span>
                <span class="min-w-0 truncate font-medium {item.status === 'fail' ? 'text-destructive' : 'text-foreground'}">{step.subject}</span>
              {/if}
              {#if step.counts}{@render chips(step.counts)}{/if}
              {#if item.image}
                <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
                <span class="ml-1 shrink-0 cursor-zoom-in" title="See the screenshot" onclick={(event) => showShot(event, item.image!)}>
                  <img src={item.image} alt="Screenshot" class="h-5 w-8 rounded-sm border border-border object-cover object-top transition hover:ring-2 hover:ring-primary/50" />
                </span>
              {/if}
              <ZIcon zType="lucideChevronRight" class="ml-auto size-3.5 shrink-0 text-muted-foreground transition-transform duration-300 {rows[index] ? 'rotate-90' : ''}" />
            </button>
            <div class="grid transition-[grid-template-rows] duration-300 ease-out motion-reduce:transition-none {rows[index] ? 'grid-rows-[1fr]' : 'grid-rows-[0fr]'}">
              <div class="min-h-0 overflow-hidden">
                <div class="flex flex-col gap-2 px-3 pt-1 pb-3">
                  {#if item.change}
                    <div class="max-h-60 overflow-auto rounded-lg border border-border bg-background py-1 font-mono text-[11px] leading-relaxed">
                      <div class="px-3 pb-1 text-muted-foreground">{item.change.path}</div>
                      {#each diffLines(item) as row}
                        <div class="px-3 whitespace-pre-wrap break-all {row.sign === '+' ? 'bg-emerald-500/10 text-emerald-500' : 'bg-destructive/10 text-destructive'}">{row.sign} {row.line}</div>
                      {/each}
                    </div>
                  {:else}
                    <pre class="m-0 overflow-x-auto rounded-md border border-border bg-background px-3 py-2 font-mono text-[11px] leading-relaxed whitespace-pre-wrap break-all">{shown(item)}</pre>
                  {/if}
                  {#if item.image}
                    <button type="button" class="w-fit max-w-full cursor-zoom-in overflow-hidden rounded-md border border-border outline-none hover:ring-2 hover:ring-primary/50" aria-label="See the screenshot" onclick={(event) => showShot(event, item.image!)}>
                      <img src={item.image} alt="Screenshot" class="block max-h-48 max-w-full object-contain" />
                    </button>
                  {/if}
                  {#if item.output && !item.change}
                    <pre class="m-0 max-h-60 overflow-auto px-1 font-mono text-[11px] leading-relaxed whitespace-pre-wrap break-all {item.status === 'fail' ? 'text-destructive' : 'text-muted-foreground'}">{item.output}</pre>
                  {/if}
                </div>
              </div>
            </div>
          </div>
        {/each}
      </div>
    </div>
  </div>
  {#if shots.length > 0}
    <!-- Pictures the tools returned (screenshots), shown in the chat too, not only inside the opened steps. -->
    <div class="mt-1.5 flex flex-wrap gap-2" aria-label="Images from the tools">
      {#each shots as shot, position (position)}
        <button type="button" class="block cursor-zoom-in overflow-hidden rounded-lg border border-border outline-none transition hover:ring-2 hover:ring-primary/50 focus-visible:ring-2 focus-visible:ring-ring" aria-label="Open image {position + 1} of {shots.length}" onclick={() => openImages(shots, position)}>
          <img src={shot.src} alt={shot.name} class="block max-h-36 max-w-60 object-contain" />
        </button>
      {/each}
    </div>
  {/if}
</div>
