<script lang="ts">
  import { ZIcon, ZNumberTicker, zTooltip } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { modelLabel } from '../../agents/models.svelte';
  import { agentDisplayName } from '../../agents/provider-registry';
  import { isRunning, workspace } from '../../chat/sessions.svelte';
  import { costParts, shortCount, totalTokens, usd } from '../../chat/runs/usage';
  import type { RunUsage } from '../../chat/runs/run-usage';
  import AgentLogo from '../ui/AgentLogo.svelte';
  import { agentName, agentText } from './chat-helpers';
  import type { Message } from '../../chat/mock';

  // The name line over the first reply of a run: agent, model, time, and the whole run's tokens and cost.
  let { message, agent, usage }: { message: Message; agent: string; usage: RunUsage | undefined } = $props();
</script>

<div class="mb-1.5 flex flex-wrap items-baseline gap-x-1.5 gap-y-0.5 text-sm">
  <span class="inline-flex items-center gap-1 font-semibold {agentText[agent as keyof typeof agentText]}">
    <AgentLogo agent={agent} model={message.model} class="size-3 shrink-0" />
    {agent === 'custom' ? agentDisplayName(agent, message.model) : agentName(agent)}
  </span>
  {#if message.model}
    <span class="text-muted-foreground/40" aria-hidden="true">/</span>
    <span class="mt-0.5 text-muted-foreground" title={message.model}>{modelLabel(message.model)}</span>
  {/if}
  <span class="text-muted-foreground/40" aria-hidden="true">·</span>
  <span class="text-muted-foreground/80 tabular-nums">{message.time}</span>
  {#if usage}
    {@const cost = usage.cost}
    {@const tokens = usage.tokens}
    {@const parts = costParts(tokens, message.model)}
    {#snippet usageTip()}
      <div class="flex min-w-44 flex-col gap-1 py-0.5 text-[0.6875rem] tabular-nums">
        {#each [
          { label: 'Input', count: tokens.input, cost: parts?.input, icon: 'lucideArrowDownToLine', tone: 'text-sky-500' },
          { label: 'Cache read', count: tokens.cacheRead, cost: parts?.cacheRead, icon: 'lucideDatabase', tone: 'text-violet-500' },
          { label: 'Cache write', count: tokens.cacheWrite, cost: parts?.cacheWrite, icon: 'lucideDatabaseZap', tone: 'text-amber-500' },
          { label: 'Output', count: tokens.output, cost: parts?.output, icon: 'lucideArrowUpFromLine', tone: 'text-emerald-500' },
        ] as row (row.label)}
          <div class="flex items-center gap-2">
            <ZIcon zType={row.icon as ZIconName} zSize="12" class="size-3! shrink-0 {row.tone}" />
            <span class="flex-1 text-muted-foreground">{row.label}</span>
            <span class="font-medium text-foreground">{shortCount(row.count)}</span>
            {#if row.cost != null}<span class="w-12 text-right text-muted-foreground">{usd(row.cost)}</span>{/if}
          </div>
        {/each}
        {#if cost != null}
          <div class="-mx-[0.6rem] mt-0.5 flex items-center gap-2 border-t border-border px-[0.6rem] pt-1.5">
            <ZIcon zType={'lucideCoins' as ZIconName} zSize="12" class="size-3! shrink-0 text-success" />
            <span class="flex-1 text-muted-foreground">Total</span>
            <span class="font-semibold text-success">{usage.exact ? '' : '≈ '}{usd(cost)}</span>
          </div>
        {/if}
      </div>
    {/snippet}
    <!-- The figures roll only while the reply is being written; a loaded chat shows them plainly. -->
    {@const rolling = Boolean(message.live) && isRunning(workspace.activeId)}
    <!-- Tokens and cost, quiet and on the right; hover for the breakdown. -->
    <span class="ml-auto flex items-baseline gap-1.5 text-[0.6875rem] tabular-nums" {@attach zTooltip({ content: usageTip, position: 'top' })}>
      <span class="text-muted-foreground">{#if rolling}<ZNumberTicker zValue={totalTokens(usage.tokens)} zFormat="compact" zDuration={600} />{:else}{shortCount(totalTokens(usage.tokens))}{/if} tokens</span>
      {#if cost != null}
        <span class="text-muted-foreground/40" aria-hidden="true">·</span>
        <span class="font-medium text-success">{usage.exact ? '' : '≈ '}{#if rolling}<ZNumberTicker zValue={cost} zFormat="currency" zCurrency="USD" zLocale="en-US" zMinimumFractionDigits={2} zMaximumFractionDigits={cost < 0.01 ? 3 : 2} zDuration={600} />{:else}{usd(cost)}{/if}</span>
      {/if}
    </span>
  {/if}
</div>
