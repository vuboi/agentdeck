<script lang="ts">
  import { ZButton, ZIcon } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import AgentLogo from '../ui/AgentLogo.svelte';
  import { openExternal } from '../../browser/browser.svelte';
  import { claudeCreditOn, claudeCreditState, connectors, refreshClaudeCredit } from '../../agents/connectors.svelte';
  import { usd } from '../../chat/runs/usage';
  import { ENDPOINTS } from '../../agents/endpoints';
  import { isClaudeConsole, type ProviderForm } from './sections/provider-form.svelte';

  // Settings > Providers: the Claude Console's API credit. Each Claude Console key is a provider of its own (added and edited
  // in the same provider form as the others); the Console sign-in reads what is left of the credit.
  const KEYS_URL = ENDPOINTS.anthropic.keysPage;

  let { form }: { form: ProviderForm } = $props();

  // Added means a Claude Console key is listed under Custom providers, so removing that row brings back the +.
  const added = $derived(connectors.custom.some(isClaudeConsole));
  const credit = $derived(claudeCreditState.reading);
  const top = $derived(Math.max(claudeCreditState.top, credit?.granted ?? 0, credit?.balance ?? 0));
  const left = $derived(credit && top > 0 ? Math.max(0, Math.min(100, (credit.balance / top) * 100)) : null);
  const expires = $derived(credit?.expires_at ? new Date(credit.expires_at).toLocaleDateString([], { day: 'numeric', month: 'short', year: 'numeric' }) : null);

  $effect(() => {
    if (connectors.claudeCredit.sessionKey && !claudeCreditState.reading) void refreshClaudeCredit();
  });
</script>

<!-- One row of Settings > Providers > Cloud providers, as the free ones; it is edited in a modal over the settings. -->
<li class="relative flex min-w-0 items-center gap-2.5 overflow-hidden rounded-lg border border-border px-3 py-2 text-sm">
  <span class="grid size-5 shrink-0 place-items-center"><AgentLogo agent="claude" class="size-4" /></span>
  <span class="min-w-0 flex-1">
    <span class="flex min-w-0 items-center gap-1.5 text-sm font-medium"><span class="truncate">Claude Console</span><span class="shrink-0 rounded bg-amber-500/15 px-1.5 py-px text-[10px] font-medium text-amber-600 dark:text-amber-400" title="Paid with API credit: a grant or a top-up">Credit</span></span>
    <span class="block truncate text-[0.6875rem] text-muted-foreground" title={claudeCreditState.error ?? undefined}>
      {#if !connectors.claudeCredit.apiKey}Claude models, prepaid credit
      {:else if credit}{usd(credit.balance)} left{expires ? ` · expires ${expires}` : ''}
      {:else if claudeCreditState.error}<span class="text-destructive">{claudeCreditState.error}</span>
      {:else if connectors.claudeCredit.sessionKey}Reading the credit…
      {:else}Key saved · credit left unknown{/if}
    </span>
  </span>
  {#if connectors.claudeCredit.sessionKey}
    <ZButton zWave={false} zType="ghost" zSize="sm" class="size-7! shrink-0 p-0" aria-label="Read the credit again" title="Read the credit again" disabled={claudeCreditState.busy} onclick={() => void refreshClaudeCredit()}>
      <ZIcon zType={'lucideRefreshCw' as ZIconName} class="size-3.5 {claudeCreditState.busy ? 'animate-spin' : ''}" />
    </ZButton>
  {/if}
  <ZButton zWave={false} zType="ghost" zSize="sm" class="size-7! shrink-0 p-0" aria-label="Create a Claude API key" title="Create an API key ({KEYS_URL.replace('https://', '')})" onclick={() => void openExternal(KEYS_URL)}>
    <ZIcon zType={'lucideKeyRound' as ZIconName} class="size-3.5" />
  </ZButton>
  <!-- A check once a key is saved, as the other providers show; the button still adds another key. -->
  {#if added}
    <ZButton zWave={false} zType="ghost" zSize="sm" class="size-7! shrink-0 p-0 text-primary" aria-label="Add another Claude API key" title="Added · add another key" onclick={() => form.useClaude()}>
      <ZIcon zType={'lucideCheck' as ZIconName} class="size-3.5" />
    </ZButton>
  {:else}
    <ZButton zWave={false} zType="outline" zSize="sm" class="size-7! shrink-0 p-0" aria-label="Add a Claude API key" title="Add an API key" onclick={() => form.useClaude()}>
      <ZIcon zType={'lucidePlus' as ZIconName} class="size-3.5" />
    </ZButton>
  {/if}
  {#if left !== null && claudeCreditOn()}
    <!-- The credit left, along the row's bottom edge. -->
    <div class="absolute inset-x-0 bottom-0 h-0.5 bg-muted" role="progressbar" aria-label="Claude credit left" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(left)}>
      <div class="h-full transition-[width] duration-500 {left <= 10 ? 'bg-destructive' : left <= 30 ? 'bg-amber-500' : 'bg-primary'}" style:width="{left}%"></div>
    </div>
  {/if}
</li>
