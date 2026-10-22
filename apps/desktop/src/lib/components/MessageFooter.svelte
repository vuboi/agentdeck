<script lang="ts">
  import { ZButton, ZIcon, zTooltip } from '@shival99/z-ui-svelte';
  import { ticker } from '../clock.svelte';
  import { toast } from '../toast';
  import { ago, answerText, elapsed, fullDate } from '../blocks';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import type { Message } from '../mock';

  // Under each answer: copy, fork, how long it took, and how long ago it was (hover for the date).
  let { message, onfork, onedit, end = false }: { message: Message; onfork: () => void; onedit?: () => void; end?: boolean } = $props();

  let copied = $state(false);
  async function copy() {
    try {
      await navigator.clipboard.writeText(answerText(message.blocks));
      copied = true;
      setTimeout(() => (copied = false), 1500);
      toast.success('Copied');
    } catch {
      toast.error('Could not copy');
    }
  }
</script>

<div class="mt-2.5 flex items-center {end ? 'flex-row-reverse' : '-ml-1'} gap-1 text-muted-foreground opacity-0 transition-opacity duration-150 group-hover/msg:opacity-100 focus-within:opacity-100">
  <ZButton zWave={false} zType="ghost" zSize="sm" class="size-5! p-0 !bg-transparent !shadow-none !ring-0 !outline-none hover:!bg-transparent hover:!text-primary focus-visible:!text-primary" aria-label="Copy answer" onclick={copy} {@attach zTooltip({ content: copied ? 'Copied' : 'Copy', position: 'top' })}>
    <ZIcon zType={copied ? 'lucideCheck' : 'lucideCopy'} zSize="14" class="size-[14px]!" />
  </ZButton>
  <ZButton zWave={false} zType="ghost" zSize="sm" class="size-5! p-0 !bg-transparent !shadow-none !ring-0 !outline-none hover:!bg-transparent hover:!text-primary focus-visible:!text-primary" aria-label="Fork chat from here" onclick={onfork} {@attach zTooltip({ content: 'Fork chat', position: 'top' })}>
    <ZIcon zType="lucideGitBranchPlus" zSize="14" class="size-[14px]!" />
  </ZButton>
  {#if onedit}
    <ZButton zWave={false} zType="ghost" zSize="sm" class="size-5! p-0 !bg-transparent !shadow-none !ring-0 !outline-none hover:!bg-transparent hover:!text-primary focus-visible:!text-primary" aria-label="Edit message" onclick={onedit} {@attach zTooltip({ content: 'Edit', position: 'top' })}>
      <ZIcon zType={'lucidePencil' as ZIconName} zSize="14" class="size-[14px]!" />
    </ZButton>
  {/if}
  <span class="mx-1 flex items-center gap-1.5 text-[11px]">
    {#if message.ms}<span class="tabular-nums">Worked for {elapsed(message.ms)}</span><span aria-hidden="true">·</span>{/if}
    {#if message.at}
      <span {@attach zTooltip({ content: fullDate(message.at), position: 'top' })}>{ago(new Date(new Date(message.at).getTime() + (message.ms ?? 0)).toISOString(), ticker.min)}</span>
    {:else}
      <span>{message.time}</span>
    {/if}
  </span>
</div>
