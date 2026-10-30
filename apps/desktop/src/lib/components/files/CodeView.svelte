<script lang="ts">
  import { tick, untrack } from 'svelte';
  import { createVirtualizer } from '@shival99/z-ui-svelte';
  import { observedMeasure } from '../../app/ui/virtual';
  import { highlightTokens, type Token } from '../../files/highlight';
  import { theme } from '../../app/ui/theme.svelte';

  // Read-only source view. Only the rows on screen are mounted, so a 1M-line file stays light. Colours arrive in
  // chunks for the rows in view: the plain text is there at once, no spinner. With `wrap` long lines fold (rows
  // are measured); without it they scroll sideways. A search `query` marks every match and the current one stronger.
  let {
    text,
    lang,
    added,
    removed,
    modified,
    wrap = true,
    query = '',
    matchCase = false,
    wholeWord = false,
    regex = false,
    current = 0,
    total = $bindable(0),
    invalid = $bindable(false),
  }: {
    text: string;
    lang?: string;
    /** Lines (1-based) a change added: marked in green. */
    added?: ReadonlySet<number>;
    /** Lines (1-based) where lines were taken out: marked in red. */
    removed?: ReadonlySet<number>;
    /** Lines (1-based) that replace others: marked in blue. */
    modified?: ReadonlySet<number>;
    wrap?: boolean;
    query?: string;
    matchCase?: boolean;
    wholeWord?: boolean;
    regex?: boolean;
    /** The query is a regular expression that does not parse. */
    invalid?: boolean;
    /** Index of the match to show as the current one. */
    current?: number;
    /** Number of matches, reported back. */
    total?: number;
  } = $props();

  export const goToLine = (line: number) => $virtualizer.scrollToIndex(line - 1, { align: 'center' });
  /** Classes of the line-number cell: green for an added line, red where lines were taken out. */
  const numberClass = (added: boolean, cut: boolean, mod: boolean) =>
    mod ? 'font-bold text-blue-500 shadow-[inset_2px_0_0_var(--color-blue-500)]' : added ? 'font-bold text-emerald-500 shadow-[inset_2px_0_0_var(--color-emerald-500)]' : cut ? 'font-bold text-red-500 shadow-[inset_2px_0_0_var(--color-red-500)]' : 'text-muted-foreground/60';
  const ROW = 20; // px, equals leading-5
  const CHUNK = 80; // lines per tokenizer call: small, so one call never holds a frame for long
  const MAX_MATCHES = 20_000;
  const lines = $derived(text.split('\n'));

  type Match = { line: number; start: number; end: number };
  const pattern = $derived.by(() => {
    if (!query) return null;
    try {
      const body = regex ? query : query.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
      return new RegExp(wholeWord ? `\\b(?:${body})\\b` : body, matchCase ? 'g' : 'gi');
    } catch {
      return undefined;
    }
  });
  $effect(() => {
    invalid = pattern === undefined;
  });
  const found = $derived.by(() => {
    const out: Match[] = [];
    if (!pattern) return out;
    for (let line = 0; line < lines.length && out.length < MAX_MATCHES; line++) {
      pattern.lastIndex = 0;
      for (let hit = pattern.exec(lines[line]); hit && out.length < MAX_MATCHES; hit = pattern.exec(lines[line])) {
        if (hit[0] === '') pattern.lastIndex += 1;
        else out.push({ line, start: hit.index, end: hit.index + hit[0].length });
      }
    }
    return out;
  });
  const byLine = $derived.by(() => {
    const map = new Map<number, { match: Match; index: number }[]>();
    found.forEach((match, index) => {
      const list = map.get(match.line);
      if (list) list.push({ match, index });
      else map.set(match.line, [{ match, index }]);
    });
    return map;
  });
  $effect(() => {
    total = found.length;
  });

  let scroller: HTMLDivElement | undefined = $state();
  // Plain map, not $state: one version counter tells the template when a chunk has landed.
  const tokens = new Map<number, Token[]>();
  let version = $state(0);
  let generation = 0;

  // Wrapped rows are measured from the ResizeObserver, not by reading offsetHeight as each one mounts (virtual.ts).
  const sizing = observedMeasure(() => ROW);
  const virtualizer = createVirtualizer<HTMLDivElement, HTMLDivElement>({
    count: 0,
    getScrollElement: () => scroller ?? null,
    estimateSize: () => ROW,
    measureElement: sizing.measureElement,
    overscan: 30,
  });

  $effect(() => {
    const count = lines.length;
    untrack(() => $virtualizer.setOptions({ count, getScrollElement: () => scroller ?? null, estimateSize: () => ROW, measureElement: sizing.measureElement, overscan: 30 }));
  });

  // Folded rows have a height of their own: measured once mounted (see `measure`).
  function measure(node: HTMLElement) {
    $virtualizer.measureElement(node as HTMLDivElement);
  }
  $effect(() => {
    void wrap;
    void text;
    untrack(() => {
      sizing.reset();
      $virtualizer.measure();
    });
  });

  // New file or theme: drop the old colours and start again.
  $effect(() => {
    void text;
    void lang;
    void theme.dark;
    untrack(() => {
      generation += 1;
      tokens.clear();
      version += 1;
    });
  });

  const items = $derived($virtualizer.getVirtualItems());

  // Colour the rows in view that are still plain, one chunk at a time.
  async function fill(gen: number, from: number, to: number) {
    if (!lang) return;
    let start = -1;
    for (let i = from; i <= to && i < lines.length; i++) {
      if (!tokens.has(i)) {
        start = i;
        break;
      }
    }
    if (start === -1) return;
    const end = Math.min(start + CHUNK, lines.length);
    const result = await highlightTokens(lines.slice(start, end).join('\n'), lang, theme.dark);
    if (gen !== generation || !result) return; // no result: unknown language, the rows stay plain
    result.forEach((row, offset) => tokens.set(start + offset, row));
    version += 1;
    await fill(gen, from, to);
  }

  $effect(() => {
    const first = items[0]?.index ?? 0;
    const last = items[items.length - 1]?.index ?? 0;
    const gen = generation;
    untrack(() => void fill(gen, first, last));
  });

  type Piece = { text: string; color?: string; mark: 0 | 1 | 2 };
  /** The pieces of one line: its colour tokens cut at the edges of the search matches. */
  function pieces(index: number): Piece[] {
    const row: { content: string; color?: string }[] = tokens.get(index) ?? [{ content: lines[index] ?? '' }];
    const marks = byLine.get(index);
    if (!marks) return row.map((token) => ({ text: token.content, color: token.color, mark: 0 }));
    const out: Piece[] = [];
    let offset = 0;
    for (const token of row) {
      let from = 0;
      const base = offset;
      for (const { match, index: n } of marks) {
        const s = Math.max(match.start - base, from);
        const e = Math.min(match.end - base, token.content.length);
        if (e <= s) continue;
        if (s > from) out.push({ text: token.content.slice(from, s), color: token.color, mark: 0 });
        out.push({ text: token.content.slice(s, e), color: token.color, mark: n === current ? 2 : 1 });
        from = e;
      }
      if (from < token.content.length) out.push({ text: token.content.slice(from), color: token.color, mark: 0 });
      offset += token.content.length;
    }
    return out;
  }

  // Bring the current match into view: its row first (it may not be mounted yet), then sideways if lines do not fold.
  $effect(() => {
    const match = found[current];
    void query;
    if (!match) return;
    untrack(() => {
      $virtualizer.scrollToIndex(match.line, { align: 'center' });
      void tick().then(() => requestAnimationFrame(() => scroller?.querySelector('[data-current-match]')?.scrollIntoView({ block: 'nearest', inline: 'center' })));
    });
  });
