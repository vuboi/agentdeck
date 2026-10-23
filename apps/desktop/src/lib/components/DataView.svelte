<script lang="ts">
  import { ZIcon } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { toast } from '../toast';
  import CodeView from './CodeView.svelte';

  // A payload or a response, laid out to read: JSON as a tree whose objects and arrays fold and unfold, a form body as
  // the JSON object it stands for, anything else line by line. Only the rows on screen are drawn (every row is one
  // line high), so a response of many thousands of nodes or lines scrolls as smoothly as a small one. The copy
  // buttons give the whole text, or one part of the tree, in full.
  let { text, label = '', fill = false, lang }: { text: string; label?: string; fill?: boolean; lang?: string } = $props();

  const OPEN_DEPTH = 2;
  const ROW = 20;
  /** A line longer than this is cut on screen (copying still gives all of it). */
  const CUT = 4000;
  const PARSE_LIMIT = 8_000_000;

  const parsed = $derived.by(() => {
    const raw = text.trim();
    if (raw.length === 0 || raw.length > PARSE_LIMIT) return null;
    if (!(raw.startsWith('{') || raw.startsWith('['))) return null;
    try {
      return JSON.parse(raw) as unknown;
    } catch {
      return null;
    }
  });
  // A form body (a=1&b=2) is shown as the JSON object it stands for.
  const fromForm = $derived.by(() => {
    if (parsed !== null) return null;
    const raw = text.trim();
    if (!raw || raw.length > 100_000 || /[\s<>{}]/.test(raw) || !/^[^=&]+=[^&]*(&[^=&]+=[^&]*)*$/.test(raw)) return null;
    try {
      const out: Record<string, string | string[]> = {};
      for (const [name, value] of new URLSearchParams(raw).entries()) {
        const old = out[name];
        out[name] = old === undefined ? value : Array.isArray(old) ? [...old, value] : [old, value];
      }
      return out;
    } catch {
      return null;
    }
  });
  const data = $derived(parsed ?? fromForm);
  const kind = $derived(parsed !== null ? 'JSON' : fromForm ? 'Form' : (lang ?? 'Text'));
  const lineCount = $derived(data === null ? text.split('\n').length : 0);

  // What the user folded or unfolded, by path; anything not touched follows the default depth.
  let overrides = $state<Record<string, boolean>>({});
  $effect(() => {
    void text;
    overrides = {};
    scrollTop = 0;
    if (scroller) scroller.scrollTop = 0;
  });
  const isOpen = (path: string, depth: number) => overrides[path] ?? depth < OPEN_DEPTH;
  const toggle = (path: string, depth: number) => (overrides[path] = !isOpen(path, depth));

  type Row = {
    id: string;
    kind: 'open' | 'close' | 'leaf' | 'empty' | 'line';
    depth: number;
    name: string | null;
    value: unknown;
    comma: boolean;
    open: boolean;
    array: boolean;
    count: number;
    path: string;
  };
  const entriesOf = (value: object): [string, unknown][] => (Array.isArray(value) ? value.map((item, index) => [String(index), item]) : Object.entries(value));

  // The rows that are visible now: a folded object is one row, an open one is its head, its children and its closing bracket.
  const rows = $derived.by<Row[]>(() => {
    if (data === null) return [];
    const out: Row[] = [];
    const walk = (value: unknown, name: string | null, path: string, depth: number, comma: boolean) => {
      if (value !== null && typeof value === 'object') {
        const array = Array.isArray(value);
        const entries = entriesOf(value);
        if (entries.length === 0) {
          out.push({ id: path, kind: 'empty', depth, name, value, comma, open: false, array, count: 0, path });
          return;
        }
        const open = isOpen(path, depth);
        out.push({ id: path, kind: 'open', depth, name, value, comma, open, array, count: entries.length, path });
        if (!open) return;
        entries.forEach(([key, item], index) => walk(item, array ? null : key, `${path}/${key}`, depth + 1, index < entries.length - 1));
        out.push({ id: `${path}~`, kind: 'close', depth, name: null, value, comma, open: true, array, count: 0, path });
      } else {
        out.push({ id: path, kind: 'leaf', depth, name, value, comma, open: false, array: false, count: 0, path });
      }
    };
    walk(data, null, '', 0, false);
    return out;
  });

  function setAll(open: boolean) {
    const next: Record<string, boolean> = {};
    const walk = (value: unknown, path: string, depth: number) => {
      if (value === null || typeof value !== 'object') return;
      next[path] = open || depth === 0;
      for (const [key, item] of Object.entries(value)) walk(item, `${path}/${key}`, depth + 1);
    };
    walk(data, '', 0);
    overrides = next;
  }

  // Windowing: the rows from a little above the top of the view to a little below the bottom.
  let scroller = $state<HTMLElement | null>(null);
  let scrollTop = $state(0);
  let viewHeight = $state(320);
  const first = $derived(Math.max(0, Math.floor(scrollTop / ROW) - 8));
  const last = $derived(Math.min(rows.length, Math.ceil((scrollTop + viewHeight) / ROW) + 8));
  const slice = $derived(rows.slice(first, last));

  const tone = (value: unknown) => (typeof value === 'string' ? 'text-success' : typeof value === 'number' ? 'text-warning' : 'text-destructive/80');
  const shown = (value: unknown) => {
    const out = JSON.stringify(value);
    return out.length > CUT ? `${out.slice(0, CUT)}…` : out;
  };
  const brackets = (row: Row) => (row.array ? ['[', ']'] : ['{', '}']);
  const summary = (row: Row) => `${row.count} ${row.array ? (row.count === 1 ? 'item' : 'items') : row.count === 1 ? 'key' : 'keys'}`;

  async function write(out: string, what: string) {
    try {
      await navigator.clipboard.writeText(out);
      toast.success(`Copied${what ? ` ${what}` : ''}`);
    } catch {
      toast.error('Could not copy');
    }
  }
  const copyAll = () => write(data === null ? text : JSON.stringify(data, null, 2), label);
  /** One part of the JSON: an object or array as indented JSON, a string as its text, anything else as written. */
  const copyPart = (value: unknown) => write(value !== null && typeof value === 'object' ? JSON.stringify(value, null, 2) : typeof value === 'string' ? value : String(value), '');

  const small = 'grid size-5 cursor-pointer place-items-center rounded bg-background/80 text-muted-foreground opacity-0 outline-none group-hover:opacity-100 hover:text-primary focus-visible:opacity-100';
  const part = 'absolute top-0.5 right-1 grid size-4 cursor-pointer place-items-center rounded bg-background/90 text-muted-foreground opacity-0 outline-none group-hover/row:opacity-100 hover:text-primary focus-visible:opacity-100';
