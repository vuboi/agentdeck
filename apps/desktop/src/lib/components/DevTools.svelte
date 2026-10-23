<script lang="ts">
  import { fly } from 'svelte/transition';
  import { ZDropdownMenu, ZIcon, ZPopover, zTooltip } from '@shival99/z-ui-svelte';
  import type { ZDropdownMenuItem, ZIconName } from '@shival99/z-ui-svelte';
  import { browser } from '../browser.svelte';
  import { setDevtools } from '../layout.svelte';
  import { toast } from '../toast';
  import { clearNetwork, TYPES } from '../devtools-net.svelte';
  import ApplicationPanel from './ApplicationPanel.svelte';
  import DevEmpty from './DevEmpty.svelte';
  import NetworkPanel from './NetworkPanel.svelte';
  import Splitter from './Splitter.svelte';

  // Console and network of the page, laid out like a browser's devtools: tabs, a filter, a bar to resize it.
  let { onresize, embedded = false }: { onresize?: (height: number) => void; embedded?: boolean } = $props();

  type Tab = 'console' | 'network' | 'application';
  let tab = $state<Tab>('console');
  let filter = $state('');
  let level = $state<'all' | 'error' | 'warn' | 'info'>('all');
  let netType = $state('fetch');
  let typeOpen = $state(false);
  let typeButton: HTMLElement | null = $state(null);
  let levelOpen = $state(false);
  let levelButton: HTMLElement | null = $state(null);

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
    else if (tab === 'network') {
      clearNetwork(browser.activeId);
      browser.network = [];
    }
  }

  // ---------- copy: the log or the requests as text, to paste into a chat (fix a bug, write a DTO) ----------
  const bad = (status: number) => status === 0 || status >= 400;
  async function copyText(text: string, what: string) {
    if (!text) return void toast.info(`No ${what} to copy`);
    try {
      await navigator.clipboard.writeText(text);
      toast.success(`Copied ${what}`);
    } catch {
      toast.error('Could not copy');
    }
  }
  function copyConsole(errorsOnly: boolean) {
    const picked = errorsOnly ? browser.console.filter((line) => line.level === 'error') : lines;
    void copyText(picked.map((line) => `[${clock(line.at)}] ${line.level.toUpperCase()} ${line.text}`).join('\n'), errorsOnly ? 'errors' : 'console');
  }
  function copyRequests(which: 'failed' | 'ok' | 'all') {
    const picked = browser.network.filter((line) => which === 'all' || (which === 'failed') === bad(line.status));
    const text = picked
      .map((line) => {
        const head = `${line.method} ${line.url} -> ${line.status} (${line.ms} ms)`;
        return [head, line.request ? `request:\n${line.request}` : '', line.body ? `response:\n${line.body}` : ''].filter(Boolean).join('\n');
      })
      .join('\n\n');
    void copyText(text, which === 'failed' ? 'failed requests' : which === 'ok' ? 'successful requests' : 'requests');
  }
  const copyItems = $derived<ZDropdownMenuItem[]>(
    tab === 'console'
      ? [
          { label: 'Copy console (as shown)', icon: 'lucideCopy', iconSize: '12', class: 'text-xs! py-1', onClick: () => copyConsole(false) },
          { label: 'Copy errors only', icon: 'lucideCopy', iconSize: '12', class: 'text-xs! py-1', onClick: () => copyConsole(true) },
        ]
      : [
          { label: 'Copy failed requests and responses', icon: 'lucideCopy', iconSize: '12', class: 'text-xs! py-1', onClick: () => copyRequests('failed') },
          { label: 'Copy successful requests and responses', icon: 'lucideCopy', iconSize: '12', class: 'text-xs! py-1', onClick: () => copyRequests('ok') },
          { label: 'Copy all requests', icon: 'lucideCopy', iconSize: '12', class: 'text-xs! py-1', divide: 'before', onClick: () => copyRequests('all') },
        ],
  );

  // The same divider as every other pane: a hairline with the primary glow on hover and while dragging.
  const resize = (delta: number) => {
    browser.devtoolsHeight = Math.min(Math.max(browser.devtoolsHeight - delta, 96), Math.round(window.innerHeight * 0.7));
    onresize?.(browser.devtoolsHeight);
  };
</script>

