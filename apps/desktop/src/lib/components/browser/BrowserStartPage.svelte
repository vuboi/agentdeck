<script lang="ts">
  import { ZIcon, ZLoading, zTooltip } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import type { RunTarget } from '../../browser/browser.svelte';

  let { deviceBox, targets, starting, onrun }: { deviceBox: { x: number; y: number; w: number; h: number; zoom: number } | null; targets: RunTarget[]; starting: string[]; onrun: (target: RunTarget) => void } = $props();
</script>

<!-- The page itself scrolls (its bar sits against the pane's edge); the content is centred while it fits. -->
<div class="absolute overflow-y-auto text-center text-sm text-muted-foreground {deviceBox ? '' : 'inset-0'}" style:left={deviceBox ? `${deviceBox.x}px` : undefined} style:top={deviceBox ? `${deviceBox.y}px` : undefined} style:width={deviceBox ? `${deviceBox.w}px` : undefined} style:height={deviceBox ? `${deviceBox.h}px` : undefined}>
  <div class="flex min-h-full flex-col px-1.5 py-8">
  <div class="m-auto flex flex-col items-center gap-2">
    <ZIcon zType="lucideGlobe" zSize="28" class="size-7 opacity-60" />
    <p class="m-0">{targets.length ? 'Run the project, or enter an address above.' : 'Search or enter an address above.'}</p>
    {#if targets.length}
      <div class="mt-1 flex w-72 max-w-full flex-col gap-1.5 text-left" role="list" aria-label="Ways to run the project">
        {#each targets as target (`${target.cwd}|${target.command}`)}
          {@const key = `${target.cwd}|${target.command}`}
          <div class="flex items-center gap-2 rounded-md border border-border bg-background/60 px-2.5 py-1.5" role="listitem">
            <div class="min-w-0 flex-1">
              <div class="flex min-w-0 items-center gap-1.5">
                <span class="truncate text-sm font-medium text-foreground">{target.name}</span>
                {#if target.url}<span class="shrink-0 rounded bg-muted px-1 font-mono text-[9px] leading-4">{target.url.match(/:(\d+)/)?.[1]}</span>{/if}
                {#if target.wrapper}<span class="shrink-0 rounded bg-muted px-1 font-mono text-[9px] leading-4" title="Runs {target.wrapper}{target.prompts ? ', which asks questions: run it in the Terminal' : ''}">script{target.prompts ? ' · asks' : ''}</span>{/if}
              </div>
              <div class="truncate font-mono text-[10px]" title={target.command}>{target.command}</div>
            </div>
            <button
              type="button"
              class="grid size-7 shrink-0 cursor-pointer place-items-center rounded-md outline-none hover:bg-accent hover:text-foreground disabled:pointer-events-none disabled:opacity-50"
              aria-label="Run {target.name}"
              disabled={starting.includes(key)}
              {@attach zTooltip({ content: starting.includes(key) ? 'Starting…' : 'Run and open in the Preview', position: 'top' })}
              onclick={() => onrun(target)}
            >
              {#if starting.includes(key)}<ZLoading zLoading zSpinner="spinner" zSize="xs" zColor="primary" />{:else}<ZIcon zType={'lucidePlay' as ZIconName} class="size-4" />{/if}
            </button>
          </div>
        {/each}
      </div>
    {:else}
      <p class="m-0 max-w-60">Dev servers on localhost work best. Inspect elements, mark up the page, and send it all to the chat.</p>
    {/if}
  </div>
  </div>
</div>