</script>

<div class="group relative flex min-h-0 flex-col rounded border border-border/60 bg-muted/20 {fill ? 'h-full' : ''}">
  <div class="absolute top-1 right-3 z-10 flex items-center gap-1">
    <span class="rounded bg-background/80 px-1 text-[10px] leading-4 text-muted-foreground">{kind}</span>
    {#if data !== null}
      <button type="button" class="{small}" aria-label="Expand all" title="Expand all" onclick={() => setAll(true)}>
        <ZIcon zType={'lucideChevronsUpDown' as ZIconName} zSize="12" />
      </button>
      <button type="button" class="{small}" aria-label="Collapse all" title="Collapse all" onclick={() => setAll(false)}>
        <ZIcon zType={'lucideChevronsDownUp' as ZIconName} zSize="12" />
      </button>
    {/if}
    <button type="button" class="{small}" aria-label="Copy{label ? ` ${label}` : ''}" title="Copy all" onclick={() => void copyAll()}>
      <ZIcon zType={'lucideCopy' as ZIconName} zSize="12" />
    </button>
  </div>

  {#if data === null}
    <div class="min-h-0 {fill ? 'flex-1' : ''}" style:height={fill ? undefined : `${Math.min(lineCount * ROW + 8, 416)}px`}><CodeView {text} {lang} /></div>
  {:else}
  <div
    bind:this={scroller}
    bind:clientHeight={viewHeight}
    onscroll={() => (scrollTop = scroller?.scrollTop ?? 0)}
    class="min-h-0 overflow-auto {fill ? 'flex-1' : ''}"
    style:max-height={fill ? undefined : '26rem'}
    role={data === null ? 'document' : 'tree'}
  >
    <div style:height="{rows.length * ROW}px" style:padding-top="{first * ROW}px" style:box-sizing="border-box" style:min-width="max-content" class="w-full">
      {#each slice as row (row.id)}
        {#if row.kind === 'line'}
          <div class="h-5 pr-24 pl-2 leading-5 whitespace-pre text-foreground">{String(row.value).length > CUT ? `${String(row.value).slice(0, CUT)}…` : row.value}</div>
        {:else}
          <div class="group/row relative flex h-5 items-center pr-24 leading-5 whitespace-pre" style:padding-left="{8 + row.depth * 14}px" role="treeitem" aria-selected="false" aria-expanded={row.kind === 'open' ? row.open : undefined}>
            {#if row.kind === 'open'}
              <button type="button" class="grid size-4 shrink-0 cursor-pointer place-items-center rounded text-muted-foreground outline-none hover:text-primary" aria-label={row.open ? 'Collapse' : 'Expand'} onclick={() => toggle(row.path, row.depth)}>
                <ZIcon zType={'lucideChevronRight' as ZIconName} zSize="12" class="transition-transform {row.open ? 'rotate-90' : ''}" />
              </button>
            {:else}
              <span class="size-4 shrink-0"></span>
            {/if}
            {#if row.kind === 'close'}
              <span class="text-muted-foreground">{brackets(row)[1]}{row.comma ? ',' : ''}</span>
            {:else}
              {#if row.name !== null}<span class="text-primary">"{row.name}"</span><span class="text-muted-foreground">:&nbsp;</span>{/if}
              {#if row.kind === 'leaf'}
                <span class={tone(row.value)} title={typeof row.value === 'string' && row.value.length > 80 ? row.value.slice(0, 600) : undefined}>{shown(row.value)}</span>{#if row.comma}<span class="text-muted-foreground">,</span>{/if}
              {:else if row.kind === 'empty'}
                <span class="text-muted-foreground">{brackets(row)[0]}{brackets(row)[1]}{row.comma ? ',' : ''}</span>
              {:else}
                <span class="text-muted-foreground">{brackets(row)[0]}</span>
                {#if !row.open}
                  <button type="button" class="mx-1 cursor-pointer rounded px-1 leading-4 text-muted-foreground italic outline-none hover:bg-primary/10 hover:text-primary" onclick={() => toggle(row.path, row.depth)}>{summary(row)}</button><span class="text-muted-foreground">{brackets(row)[1]}{row.comma ? ',' : ''}</span>
                {/if}
              {/if}
              <button type="button" class={part} aria-label="Copy this part" title="Copy" onclick={() => void copyPart(row.value)}>
                <ZIcon zType={'lucideCopy' as ZIconName} zSize="10" />
              </button>
            {/if}
          </div>
        {/if}
      {/each}
    </div>
  </div>
  {/if}
</div>
