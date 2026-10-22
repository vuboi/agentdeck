<script lang="ts">
  import { ZButton, ZIcon, zTooltip } from '@shival99/z-ui-svelte';
  import { ticker } from '../clock.svelte';
  import { ago, answerText, elapsed, fullDate } from '../blocks';
  import type { Message } from '../mock';

  // Under each answer: copy, fork, how long it took, and how long ago it was (hover for the date).
  let { message, onfork }: { message: Message; onfork: () => void } = $props();

  let copied = $state(false);
  async function copy() {
    try {
      await navigator.clipboard.writeText(answerText(message.blocks));
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch {
      // No clipboard access: nothing to show.
    }
  }
</script>

<div class="mt-0.5 -ml-1 flex items-center gap-0.5 text-muted-foreground opacity-0 transition-opacity duration-150 group-hover/msg:opacity-100 focus-within:opacity-100">
  <ZButton zWave={false} zType="ghost" zSize="sm" class="size-5! p-0" aria-label="Copy answer" onclick={copy} {@attach zTooltip({ content: copied ? 'Copied' : 'Copy', position: 'top' })}>
    <ZIcon zType={copied ? 'lucideCheck' : 'lucideCopy'} class="size-3" />
  </ZButton>
  <ZButton zWave={false} zType="ghost" zSize="sm" class="size-5! p-0" aria-label="Fork chat from here" onclick={onfork} {@attach zTooltip({ content: 'Fork chat', position: 'top' })}>
    <ZIcon zType="lucideGitBranchPlus" class="size-3" />
  </ZButton>
  <span class="ml-1.5 flex items-center gap-1.5 text-[11px]">
    {#if message.ms}<span class="tabular-nums">Worked for {elapsed(message.ms)}</span><span aria-hidden="true">·</span>{/if}
    {#if message.at}
      <span {@attach zTooltip({ content: fullDate(message.at), position: 'top' })}>{ago(new Date(new Date(message.at).getTime() + (message.ms ?? 0)).toISOString(), ticker.min)}</span>
    {:else}
      <span>{message.time}</span>
    {/if}
  </span>
</div>
