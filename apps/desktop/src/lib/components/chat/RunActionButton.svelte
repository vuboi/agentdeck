<script lang="ts">
  import { ZButton, ZIcon, zTooltip } from '@shival99/z-ui-svelte';
  import { lucideArrowUp, lucidePlay } from '@ng-icons/lucide';

  /** Composer and workers keep all three icons mounted at a fixed center during state changes. */
  let {
    action, label, tooltip = label, spinning = action === 'stop', disabled = false, type = 'button', onclick,
  }: {
    action: 'send' | 'stop' | 'resume';
    label: string;
    tooltip?: string;
    spinning?: boolean;
    disabled?: boolean;
    type?: 'button' | 'submit';
    onclick?: () => void;
  } = $props();
</script>

<span class="run-action relative inline-grid size-7 shrink-0 place-items-center" {@attach zTooltip({ content: tooltip, position: 'top' })}>
  {#if spinning && !disabled}
    <span class="pointer-events-none absolute -inset-[3px] animate-spin rounded-full [background:conic-gradient(from_0deg,transparent_0_55%,var(--color-primary)_100%)] [mask:radial-gradient(farthest-side,transparent_calc(100%_-_2px),#000_calc(100%_-_2px))] motion-reduce:hidden" aria-hidden="true"></span>
    <span class="pointer-events-none absolute inset-0 animate-ping rounded-full bg-primary/25 [animation-duration:1.8s] motion-reduce:hidden" aria-hidden="true"></span>
  {/if}
  <ZButton zWave={false} zIconOnly={true} zType="default" zSize="default" {type} class="relative size-7! shrink-0 rounded-full p-0" {disabled} aria-label={label} {onclick}>
    <span class="action-icons relative block size-3.5 shrink-0" aria-hidden="true">
      <span class="action-icon grid place-items-center" class:visible={action === 'send'}><ZIcon zSvg={lucideArrowUp} zSize="14" /></span>
      <svg class="action-icon" class:visible={action === 'stop'} viewBox="0 0 16 16"><rect x="2.5" y="2.5" width="11" height="11" rx="2.5" fill="currentColor" /></svg>
      <span class="action-icon grid place-items-center" class:visible={action === 'resume'}><ZIcon zSvg={lucidePlay} zSize="14" class="[&>svg]:fill-current" /></span>
    </span>
  </ZButton>
</span>

<style>
  .action-icon {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    opacity: 0;
    transform: scale(0.82);
    transition: opacity 140ms ease-out, transform 140ms cubic-bezier(0.2, 0.8, 0.2, 1);
  }
  .action-icon.visible {
    opacity: 1;
    transform: scale(1);
  }
  @media (prefers-reduced-motion: reduce) {
    .action-icon { transition: none; }
    .action-icon { transform: none; }
  }
</style>
