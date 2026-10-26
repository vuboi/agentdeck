<script lang="ts">
  import { tick, untrack } from 'svelte';
  import { fly } from 'svelte/transition';
  import { ZIcon } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';

  // Ctrl+F for the whole window, as in a browser: every match of the text on screen is marked, Enter / Shift+Enter (or
  // the arrows) go to the next / previous one, Escape closes. Marks are CSS highlights (app.css `::highlight(find-*)`),
  // so the page itself is never changed. A virtual list only has its visible rows in the page: those are what is found.
  let { open = $bindable(false) }: { open?: boolean } = $props();

  let input: HTMLInputElement | undefined = $state();
  let bar: HTMLDivElement | undefined = $state();
  let query = $state('');
  let matchCase = $state(false);
  let count = $state(0);
  let current = $state(-1);
  let ranges: Range[] = [];
  const MAX = 2000;

  const supported = typeof CSS !== 'undefined' && 'highlights' in CSS;

  function collect(): Range[] {
    const needle = matchCase ? query : query.toLowerCase();
    if (!needle) return [];
    const found: Range[] = [];
    // Elements are walked first, so a hidden subtree (display: none, a closed panel, the find bar itself) is skipped
    // whole instead of asking every text node in it for its layout boxes: that was the lag while typing.
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_ELEMENT | NodeFilter.SHOW_TEXT, {
      acceptNode(node) {
        if (node.nodeType === Node.TEXT_NODE) return node.nodeValue ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_SKIP;
        const el = node as Element;
        if (/^(SCRIPT|STYLE|NOSCRIPT|TEMPLATE|SVG|INPUT|TEXTAREA)$/i.test(el.tagName) || el === bar || el.getAttribute('aria-hidden') === 'true' || el.hasAttribute('inert')) return NodeFilter.FILTER_REJECT;
        // `display: contents` (the app's own wrappers) has no box of its own, so checkVisibility() says false for it:
        // its children are still on screen.
        if ((el as HTMLElement).checkVisibility?.() === false && getComputedStyle(el).display !== 'contents') return NodeFilter.FILTER_REJECT;
        return NodeFilter.FILTER_SKIP;
      },
    });
    // The text of one block (a paragraph, a list item, a row) can be split over several nodes: a streamed answer wraps
    // each word in a span of its own. The nodes of a block are read as one string, so "xác nhận" is found across them;
    // a match never runs from one block into the next.
    const BLOCK = 'p,li,h1,h2,h3,h4,h5,h6,td,th,pre,blockquote,dt,dd,summary,label,button,a,[role=option],[role=treeitem],div';
    let nodes: Text[] = [];
    let starts: number[] = [];
    let text = '';
    let block: Element | null = null;
    const flush = () => {
      const hay = matchCase ? text : text.toLowerCase();
      for (let at = hay.indexOf(needle); at !== -1 && found.length < MAX; at = hay.indexOf(needle, at + needle.length)) {
        const end = at + needle.length;
        const from = lastStartAtOrBefore(starts, at);
        const to = lastStartAtOrBefore(starts, end - 1);
        const range = document.createRange();
        range.setStart(nodes[from], at - starts[from]);
        range.setEnd(nodes[to], end - starts[to]);
        found.push(range);
      }
      nodes = [];
      starts = [];
      text = '';
    };
    for (let node = walker.nextNode() as Text | null; node && found.length < MAX; node = walker.nextNode() as Text | null) {
      const owner = node.parentElement?.closest(BLOCK) ?? null;
      if (owner !== block) {
        flush();
        block = owner;
      }
      starts.push(text.length);
      nodes.push(node);
      text += node.nodeValue;
    }
    flush();
    return found;
  }

  /** The index of the last node that starts at or before `offset` (binary search over the start offsets). */
  function lastStartAtOrBefore(starts: number[], offset: number): number {
    let lo = 0;
    let hi = starts.length - 1;
    while (lo < hi) {
      const mid = (lo + hi + 1) >> 1;
      if (starts[mid] <= offset) lo = mid;
      else hi = mid - 1;
    }
    return lo;
  }

  function paint() {
    if (!supported) return;
    CSS.highlights.set('find-all', new Highlight(...ranges));
    if (current >= 0 && ranges[current]) CSS.highlights.set('find-current', new Highlight(ranges[current]));
    else CSS.highlights.delete('find-current');
  }

  function clear() {
    ranges = [];
    count = 0;
    current = -1;
    if (supported) {
      CSS.highlights.delete('find-all');
      CSS.highlights.delete('find-current');
    }
  }

  function reveal() {
    const range = ranges[current];
    const el = range?.startContainer.parentElement;
    if (!el) return;
    const box = range.getBoundingClientRect();
    if (box.top < 60 || box.bottom > innerHeight - 40) el.scrollIntoView({ block: 'center', behavior: 'instant' });
  }

  /** Finds again (the page may have changed: a chat scrolled, a panel opened) and keeps the place in the list. */
  function search(step = 0) {
    const before = ranges[current];
    ranges = collect();
    count = ranges.length;
    if (!count) {
      current = -1;
      paint();
      return;
    }
    // Stay on the match that was current, or the first one after it in the page.
    let at = before ? ranges.findIndex((range) => range.compareBoundaryPoints(Range.START_TO_START, before) >= 0) : 0;
    if (at === -1) at = 0;
    current = step === 0 ? at : (at + step + count) % count;
    paint();
    reveal();
  }

  let timer: ReturnType<typeof setTimeout> | undefined;
  function onInput() {
    clearTimeout(timer);
    current = -1;
    timer = setTimeout(() => search(0), 80);
  }

  function next(step: number) {
    if (!query) return;
    if (!count) return search(0);
    // A match that left the page (its row scrolled away) is found again from the current place.
    const stale = ranges.some((range) => !range.startContainer.isConnected);
    if (stale) return search(step);
    current = (current + step + count) % count;
    paint();
    reveal();
  }

  function onKey(event: KeyboardEvent) {
    if (event.key === 'Enter') {
      event.preventDefault();
      next(event.shiftKey ? -1 : 1);
    } else if (event.key === 'Escape') {
      event.preventDefault();
      open = false;
    }
  }

  /** Called again while open (Ctrl+F once more): the box is focused with its text selected. */
  export async function focus() {
    await tick();
    const picked = getSelection()?.toString().trim();
    if (picked && picked.length < 200 && !picked.includes('\n')) {
      query = picked;
      search(0);
    }
    // The webview finishes handling the key after this runs and can take the focus back: try again for a few frames.
    for (let attempt = 0; attempt < 5 && document.activeElement !== input; attempt++) {
      input?.focus();
      input?.select();
      await new Promise((resolve) => setTimeout(resolve, 30));
    }
  }

  // Only on open / close: reading `query` here made every key typed run focus() again, which selected the text typed so
  // far (and the next key replaced it).
  $effect(() => {
    if (open)
      untrack(() => {
        void focus();
        if (query) search(0);
      });
    else untrack(clear);
  });
