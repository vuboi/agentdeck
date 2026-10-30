<script lang="ts">
  import { fly } from 'svelte/transition';
  import { ZButton, ZIcon, ZSwitch } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { ticker } from '../../app/platform/clock.svelte';
  import { resetCountdown } from '../../chat/recovery/countdown';
  import { resumeNow, setResumeAuto, type Resume } from '../../chat/sessions.svelte';
  import { openExternal } from '../../browser/browser.svelte';
  import { ENDPOINTS } from '../../agents/endpoints';
  import { modelLabel } from '../../agents/models.svelte';

  // Compact status directly above the composer when a turn is paused at a usage limit.
  let { resume }: { resume: Resume } = $props();

  const left = $derived(resume.at > ticker.sec ? resetCountdown(resume.at / 1000, ticker.sec) : 'now');
  const canAutoResume = $derived(resume.canAutoResume ?? true);
  const model = $derived(resume.model ? modelLabel(resume.model) : '');
  function changeAuto(value: boolean) {
    setResumeAuto(resume.sessionId, value);
  }
  function tryAgain() {
    resumeNow(resume.sessionId);
  }
</script>

<section class="mb-2 flex min-h-[5.5rem] w-fit min-w-[23rem] max-w-full flex-col justify-between gap-2 rounded-lg border border-border bg-card px-3.5 pb-2.5 pt-3.5 text-sm" aria-label="Usage limit reached" transition:fly={{ y: 4, duration: 140 }}>
  <div class="flex min-w-0 items-start gap-2.5">
    <ZIcon zType={'lucideGauge' as ZIconName} class="mt-0.5 size-4 shrink-0 text-destructive" />
    <div class="min-w-0">
      <div class="truncate text-sm font-medium">{resume.label}{model ? ` · ${model}` : ''}</div>
      <div class="truncate text-sm text-muted-foreground">{#if canAutoResume}{resume.detail ?? 'Try again after the provider limit resets'} · {left}{:else}{resume.detail ?? 'Try again after resolving this limit'}{/if}</div>
    </div>
  </div>
  <div class="flex min-h-7 shrink-0 items-center gap-3 pl-6">
    {#if canAutoResume}
      <ZSwitch class="[&>div>label]:text-[13px]" zVariant="dot" zSize="sm" zWave={false} zText="Continue when limit resets" zTextPosition="right" checked={resume.auto} onChange={changeAuto} />
    {/if}
    {#if !resume.auto || !canAutoResume}
      {#if resume.gate === 'credit' && resume.agent === 'claude'}
        <ZButton zWave={false} zType="outline" zSize="sm" class="h-7 px-2.5 text-xs" onclick={() => void openExternal(ENDPOINTS.anthropic.billingPage)}>Add credit</ZButton>
      {/if}
      <ZButton zWave={false} zType="outline" zSize="sm" class="h-7 gap-1.5 px-2.5 text-[13px]" onclick={tryAgain}>
        <ZIcon zType={'lucideRotateCcw' as ZIconName} class="size-3.5" />
        Try again
      </ZButton>
    {/if}
  </div>
</section>
