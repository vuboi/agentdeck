<script lang="ts">
  import { fly } from 'svelte/transition';
  import { ZButton, ZIcon } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { ticker } from '../clock.svelte';
  import { resetCountdown } from '../countdown';
  import { cancelResume, resumeNow, setResumeAuto, type Resume } from '../sessions.svelte';
  import { openSettings } from '../connectors.svelte';

  // Above the message box when a turn stopped at a usage limit: when the work carries on by itself, with a switch to turn
  // that off, Try again now, and Cancel. (sessions.svelte.ts, "carrying on when a usage limit ends")
  let { resume }: { resume: Resume } = $props();

  const clock = $derived(new Date(resume.at).toLocaleString([], { hour: 'numeric', minute: '2-digit', ...(resume.at - Date.now() > 20 * 3_600_000 ? { weekday: 'short', day: 'numeric', month: 'short' } : {}) }));
  const left = $derived(resume.at > ticker.sec ? resetCountdown(resume.at / 1000, ticker.sec) : 'now');
</script>

<section class="mb-2 overflow-hidden rounded-xl border border-border bg-card" aria-label="Usage limit reached" transition:fly={{ y: 8, duration: 180 }}>
  <div class="flex items-center gap-2.5 border-b border-border bg-muted/40 px-4 py-2.5 text-sm font-medium">
    <ZIcon zType={'lucideGauge' as ZIconName} class="size-4 shrink-0 text-destructive" />
    Usage limit reached
  </div>
  <div class="px-4 py-3 text-xs">
    <p class="m-0 text-muted-foreground">
      {#if resume.auto}Auto-resuming at <b class="text-foreground">{clock}</b> · in <span class="tabular-nums">{left}</span>{:else}Resets at <b class="text-foreground">{clock}</b> · in <span class="tabular-nums">{left}</span>{/if}
    </p>
    <label class="mt-2.5 flex w-fit cursor-pointer items-center gap-2 text-sm">
      <input type="checkbox" class="size-4 cursor-pointer accent-[var(--primary)]" checked={resume.auto} onchange={(event) => setResumeAuto(resume.sessionId, event.currentTarget.checked)} />
      Auto-continue when the limit resets
    </label>
    <div class="mt-3 flex flex-wrap items-center justify-end gap-2">
      <ZButton zWave={false} zType="ghost" zSize="sm" class="h-8 px-3 text-xs" onclick={() => cancelResume(resume.sessionId)}>Cancel</ZButton>
      <ZButton zWave={false} zType="outline" zSize="sm" class="h-8 gap-1.5 px-3 text-xs" onclick={() => openSettings()}>
        <ZIcon zType={'lucideGauge' as ZIconName} class="size-3.5" /> Usage settings
      </ZButton>
      <ZButton zWave={false} zSize="sm" class="h-8 px-3 text-xs" onclick={() => resumeNow(resume.sessionId)}>Try again</ZButton>
    </div>
  </div>
</section>