</script>

{#if open}
  <!-- Built like the pickers' search: the field runs edge to edge on top, the controls in a row under a hairline. -->
  <div
    bind:this={bar}
    class="fixed top-12 right-4 z-[70] w-[340px] overflow-hidden rounded-xl border border-border bg-popover text-popover-foreground shadow-xl"
    role="search"
    aria-label="Find in the app"
    data-find-skip
    transition:fly={{ y: -8, duration: 160 }}
  >
    <label class="flex h-10 items-center gap-2 border-b border-border px-3">
      <ZIcon zType={'lucideSearch' as ZIconName} class="size-4 shrink-0 text-muted-foreground" />
      <input
        bind:this={input}
        bind:value={query}
        oninput={onInput}
        onkeydown={onKey}
        class="h-full min-w-0 flex-1 bg-transparent text-sm outline-none placeholder:text-muted-foreground"
        placeholder="Find in the app"
        aria-label="Find in the app"
        spellcheck="false"
      />
      <button type="button" class="grid size-7 shrink-0 cursor-pointer place-items-center rounded-md text-xs font-semibold outline-none hover:bg-muted {matchCase ? 'bg-primary/15! text-primary!' : 'text-muted-foreground'}" aria-pressed={matchCase} title="Match case" onclick={() => ((matchCase = !matchCase), search(0))}>Aa</button>
      <button type="button" class="-mr-1 grid size-7 shrink-0 cursor-pointer place-items-center rounded-md text-muted-foreground outline-none hover:bg-muted hover:text-foreground" aria-label="Close" title="Close (Esc)" onclick={() => (open = false)}>
        <ZIcon zType={'lucideX' as ZIconName} class="size-4" />
      </button>
    </label>
    <div class="flex h-9 items-center gap-1 px-1.5">
      <button type="button" class="grid size-7 cursor-pointer place-items-center rounded-md text-muted-foreground outline-none hover:bg-muted hover:text-foreground disabled:opacity-40" aria-label="Previous match" title="Previous (Shift+Enter)" disabled={!count} onclick={() => next(-1)}>
        <ZIcon zType={'lucideArrowUp' as ZIconName} class="size-4" />
      </button>
      <button type="button" class="grid size-7 cursor-pointer place-items-center rounded-md text-muted-foreground outline-none hover:bg-muted hover:text-foreground disabled:opacity-40" aria-label="Next match" title="Next (Enter)" disabled={!count} onclick={() => next(1)}>
        <ZIcon zType={'lucideArrowDown' as ZIconName} class="size-4" />
      </button>
      <span class="mr-1.5 ml-auto text-xs text-muted-foreground tabular-nums" role="status">
        {#if !supported}This window cannot mark matches{:else if !query}Type to find{:else if count === 0}No results{:else}{current + 1} / {count >= MAX ? `${MAX}+` : count} results{/if}
      </span>
    </div>
  </div>
{/if}
