<script lang="ts">
  import { ZButton, ZIcon, zTooltip } from '@shival99/z-ui-svelte';
  import { ticker } from '../clock.svelte';
  import { toast } from '../toast';
  import { ago, answerText, elapsed, fullDate } from '../blocks';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import type { Message } from '../mock';

  // Under each answer: copy, fork, how long it took, and how long ago it was (hover for the date).
  import { speech, toggleSpeak } from '../speech.svelte';
  let { message, onfork, onedit, oncopyall, speak, end = false }: { message: Message; onfork: () => void; onedit?: () => void; /** The markdown to read aloud (an answer): the speaker button shows when it is given. */ speak?: string; /** Your message with attachments: Copy takes text, pictures, notes and files together. */ oncopyall?: () => void | Promise<void>; end?: boolean } = $props();

  // The buttons show only while the row is hovered or focused, and each one costs a tooltip and an icon to mount: a
  // row scrolled into view gets an empty line of the same height, and the buttons the first time it is pointed at.
  // Fewer nodes per row is what keeps a fast scroll or a scrollbar drag from dropping frames.
  let armed = $state(false);
  function arm(node: HTMLElement) {
    const row = node.closest('article') ?? node;
    // Mounted under the pointer (an edit cancelled or sent from this row): no pointerenter comes, so arm now.
    if (row.matches(':hover') || row.contains(document.activeElement)) {
      armed = true;
      return;
    }
    const on = () => (armed = true);
    row.addEventListener('pointerenter', on, { once: true });
    row.addEventListener('pointermove', on, { once: true });
    row.addEventListener('focusin', on, { once: true });
    return () => {
      row.removeEventListener('pointerenter', on);
      row.removeEventListener('pointermove', on);
      row.removeEventListener('focusin', on);
    };
  }

  // One Copy for every message: a message with attachments copies them too, so pasting it in the message box brings
  // back the pictures, notes and files, not only the words.
  let copied = $state(false);
  async function copy() {
    try {
      if (oncopyall) await oncopyall();
      else {
        await navigator.clipboard.writeText(answerText(message.blocks));
        toast.success('Copied');
      }
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch {
      toast.error('Could not copy');
    }
  }
</script>

<div class="mt-2.5 flex h-5 items-center {end ? 'flex-row-reverse' : '-ml-1'} gap-1 text-muted-foreground opacity-0 transition-opacity duration-150 group-hover/msg:opacity-100 focus-within:opacity-100" {@attach arm}>
  {#if armed}
  <ZButton zWave={false} zType="ghost" zSize="sm" class="size-5! p-0 !bg-transparent !shadow-none !ring-0 !outline-none hover:!bg-transparent hover:!text-primary focus-visible:!text-primary" aria-label={oncopyall ? 'Copy with attachments' : 'Copy answer'} onclick={copy} {@attach zTooltip({ content: copied ? 'Copied' : oncopyall ? 'Copy with attachments' : 'Copy', position: 'top' })}>
    <ZIcon zType={copied ? 'lucideCheck' : 'lucideCopy'} zSize="14" class="size-[14px]!" />
  </ZButton>
  <ZButton zWave={false} zType="ghost" zSize="sm" class="size-5! p-0 !bg-transparent !shadow-none !ring-0 !outline-none hover:!bg-transparent hover:!text-primary focus-visible:!text-primary" aria-label="Fork chat from here" onclick={onfork} {@attach zTooltip({ content: 'Fork chat', position: 'top' })}>
    <ZIcon zType="lucideGitBranchPlus" zSize="14" class="size-[14px]!" />
  </ZButton>
  {#if speak !== undefined}
    {@const mine = speech.id === message.id && speech.phase !== 'idle'}
    <ZButton zWave={false} zType="ghost" zSize="sm" class="size-5! p-0 !bg-transparent !shadow-none !ring-0 !outline-none hover:!bg-transparent hover:!text-primary focus-visible:!text-primary {mine ? '!text-primary' : ''}" aria-label={mine ? 'Stop reading' : 'Read aloud'} onclick={() => void toggleSpeak(message.id, speak)} {@attach zTooltip({ content: mine ? (speech.phase === 'installing' ? `Downloading the voice ${Math.round(speech.progress * 100)}%` : 'Stop') : 'Read aloud', position: 'top' })}>
      <!-- Reading: the pause icon (two bars), in the primary colour; a click stops it. Waiting for the voice: a spinner. -->
      {@const waiting = mine && (speech.phase === 'installing' || speech.phase === 'loading')}
      <ZIcon zType={(waiting ? 'lucideLoaderCircle' : mine ? 'lucidePause' : 'lucideAudioLines') as ZIconName} zSize="14" class="size-[14px]! {waiting ? 'animate-spin' : ''}" />
    </ZButton>
  {/if}
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
  {/if}
</div>
