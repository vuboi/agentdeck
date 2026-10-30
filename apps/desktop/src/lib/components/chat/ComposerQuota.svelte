<script lang="ts">
  import { inTauri } from '../../app/platform/history';
  import { customProviderOf } from '../../agents/provider-registry';
  import { onUsage, shortCount, totalTokens, usageLog, usd } from '../../chat/runs/usage';
  import { CREDIT_PREFIX } from '../../agents/credit-model';
  import type { Agent } from '../../chat/sessions.svelte';
  import {
    activeProviderKey,
    connectors,
    refreshAccounts,
    refreshAgyQuota,
    refreshProviderQuota,
    providerQuotas,
    claudeCreditState,
    refreshClaudeCredit,
    type UsageWindow,
  } from '../../agents/connectors.svelte';

  // The quota line under the composer, for the agent and model picked in it.
  let { agent, model }: { agent: Agent; model: string } = $props();

  // Keep the quota shown below the composer current for the provider selected here. These are the same
  // provider readings shown in Settings; providers that do not report a limit stay explicitly unavailable.
  $effect(() => {
    const selected = agent;
    if (!inTauri()) return;
    const refresh = () => {
      if (selected === 'antigravity') void refreshAgyQuota();
      else if (selected === 'claude' || selected === 'codex')
        void refreshAccounts(selected);
    };
    refresh();
    const timer = window.setInterval(refresh, 60_000);
    return () => clearInterval(timer);
  });

  // A custom provider (OpenRouter…) of the chosen model: its credit left when it tells it, else what was used today.
  const customProvider = $derived(agent === 'custom' ? customProviderOf(model) : undefined);
  $effect(() => {
    const provider = customProvider;
    if (!provider || !inTauri()) return;
    activeProviderKey(provider);
    void refreshProviderQuota(provider);
    const timer = window.setInterval(() => void refreshProviderQuota(provider), 120_000);
    return () => clearInterval(timer);
  });
  /** What this provider's key can still spend (OpenRouter), or its free-tier note. */
  const keyQuota = $derived(customProvider ? providerQuotas[`${customProvider.id}:${activeProviderKey(customProvider)?.id ?? 'default'}`] : undefined);

  // A Claude model billed to the Console API credit: the credit left, read from the Console now and then and after
  // each reply (a reply spends some of it).
  const onCredit = $derived(agent === 'claude' && model.startsWith(CREDIT_PREFIX));
  $effect(() => {
    if (!onCredit || !inTauri()) return;
    void refreshClaudeCredit();
    const timer = window.setInterval(() => void refreshClaudeCredit(), 120_000);
    const stop = onUsage(() => window.setTimeout(() => void refreshClaudeCredit(), 3000));
    return () => {
      clearInterval(timer);
      stop();
    };
  });
  const creditReading = $derived(onCredit ? claudeCreditState.reading : null);

  // What was used today with this agent (or custom provider), counted from the replies the app finished.
  let todayUsed = $state<{ tokens: number; replies: number; cost: number } | null>(null);
  async function readToday() {
    const log = await usageLog();
    const from = new Date();
    from.setHours(0, 0, 0, 0);
    const mine = log.filter((item) => new Date(item.at) >= from && item.agent === agent && (agent !== 'custom' || customProviderOf(item.model)?.id === customProvider?.id) && (agent !== 'claude' || (item.model ?? '').startsWith(CREDIT_PREFIX) === onCredit));
    todayUsed = { tokens: mine.reduce((sum, item) => sum + totalTokens(item.tokens), 0), replies: mine.length, cost: mine.reduce((sum, item) => sum + (item.cost ?? 0), 0) };
  }
  $effect(() => {
    void agent;
    void customProvider?.id;
    void onCredit;
    void readToday();
    return onUsage(() => void readToday());
  });

  const quota = $derived.by(() => {
    let windows: UsageWindow[] = [];
    let observedAt: number | null = null;
    if (onCredit) {
      // The credit left of what it was at its fullest (the grant, when the Console lists it).
      const top = Math.max(claudeCreditState.top, creditReading?.granted ?? 0, creditReading?.balance ?? 0);
      if (creditReading && top > 0) {
        windows = [{ label: `${usd(creditReading.balance)} credit`, pct: Math.min(100, 100 - (creditReading.balance / top) * 100), resets_at: null }];
        observedAt = creditReading.at;
      }
    } else if (keyQuota && keyQuota.limit && keyQuota.remaining !== null) {
      // A key with a credit limit reads like a quota window: the share still left.
      windows = [{ label: 'credit', pct: Math.min(100, ((keyQuota.limit - keyQuota.remaining) / keyQuota.limit) * 100), resets_at: null }];
      observedAt = keyQuota.at;
    } else if (agent === 'copilot') {
      const account = connectors.accounts.copilot?.accounts.find((item) => item.active);
      windows = account?.usage ?? [];
      observedAt = account?.observed_at ?? null;
    } else if (agent === 'claude' || agent === 'codex') {
      const account = connectors.accounts[agent]?.accounts.find(
        (item) => item.active,
      );
      windows = account?.usage ?? [];
      observedAt = account?.observed_at ?? null;
    } else if (agent === 'antigravity') {
      const family = model.startsWith('gemini-')
        ? 'gemini'
        : /^(claude-|gpt-oss-)/.test(model)
          ? '3p'
          : null;
      windows =
        connectors.agyQuota?.groups.find((group) => group.id === family)
          ?.windows ?? [];
      observedAt = connectors.agyQuota?.observed_at ?? null;
    }
    // The most restrictive window: the one with the least left. A window that already reset is full again. A 5h window
    // that is fresh does not help when the 7d one is used up: the next message would be refused all the same.
    const now = Date.now() / 1000;
    const left = (item: UsageWindow) =>
      item.resets_at !== null && item.resets_at <= now
        ? 100
        : Math.max(0, Math.min(100, 100 - item.pct));
    const minutesFor = (label: string) => {
      const match = /^(\d+)(m|h|d)$/.exec(label);
      if (!match) return Number.POSITIVE_INFINITY;
      const amount = Number(match[1]);
      return amount * (match[2] === 'd' ? 1440 : match[2] === 'h' ? 60 : 1);
    };
    const selected = [...windows].sort(
      (a, b) => left(a) - left(b) || minutesFor(a.label) - minutesFor(b.label),
    )[0];
    if (!selected) return null;
    return {
      label: selected.label,
      remaining: left(selected),
      resetsAt: selected.resets_at,
      observedAt,
    };
  });
  const quotaUnavailable = $derived(!quota);
  let previousQuotaRemaining: number | null = null;
  let quotaTipChanged = $state(false);
  let quotaTipTimer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    const remaining = quota?.remaining;
    if (remaining === undefined) {
      previousQuotaRemaining = null;
      quotaTipChanged = false;
      return;
    }
    if (
      previousQuotaRemaining !== null &&
      previousQuotaRemaining !== remaining
    ) {
      quotaTipChanged = true;
      clearTimeout(quotaTipTimer);
      quotaTipTimer = setTimeout(() => (quotaTipChanged = false), 900);
    }
    previousQuotaRemaining = remaining;
  });

  function quotaResetText(timestamp: number | null) {
    if (timestamp === null) return '';
    const days = Math.floor((timestamp * 1000 - Date.now()) / 86_400_000);
    if (days >= 1) {
      const hours = Math.round(
        ((timestamp * 1000 - Date.now()) % 86_400_000) / 3_600_000,
      );
      return `resets in ${days}d${hours ? ` ${hours}h` : ''}`;
    }
    const minutes = Math.max(
      0,
      Math.ceil((timestamp * 1000 - Date.now()) / 60_000),
    );
    if (minutes === 0) return 'resets now';
    if (minutes < 60) return `resets in ${minutes}m`;
    const hours = Math.floor(minutes / 60);
    const rest = minutes % 60;
    return `resets in ${hours}h${rest ? ` ${rest}m` : ''}`;
  }

  function quotaAgeText(timestamp: number | null) {
    if (timestamp === null) return '';
    const minutes = Math.max(
      0,
      Math.floor((Date.now() / 1000 - timestamp) / 60),
    );
    return minutes < 1 ? 'updated just now' : `updated ${minutes}m ago`;
  }
