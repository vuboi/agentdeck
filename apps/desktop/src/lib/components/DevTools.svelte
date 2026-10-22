<script lang="ts">
  import { ZIcon, zTooltip } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { browser } from '../browser.svelte';

  // Console and network of the page, laid out like a browser's devtools: tabs, a filter, a bar to resize it.
  let { onresize }: { onresize?: (height: number) => void } = $props();

  type Tab = 'console' | 'network';
  let tab = $state<Tab>('console');
  let filter = $state('');
  let level = $state<'all' | 'error' | 'warn' | 'info'>('all');
  let onlyFailed = $state(false);

  const LEVELS: { value: typeof level; label: string }[] = [
    { value: 'all', label: 'All' },
    { value: 'error', label: 'Errors' },
    { value: 'warn', label: 'Warnings' },
    { value: 'info', label: 'Info' },
  ];

  const matches = (text: string) => !filter.trim() || text.toLowerCase().includes(filter.trim().toLowerCase());
  const lines = $derived(
    browser.console.filter((line) => {
      const group = line.level === 'error' ? 'error' : line.level === 'warn' ? 'warn' : 'info';
      return (level === 'all' || group === level) && matches(line.text);
    }),
  );
  const requests = $derived(
    browser.network.filter((line) => (!onlyFailed || line.status === 0 || line.status >= 400) && matches(`${line.method} ${line.url} ${line.status}`)),
  );
  const errors = $derived(browser.console.filter((line) => line.level === 'error').length);

  const tone = (value: string) => (value === 'error' ? 'text-destructive bg-destructive/5' : value === 'warn' ? 'text-warning bg-warning/5' : 'text-muted-foreground');
  const statusTone = (status: number) => (status === 0 || status >= 400 ? 'text-destructive' : status >= 300 ? 'text-warning' : 'text-success');
  const name = (url: string) => {
    try {
      const parsed = new URL(url, 'http://x');
      return parsed.pathname.split('/').filter(Boolean).at(-1) || parsed.pathname || url;
    } catch {
      return url;
    }
  };
  const clock = (at: number) => new Date(at).toLocaleTimeString([], { hour12: false });

  function clear() {
    if (tab === 'console') browser.console = [];
    else browser.network = [];
  }

  // Drag the top edge to change the height.
  function startResize(event: PointerEvent) {
    const bar = event.currentTarget as HTMLElement;
    const startY = event.clientY;
    const start = browser.devtoolsHeight;
    bar.setPointerCapture(event.pointerId);
    const move = (e: PointerEvent) => {
      browser.devtoolsHeight = Math.min(Math.max(start + (startY - e.clientY), 96), Math.round(window.innerHeight * 0.7));
      onresize?.(browser.devtoolsHeight);
    };
    const stop = () => {
      bar.removeEventListener('pointermove', move);
      bar.removeEventListener('pointerup', stop);
    };
    bar.addEventListener('pointermove', move);
    bar.addEventListener('pointerup', stop);
  }
</script>

<section class="flex shrink-0 flex-col border-t border-border bg-background" style:height="{browser.devtoolsHeight}px" aria-label="Developer tools">
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="-mt-0.5 h-1 shrink-0 cursor-row-resize hover:bg-primary/40" onpointerdown={startResize} aria-hidden="true"></div>

  <div class="flex shrink-0 items-center gap-1 border-b border-border px-1.5 py-1" role="tablist">
    {#each [['console', 'Console'], ['network', 'Network']] as [value, label] (value)}
      <button
        type="button"
        role="tab"
        aria-selected={tab === value}
        class="flex h-6 cursor-pointer items-center gap-1 rounded px-2 text-xs outline-none {tab === value ? 'bg-primary/15 text-primary' : 'text-muted-foreground hover:bg-primary/10 hover:text-primary'}"
        onclick={() => (tab = value as Tab)}
      >
        {label}
        {#if value === 'console' && errors > 0}<span class="rounded-full bg-destructive px-1 text-[9px] leading-3.5 text-white">{errors}</span>{/if}
      </button>
    {/each}
    <input
      bind:value={filter}
      class="mx-1 h-6 min-w-0 flex-1 rounded border border-input bg-background px-2 text-xs outline-none focus:border-primary"
      placeholder="Filter"
      aria-label="Filter"
    />
    {#if tab === 'console'}
      <select bind:value={level} class="h-6 cursor-pointer rounded border border-input bg-background px-1 text-xs outline-none" aria-label="Level">
        {#each LEVELS as item (item.value)}<option value={item.value}>{item.label}</option>{/each}
      </select>
    {:else}
      <label class="flex cursor-pointer items-center gap-1 text-xs text-muted-foreground"><input type="checkbox" bind:checked={onlyFailed} /> Failed</label>
    {/if}
    <button type="button" class="grid size-6 cursor-pointer place-items-center rounded text-muted-foreground hover:bg-primary/10 hover:text-primary" aria-label="Clear" {@attach zTooltip({ content: 'Clear', position: 'top' })} onclick={clear}>
      <ZIcon zType={'lucideBan' as ZIconName} class="size-3.5" />
    </button>
    <button type="button" class="grid size-6 cursor-pointer place-items-center rounded text-muted-foreground hover:bg-primary/10 hover:text-primary" aria-label="Close" {@attach zTooltip({ content: 'Close', position: 'top' })} onclick={() => (browser.devtools = false)}>
      <ZIcon zType="lucideX" class="size-3.5" />
    </button>
  </div>

  <div class="min-h-0 flex-1 overflow-auto font-mono text-[11px]" role="log" aria-label={tab}>
    {#if tab === 'console'}
      {#each lines as line, index (index)}
        <div class="flex gap-2 border-b border-border/50 px-2 py-0.5 {tone(line.level)}">
          <span class="shrink-0 tabular-nums opacity-70">{clock(line.at)}</span>
          <span class="min-w-0 break-all whitespace-pre-wrap {line.level === 'error' || line.level === 'warn' ? '' : 'text-foreground'}">{line.text}</span>
        </div>
      {:else}
        <div class="p-3 text-muted-foreground">Nothing logged{filter || level !== 'all' ? ' for this filter' : ' yet'}.</div>
      {/each}
    {:else}
      <div class="sticky top-0 flex gap-2 border-b border-border bg-muted/60 px-2 py-0.5 text-muted-foreground backdrop-blur">
        <span class="w-10 shrink-0">Method</span><span class="w-9 shrink-0">Status</span><span class="w-32 shrink-0">Name</span><span class="min-w-0 flex-1">URL</span><span class="w-12 shrink-0 text-right">Time</span>
      </div>
      {#each requests as line, index (index)}
        <div class="flex gap-2 border-b border-border/50 px-2 py-0.5 hover:bg-muted/40">
          <span class="w-10 shrink-0 text-muted-foreground">{line.method}</span>
          <span class="w-9 shrink-0 {statusTone(line.status)}">{line.status || 'ERR'}</span>
          <span class="w-32 shrink-0 truncate">{name(line.url)}</span>
          <span class="min-w-0 flex-1 truncate text-muted-foreground" title={line.url}>{line.url}</span>
          <span class="w-12 shrink-0 text-right text-muted-foreground">{line.ms}ms</span>
        </div>
      {:else}
        <div class="p-3 text-muted-foreground">No requests{filter || onlyFailed ? ' for this filter' : ' yet'}.</div>
      {/each}
    {/if}
  </div>
</section>
