<script lang="ts">
  import { untrack } from 'svelte';
  import type { ZDropdownMenuItem } from '@shival99/z-ui-svelte';
  import { type DiffRow } from '../../files/diff';
  import { highlightTokens, type Token } from '../../files/highlight';
  import { activeSession, drafts } from '../../chat/sessions.svelte';
  import { theme } from '../../app/ui/theme.svelte';
  import { toast } from '../../app/platform/toast';
  import Menu from '../ui/Menu.svelte';

  // Right-click: copy the selected lines or the whole change, or put them in the message box as a code block.
  let menu = $state<{ x: number; y: number } | null>(null);
  let menuOpen = $state(false);
  let picked = '';
  const signOf = { add: '+', del: '-', ctx: ' ', hunk: '' } as const;
  const asText = (list: DiffRow[]) => list.map((row) => (row.kind === 'hunk' ? row.text : `${signOf[row.kind]}${row.text}`)).join('\n');
  const copy = (text: string) => void navigator.clipboard.writeText(text).then(() => toast.success('Copied'));
  function addToChat(text: string, title: string) {
    const session = activeSession();
    if (!session) return toast.error('Open a chat first');
    const block = `${title}\n\`\`\`diff\n${text}\n\`\`\``;
    drafts[session.id] = [drafts[session.id]?.trimEnd(), block].filter(Boolean).join('\n\n');
    toast.success('Added to chat');
  }
  const items = $derived<ZDropdownMenuItem[]>([
    { label: 'Copy selection', icon: 'lucideCopy', iconSize: 12, class: 'text-sm! py-1', onClick: () => copy(picked || asText(rows)) },
    { label: 'Copy all changes', icon: 'lucideCopy', iconSize: 12, class: 'text-sm! py-1', onClick: () => copy(asText(rows)) },
    { label: 'Add selection to chat', icon: 'lucideMessageSquarePlus', iconSize: 12, class: 'text-sm! py-1', divide: 'before', onClick: () => addToChat(picked || asText(rows), 'Selected changes:') },
    { label: 'Add all changes to chat', icon: 'lucideMessageSquarePlus', iconSize: 12, class: 'text-sm! py-1', onClick: () => addToChat(asText(rows), 'Changes:') },
  ]);

  // The change of one file, line by line: removed lines in red, added lines in green, a hunk heading between the
  // groups, and the line numbers of the old and the new file. Only the rows on screen are drawn, and the code is
  // coloured by its language (in chunks, for the rows in view: the plain text is there at once).
  let { rows, lang }: { rows: DiffRow[]; lang?: string } = $props();

  const ROW = 20;
  const CHUNK = 200;
  let scroller = $state<HTMLElement | null>(null);
  let scrollTop = $state(0);
  let viewHeight = $state(400);
  const first = $derived(Math.max(0, Math.floor(scrollTop / ROW) - 10));
  const last = $derived(Math.min(rows.length, Math.ceil((scrollTop + viewHeight) / ROW) + 10));
  const slice = $derived(rows.slice(first, last));
  const sign = { add: '+', del: '−', ctx: ' ', hunk: '' } as const;
  const tone = {
    add: 'bg-emerald-500/15 text-foreground',
    del: 'bg-red-500/15 text-foreground',
    ctx: 'text-foreground/80',
    hunk: 'bg-primary/10 text-muted-foreground',
  } as const;
  const gutter = { add: 'text-emerald-500', del: 'text-red-500', ctx: 'text-muted-foreground/60', hunk: '' } as const;

  const shikiTheme = $derived(theme.dark ? 'github-dark' : 'github-light');
  // Plain map, not $state: one version counter tells the template when a chunk has landed.
  const tokens = new Map<number, Token[]>();
  let version = $state(0);
  let generation = 0;

  // New rows, language or theme: colours are worked out again.
  $effect(() => {
    void rows;
    void lang;
    void shikiTheme;
    // Untracked: `version += 1` also reads `version`, and this effect would run again each time a chunk landed.
    untrack(() => {
      tokens.clear();
      generation += 1;
      version += 1;
    });
  });

  async function fill(gen: number, from: number, to: number) {
    if (!lang) return;
    const start = Math.floor(from / CHUNK) * CHUNK;
    for (let at = start; at <= to; at += CHUNK) {
      if (gen !== generation) return;
      if (tokens.has(at)) continue;
      const end = Math.min(rows.length, at + CHUNK);
      // The lines of a chunk go in one piece, in the order of the diff, so a comment or a string that spans lines stays one.
      const text = rows.slice(at, end).map((row) => (row.kind === 'hunk' ? '' : row.text)).join('\n');
      try {
        const result = await highlightTokens(text, lang, theme.dark);
        if (!result) continue;
        if (gen !== generation) return;
        result.forEach((line, offset) => tokens.set(at + offset, line));
        version += 1;
      } catch {
        // A language shiki does not know: the rows stay plain.
        return;
      }
    }
  }

  $effect(() => {
    const from = first;
    const to = Math.max(first, last - 1);
    const gen = generation;
    void version;
    untrack(() => void fill(gen, from, to));
  });
</script>

<div bind:this={scroller} bind:clientHeight={viewHeight} onscroll={() => (scrollTop = scroller?.scrollTop ?? 0)} oncontextmenu={(event) => { event.preventDefault(); picked = window.getSelection()?.toString() ?? ''; menu = { x: event.clientX, y: event.clientY }; menuOpen = true; }} class="isolate h-full min-h-0 select-text overflow-auto bg-background font-mono text-sm leading-5" role="document" aria-label="Changes of the file">
  <div class="w-max min-w-full" style:height="{rows.length * ROW}px" style:padding-top="{first * ROW}px" style:box-sizing="border-box">
    {#each slice as row, index (first + index)}
      {@const line = version >= 0 && row.kind !== 'hunk' ? tokens.get(first + index) : undefined}
      <div class="flex h-5 w-full min-w-max whitespace-pre {tone[row.kind]}">
        {#if row.kind === 'hunk'}
          <span class="sticky left-0 z-20 w-max bg-background px-3">{row.text}</span>
        {:else}
          <span class="sticky left-0 z-20 flex h-5 w-[100px] flex-none bg-background {row.kind === 'add' ? 'bg-emerald-500/20!' : row.kind === 'del' ? 'bg-red-500/20!' : ''} select-none">
            <span class="w-10 shrink-0 pr-2 text-right text-muted-foreground/60">{row.old ?? ''}</span>
            <span class="w-10 shrink-0 pr-2 text-right text-muted-foreground/60">{row.now ?? ''}</span>
            <span class="w-5 shrink-0 text-center font-bold {gutter[row.kind]}">{sign[row.kind]}</span>
          </span>
          <span class="relative z-0 pr-6">
            {#if line && line.length > 0}
              {#each line as token, at (at)}<span style:color={token.color}>{token.content}</span>{/each}
            {:else}
              {row.text}
            {/if}
          </span>
        {/if}
      </div>
    {/each}
  </div>
</div>
{#if menu}
  <Menu {items} bind:open={menuOpen} at={menu} position="bottom-left" width={220} />
{/if}
