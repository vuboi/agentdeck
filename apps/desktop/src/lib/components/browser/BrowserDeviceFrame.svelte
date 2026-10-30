<script lang="ts">
  import { ZIcon, zTooltip } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { browser, DEVICES } from '../../browser/browser.svelte';

  let { deviceBox, morphing }: { deviceBox: { x: number; y: number; w: number; h: number; zoom: number }; morphing: boolean } = $props();

  const glideClass = 'transition-[left,top,width,height,border-radius] duration-300 ease-[cubic-bezier(.2,.8,.2,1)] motion-reduce:transition-none';
</script>

<!-- The device: a bezel around the page (the page itself is the native layer above) and its caption. -->
<div
  class="pointer-events-none absolute border-[6px] border-black bg-background shadow-lg ring-1 ring-border {browser.device === 'mobile' ? 'rounded-2xl' : 'rounded-xl'} {morphing ? glideClass : ''}"
  style:left="{deviceBox.x - 6}px"
  style:top="{deviceBox.y - 6}px"
  style:width="{deviceBox.w + 12}px"
  style:height="{deviceBox.h + 12}px"
  aria-hidden="true"
></div>
<div class="absolute flex items-center justify-center gap-2 text-[0.6875rem] text-muted-foreground {morphing ? glideClass : ''}" style:left="{deviceBox.x - 6}px" style:width="{deviceBox.w + 12}px" style:top="{deviceBox.y - 30}px">
  <span>{DEVICES[browser.device].name} · {browser.landscape ? `${DEVICES[browser.device].h}×${DEVICES[browser.device].w}` : `${DEVICES[browser.device].w}×${DEVICES[browser.device].h}`}{deviceBox.zoom < 1 ? ` · ${Math.round(deviceBox.zoom * 100)}%` : ''}</span>
  <button type="button" class="grid size-5 cursor-pointer place-items-center rounded hover:bg-accent hover:text-foreground" aria-label="Rotate" onclick={() => (browser.landscape = !browser.landscape)} {@attach zTooltip({ content: 'Rotate', position: 'top' })}>
    <ZIcon zType={'lucideRotateCw' as ZIconName} class="size-3" />
  </button>
</div>
