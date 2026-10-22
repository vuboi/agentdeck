<script lang="ts">
  import { ZIcon, ZLoading, zTooltip } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { activity, clearActivity, type Activity } from '../activity.svelte';
  import { elapsed } from '../blocks';
  import { ticker } from '../clock.svelte';
  import { showSession } from '../content-tabs.svelte';

  // Tasks: what the agents are running now, background shells and sub-agents included.
  // Audit (`log`): every tool call of every chat, newest first.
  let { log = false }: { log?: boolean } = $props();

  let open = $state<Record<string, boolean>>({});
  const live = (item: Activity) => item.status === 'running' || item.status === 'background';
  const items = $derived(log ? [...activity].reverse() : activity.filter(live).reverse());
  const recent = $derived(log ? [] : activity.filter((item) => !live(item) && item.background).slice(-20).reverse());

  const STATUS: Record<Activity['status'], { label: string; tone: string }> = {
    running: { label: 'Running', tone: 'text-primary' },
    background: { label: 'In background', tone: 'text-primary' },
    ok: { label: 'Done', tone: 'text-success' },
    fail: { label: 'Failed', tone: 'text-destructive' },
    stopped: { label: 'Stopped', tone: 'text-muted-foreground' },
  };
  const took = (item: Activity) => elapsed((item.endedAt ?? ticker.sec) - item.startedAt);
  const clock = (at: number) => new Date(at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
</script>

{#snippet row(item: Activity)}
  <li class="border-b border-border last:border-b-0">
    <button
      type="button"
      class="flex w-full cursor-pointer items-center gap-2 px-3 py-1.5 text-left text-xs outline-none hover:bg-muted/40 focus-visible:bg-muted/40"
      aria-expanded={open[item.key] ?? false}
      onclick={() => (open[item.key] = !open[item.key])}
    >
      <span class="grid size-3.5 shrink-0 place-items-center {STATUS[item.status].tone}" aria-label={STATUS[item.status].label} {@attach zTooltip({ content: STATUS[item.status].label, position: 'top' })}>
        {#if live(item)}
          <ZLoading zLoading zSpinner="orbit" zSize="2xs" zColor="current" />
        {:else}
          <ZIcon zType={(item.status === 'ok' ? 'lucideCircleCheck' : item.status === 'fail' ? 'lucideCircleX' : 'lucideBan') as ZIconName} class="size-3.5" />
        {/if}
      </span>
      <span class="shrink-0 font-medium {item.status === 'fail' ? 'text-destructive' : 'text-foreground'}">{item.name}</span>
      {#if item.background}<span class="shrink-0 rounded bg-primary/10 px-1 text-[10px] text-primary">bg</span>{/if}
      <span class="min-w-0 flex-1 truncate font-mono text-[11px] text-muted-foreground">{item.input}</span>
      <span class="shrink-0 tabular-nums text-[11px] text-muted-foreground">{log ? clock(item.startedAt) : took(item)}</span>
    </button>
    {#if open[item.key]}
      <div class="flex flex-col gap-1.5 px-3 pb-2.5 text-[11px]">
        <div class="flex items-center gap-2 text-muted-foreground">
          <button type="button" class="min-w-0 cursor-pointer truncate text-primary hover:underline" onclick={() => showSession(item.sessionId)}>{item.title}</button>
          <span aria-hidden="true">·</span>
          <span class="tabular-nums">{STATUS[item.status].label}, {took(item)}</span>
        </div>
        <pre class="m-0 overflow-x-auto rounded-md border border-border bg-background px-2 py-1.5 font-mono whitespace-pre-wrap break-all">{item.input}</pre>
        {#if item.output}
          <pre class="m-0 max-h-48 overflow-auto px-1 font-mono whitespace-pre-wrap break-all {item.status === 'fail' ? 'text-destructive' : 'text-muted-foreground'}">{item.output}</pre>
        {/if}
      </div>
    {/if}
  </li>
{/snippet}

<div class="flex h-full min-h-0 flex-1 flex-col" role="tabpanel" aria-label={log ? 'Audit log' : 'Tasks'}>
  <div class="flex h-8 shrink-0 items-center gap-2 border-b border-border px-3 text-xs">
    <span class="font-medium">{log ? 'Audit log' : 'Running now'}</span>
    <span class="text-muted-foreground tabular-nums">{items.length}</span>
    {#if log && activity.length > 0}
      <button type="button" class="ml-auto grid size-6 cursor-pointer place-items-center rounded text-muted-foreground hover:bg-primary/10 hover:text-primary" aria-label="Clear the log" {@attach zTooltip({ content: 'Clear', position: 'top' })} onclick={clearActivity}>
        <ZIcon zType={'lucideBan' as ZIconName} class="size-3.5" />
      </button>
    {/if}
  </div>
  <div class="min-h-0 flex-1 overflow-y-auto">
    {#if items.length === 0}
      <div class="p-4 text-center text-xs text-muted-foreground">
        {log ? 'Nothing yet. Every command, file edit and search an agent makes is listed here.' : 'Nothing running. Commands, background shells and sub-agents show here while they work.'}
      </div>
    {:else}
      <ul class="m-0 list-none p-0">
        {#each items as item (item.key)}{@render row(item)}{/each}
      </ul>
    {/if}
    {#if recent.length > 0}
      <div class="border-t border-border px-3 pt-2.5 pb-1 text-[10px] font-medium tracking-wider text-muted-foreground uppercase">Background, finished</div>
      <ul class="m-0 list-none p-0">
        {#each recent as item (item.key)}{@render row(item)}{/each}
      </ul>
    {/if}
  </div>
</div>