<!-- Embedded: it fills a window of its own (floating or docked), so the page is never resized for it. -->
<div class="flex min-h-0 flex-col overflow-hidden {embedded ? 'h-full flex-1' : 'shrink-0'}" style:height={embedded ? undefined : `${browser.devtoolsHeight + 1}px`} in:fly={{ y: embedded ? 0 : -8, duration: embedded ? 0 : 200, opacity: 0 }}>
  {#if !embedded}<Splitter axis="y" label="Resize developer tools" onDelta={resize} />{/if}
  <section class="flex flex-col bg-background {embedded ? 'min-h-0 flex-1' : 'shrink-0'}" style:height={embedded ? undefined : `${browser.devtoolsHeight}px`} aria-label="Developer tools">
  <div class="flex shrink-0 items-center gap-1 border-b border-border px-1.5 py-1" role="tablist">
    {#each [['console', 'Console'], ['network', 'Network'], ['application', 'Application']] as [value, label] (value)}
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
      <button
        bind:this={levelButton}
        type="button"
        class="flex h-6 shrink-0 cursor-pointer items-center gap-1 rounded border border-input bg-background px-1.5 text-xs outline-none hover:border-primary/60 {levelOpen ? 'border-primary' : ''}"
        aria-haspopup="listbox"
        aria-expanded={levelOpen}
        aria-label="Level"
        onclick={() => (levelOpen = !levelOpen)}
      >
        {LEVELS.find((item) => item.value === level)?.label}
        <ZIcon zType="lucideChevronDown" class="size-3 text-muted-foreground" />
      </button>
      <ZPopover anchor={levelButton} bind:open={levelOpen} zTrigger="manual" zPosition="top-right" zOffset={4} zWidth={110}>
        {#snippet children()}
          <div class="flex w-full flex-col gap-0.5 p-1" role="listbox" aria-label="Level">
            {#each LEVELS as item (item.value)}
              <button
                type="button"
                role="option"
                aria-selected={level === item.value}
                class="flex w-full cursor-pointer items-center justify-between rounded px-2 py-1 text-left text-xs outline-none hover:bg-primary/10 focus-visible:bg-primary/10 {level === item.value ? 'bg-primary/10 text-primary' : ''}"
                onclick={() => {
                  level = item.value;
                  levelOpen = false;
                }}
              >
                {item.label}
                {#if level === item.value}<ZIcon zType="lucideCheck" class="size-3" />{/if}
              </button>
            {/each}
          </div>
        {/snippet}
      </ZPopover>
    {:else if tab === 'network'}
      <button
        bind:this={typeButton}
        type="button"
        class="flex h-6 shrink-0 cursor-pointer items-center gap-1 rounded border border-input bg-background px-1.5 text-xs outline-none hover:border-primary/60 {typeOpen ? 'border-primary' : ''}"
        aria-haspopup="listbox"
        aria-expanded={typeOpen}
        aria-label="Type"
        onclick={() => (typeOpen = !typeOpen)}
      >
        {TYPES.find((item) => item.value === netType)?.label}
        <ZIcon zType="lucideChevronDown" class="size-3 text-muted-foreground" />
      </button>
      <ZPopover anchor={typeButton} bind:open={typeOpen} zTrigger="manual" zPosition="top-right" zOffset={4} zWidth={130}>
        {#snippet children()}
          <div class="flex w-full flex-col gap-0.5 p-1" role="listbox" aria-label="Type">
            {#each TYPES as item (item.value)}
              <button
                type="button"
                role="option"
                aria-selected={netType === item.value}
                class="flex w-full cursor-pointer items-center justify-between rounded px-2 py-1 text-left text-xs outline-none hover:bg-primary/10 focus-visible:bg-primary/10 {netType === item.value ? 'bg-primary/10 text-primary' : ''}"
                onclick={() => {
                  netType = item.value;
                  typeOpen = false;
                }}
              >
                {item.label}
                {#if netType === item.value}<ZIcon zType="lucideCheck" class="size-3" />{/if}
              </button>
            {/each}
          </div>
        {/snippet}
      </ZPopover>
    {/if}
    {#if tab !== 'application'}
    <div class="flex items-center [&_.z-icon]:!size-3.5 [&_button]:size-5! [&_button]:rounded [&_button]:p-0 [&_button]:!bg-transparent [&_button]:!text-muted-foreground [&_button:hover]:!bg-primary/10 [&_button:hover]:!text-primary [&_button[aria-expanded=true]]:!bg-primary/15 [&_button[aria-expanded=true]]:!text-primary [&_button]:focus-visible:ring-0">
      <ZDropdownMenu zItems={copyItems} zIcon={'lucideCopy' as ZIconName} zLabel="" zButtonType="ghost" zButtonSize="xs" zPosition="top-right" zOffset={6} zMinWidth={240} zWave={false} />
    </div>
    <button type="button" class="grid size-5 cursor-pointer place-items-center rounded text-muted-foreground hover:bg-primary/10 hover:text-primary" aria-label="Clear" {@attach zTooltip({ content: 'Clear', position: 'top' })} onclick={clear}>
      <ZIcon zType={'lucideTrash2' as ZIconName} zSize="14" />
    </button>
    {/if}
    <button type="button" class="grid size-5 cursor-pointer place-items-center rounded text-muted-foreground hover:bg-primary/10 hover:text-primary" aria-label="Close" {@attach zTooltip({ content: 'Close', position: 'top' })} onclick={() => setDevtools(false)}>
      <ZIcon zType="lucideX" zSize="14" />
    </button>
  </div>

  {#if tab === 'console'}
    <div class="min-h-0 flex-1 overflow-auto font-mono text-[11px]" role="log" aria-label="console">
      {#each lines as line, index (index)}
        <div class="group relative flex gap-2 border-b border-border/50 py-0.5 pr-7 pl-2 hover:bg-muted/40 {tone(line.level)}">
          <span class="shrink-0 tabular-nums opacity-70">{clock(line.at)}</span>
          <span class="min-w-0 break-all whitespace-pre-wrap {line.level === 'error' || line.level === 'warn' ? '' : 'text-foreground'}">{line.text}</span>
          <button type="button" class="absolute top-0.5 right-1.5 grid size-4 cursor-pointer place-items-center rounded bg-background/80 text-muted-foreground opacity-0 outline-none group-hover:opacity-100 hover:text-primary focus-visible:opacity-100" aria-label="Copy this line" title="Copy" onclick={() => void copyText(line.text, 'the line')}>
            <ZIcon zType={'lucideCopy' as ZIconName} zSize="11" />
          </button>
        </div>
      {:else}
        {#if filter || level !== 'all'}
          <DevEmpty icon="lucideSearchX" title="Nothing matches" hint="Try another filter or level." />
        {:else}
          <DevEmpty icon="lucideTerminal" title="Nothing logged yet" hint="console.log output and errors of the page appear here." />
        {/if}
      {/each}
    </div>
  {:else if tab === 'network'}
    <NetworkPanel {filter} type={netType} />
  {:else}
    <ApplicationPanel {filter} />
  {/if}
</section>
</div>