</script>

<div bind:this={scroller} class="h-full min-h-0 overflow-auto bg-background font-mono text-sm leading-5">
  <div class="relative min-w-full" style:height="{$virtualizer.getTotalSize()}px">
    {#key wrap}
      {#each items as item (item.key)}
        {@const isAdded = added?.has(item.index + 1) ?? false}
        {@const isCut = removed?.has(item.index + 1) ?? false}
        {@const isMod = modified?.has(item.index + 1) ?? false}
        <div
          data-index={item.index}
          use:measure
          class="absolute top-0 left-0 flex {wrap ? 'w-full whitespace-pre-wrap' : 'w-max min-w-full whitespace-pre'} {isAdded ? 'bg-emerald-500/15' : isMod ? 'bg-blue-500/15' : isCut ? 'bg-red-500/15' : ''}"
          style:height={wrap ? undefined : `${ROW}px`}
          style:min-height="{ROW}px"
          style:transform="translateY({item.start}px)"
        >
          <span class="sticky left-0 z-10 w-14 shrink-0 bg-background pr-3 text-right select-none {numberClass(isAdded, isCut, isMod)}" data-mark={isAdded ? 'add' : isCut ? 'del' : undefined}>{item.index + 1}</span>
          <span class="text-foreground/90 {wrap ? 'min-w-0 flex-1 pr-4 [overflow-wrap:anywhere]' : 'pr-6'}"
            >{#each version >= 0 ? pieces(item.index) : [] as piece, n (n)}{#if piece.mark}<mark data-current-match={piece.mark === 2 ? '' : undefined} class="rounded-[2px] text-inherit {piece.mark === 2 ? 'bg-primary/60 outline outline-1 outline-primary' : 'bg-primary/25'}" style:color={piece.color}>{piece.text}</mark>{:else}<span style:color={piece.color}>{piece.text}</span>{/if}{/each}</span>
        </div>
      {/each}
    {/key}
  </div>
</div>

<style>
  [data-mark] {
    position: relative;
  }
  [data-mark]::before {
    content: '+';
    position: absolute;
    left: 6px;
  }
  [data-mark='del']::before {
    content: '2';
  }
</style>
