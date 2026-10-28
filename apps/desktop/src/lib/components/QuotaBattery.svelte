<script lang="ts">
  import { ZIcon } from '@shival99/z-ui-svelte';
  /** A compact remaining-quota gauge; null means the provider has not supplied a reading. */
  let { remaining }: { remaining: number | null } = $props();
  const percent = $derived(remaining === null || !Number.isFinite(remaining) ? null : Math.round(Math.max(0, Math.min(100, remaining))));
  const tone = $derived(percent === null ? 'text-muted-foreground' : percent > 50 ? 'text-emerald-500' : percent >= 20 ? 'text-orange-500' : 'text-red-500');
</script>

<span class="quota-battery relative grid h-9 w-8 shrink-0 place-items-center overflow-hidden rounded-[5px] border border-current/40 bg-background/70 {tone}" role="progressbar" aria-label="Quota remaining" aria-valuemin={0} aria-valuemax={100} aria-valuenow={percent ?? undefined} aria-valuetext={percent === null ? 'Quota unavailable' : `${percent}% remaining`} title={percent === null ? 'Quota unavailable' : `${percent}% remaining · lowest available limit`}>
  {#if percent !== null}
    <span class="quota-water pointer-events-none absolute inset-x-0 bottom-0 bg-current/80" style:height="{percent}%" aria-hidden="true">
      {#if percent > 0 && percent < 100}
        <svg class="quota-wave absolute -top-[3px] left-0 h-[6px] w-[200%] fill-current opacity-80" viewBox="0 0 120 6" preserveAspectRatio="none"><path d="M0 3 Q7.5 0 15 3 T30 3 T45 3 T60 3 T75 3 T90 3 T105 3 T120 3 V6 H0Z" /></svg>
        <svg class="quota-wave quota-wave-back absolute -top-[4px] left-0 h-[7px] w-[200%] fill-current opacity-35" viewBox="0 0 120 6" preserveAspectRatio="none"><path d="M0 3 Q7.5 0 15 3 T30 3 T45 3 T60 3 T75 3 T90 3 T105 3 T120 3 V6 H0Z" /></svg>
      {/if}
    </span>
  {/if}
  <span class="relative z-10 grid place-items-center font-mono text-[10px] font-semibold tabular-nums leading-none {percent === null ? '' : 'quota-value text-white'}" aria-hidden="true">
    {#if percent !== null && percent >= 95}<ZIcon zType="lucideZap" class="size-4" />{:else}{percent === null ? '—' : `${percent}%`}{/if}
  </span>
</span>

<style>
  .quota-water { transition: height 320ms ease-out; }
  .quota-wave { animation: quota-wave 2.8s linear infinite; }
  .quota-wave-back { animation-duration: 4s; animation-direction: reverse; }
  .quota-value { text-shadow: 0 1px 2px rgb(0 0 0 / 85%), 0 0 2px rgb(0 0 0 / 65%); }
  @keyframes quota-wave { to { transform: translateX(-50%); } }
  @media (prefers-reduced-motion: reduce) {
    .quota-wave { animation: none; }
    .quota-water { transition: none; }
  }
</style>