</script>

{#if quota}
  <div
    class="mt-1.5 flex min-w-0 flex-col gap-1 px-0.5"
    title="Remaining quota for the selected provider's most restrictive window"
  >
    <div class="flex min-w-0 items-center gap-2 text-[10px] leading-[1.25]">
      <span class="min-w-0 flex-1 truncate text-muted-foreground"
        >{quota.label} remaining{quotaResetText(quota.resetsAt)
          ? ` · ${quotaResetText(quota.resetsAt)}`
          : ''}</span
      >
      {#if quotaAgeText(quota.observedAt)}
        <span class="shrink-0 text-[9px] text-muted-foreground"
          >{quotaAgeText(quota.observedAt)}</span
        >
      {/if}
      <span
        class="shrink-0 tabular-nums {quota.remaining <= 10
          ? 'text-destructive'
          : 'text-muted-foreground'}">{Math.round(quota.remaining)}%</span
      >
    </div>
    <div
      class="relative h-1.5 rounded-full bg-muted"
      role="progressbar"
      aria-valuenow={Math.round(quota.remaining)}
      aria-valuemin="0"
      aria-valuemax="100"
      aria-label="{model} {quota.label} quota remaining"
    >
      <div
        class="relative h-full rounded-full transition-[width] duration-500 {quota.remaining <=
        10
          ? 'bg-destructive'
          : quota.remaining <= 30
            ? 'bg-amber-500'
            : 'bg-primary'}"
        class:quota-low={quota.remaining > 0 && quota.remaining <= 20}
        style:width="{quota.remaining}%"
      >
        {#if quota.remaining > 0}
          <span
            class="pointer-events-none absolute inset-0 overflow-hidden rounded-full"
            aria-hidden="true">
            <span class="quota-droplet" style="--level: 0.5px; --duration: 4.8s; --delay: -1.2s"></span>
            <span class="quota-droplet" style="--level: 2px; --duration: 6.2s; --delay: -3.8s"></span>
            <span class="quota-droplet" style="--level: 1px; --duration: 5.4s; --delay: -4.6s"></span>
          </span>
          <span
            class="absolute -right-1 top-1/2 grid size-2 -translate-y-1/2 place-items-center rounded-full {quota.remaining <=
            10
              ? 'text-destructive'
              : quota.remaining <= 30
                ? 'text-amber-500'
                : 'text-primary'}"
          >
            <!-- The tip glows on and off, like a pulse, so the end of the bar reads as live; a change pings it once more. -->
            <span
              class="quota-tip-halo absolute inset-0 rounded-full bg-current"
            ></span>
            {#if quotaTipChanged}<span
                class="absolute inset-0 rounded-full bg-current opacity-70 motion-safe:animate-ping"
              ></span>{/if}
            <span
              class="relative size-1.5 rounded-full border border-background bg-current shadow-[0_0_5px_currentColor]"
            ></span>
          </span>
        {/if}
      </div>
    </div>
  </div>
{:else if quotaUnavailable}
  <div
    class="mt-1.5 flex min-w-0 flex-col gap-1 px-0.5"
    title="This provider does not currently expose quota data to AgentDeck"
  >
    <span class="min-w-0 truncate text-[10px] leading-[1.25] text-muted-foreground"
      title={onCredit && !connectors.claudeCredit.sessionKey ? 'Add the Console sign-in in Settings > Providers to see the credit left' : undefined}
      >{#if onCredit}{claudeCreditState.error ? `Credit: ${claudeCreditState.error}` : connectors.claudeCredit.sessionKey ? 'Reading credit…' : 'Credit left unknown'}{:else if keyQuota?.free_tier}Free key: only “:free” models work, with daily request limits{:else if keyQuota}Spent {usd(keyQuota.used)} · this key has no limit{:else}This provider reports no limit{/if}{#if todayUsed && todayUsed.replies > 0}
        · today {shortCount(todayUsed.tokens)} tokens{todayUsed.cost > 0 ? ` · ${usd(todayUsed.cost)}` : ''} · {todayUsed.replies} {todayUsed.replies === 1 ? 'reply' : 'replies'}{/if}</span
    >
    <div
      class="h-1.5 rounded-full bg-muted"
      role="progressbar"
      aria-valuemin="0"
      aria-valuemax="100"
      aria-label="{model} quota unavailable"
      aria-valuetext="Quota unavailable"
    >
      <div class="h-full w-full rounded-full bg-muted-foreground/35"></div>
    </div>
  </div>
{/if}

<style>
  /* A few tiny drifting droplets give the compact quota track its motion without a sweeping shine. */
  .quota-droplet {
    position: absolute;
    left: -6%;
    bottom: var(--level);
    width: 2.5px;
    height: 2.5px;
    border: 0.5px solid rgb(255 255 255 / 0.75);
    border-radius: 50%;
    background: rgb(255 255 255 / 0.35);
    box-shadow: 0 0 3px rgb(255 255 255 / 0.55);
    animation: quota-droplet var(--duration) linear var(--delay) infinite;
  }
  /* At 20% or less the fill breathes once every couple of seconds; dim, not a strobe. */
  .quota-low {
    animation: quota-low 2.4s ease-in-out infinite;
  }
  .quota-tip-halo {
    animation: quota-tip-halo 2s ease-out infinite;
  }
  @keyframes quota-tip-halo {
    0% {
      transform: scale(0.8);
      opacity: 0.55;
    }
    70%,
    100% {
      transform: scale(2.6);
      opacity: 0;
    }
  }
  @keyframes quota-droplet {
    from { left: -6%; }
    to { left: 106%; }
  }
  @keyframes quota-low {
    0%,
    70%,
    100% {
      opacity: 1;
    }
    82% {
      opacity: 0.45;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .quota-droplet { display: none; }
    .quota-low {
      animation: none;
    }
    .quota-tip-halo {
      animation: none;
      opacity: 0;
    }
  }
</style>
