<script lang="ts">
  import { ZIcon, ZLoading, ZPopover, ZShowMore } from '@shival99/z-ui-svelte';
  import ElementCard from './ElementCard.svelte';
  import { TRIGGER_ON } from '../popover';
  import { LOADER, loaderFor } from '../loaders';
  import type { Block } from '../mock';
  import Markdown from './Markdown.svelte';
  import PermissionCard from './PermissionCard.svelte';
  import QuestionCard from './QuestionCard.svelte';
  import { parseQuestions, withoutOpenBlock } from '../questions';

  // `fade` is the surface behind the block; ZShowMore fades its clipped edge into that colour.
  let { block, fade, agent, onanswer, onquestions, onrendered, answered = false, showQuestions = true, running = true }: { block: Block; fade?: string; agent?: import('../sessions.svelte').Agent; onanswer?: (answer: 'once' | 'always' | 'deny') => void; /** Sends the answers to questions the agent asked in this block. */ onquestions?: (text: string) => void; /** Called when this response block's final text is visible. */ onrendered?: () => void; /** Something came after this block: its questions were answered (or passed). */ answered?: boolean; /** Keep an unanswered question beside the composer instead of in the scrolling transcript. */ showQuestions?: boolean; /** The chat is still running this reply: a block still marked streaming spins only then. */ running?: boolean } = $props();

  // An agent's answer can end in questions for the user (a ```questions block): shown as cards, not as JSON.
  const asked = $derived(block.kind === 'response' && fade === undefined && !block.streaming ? parseQuestions(block.text) : { questions: [], rest: '' });
  const answerText = $derived(block.kind !== 'response' ? '' : block.streaming ? withoutOpenBlock(block.text) : asked.questions.length ? asked.rest : block.text);

  // A finished answer that leaves no text to draw (only a questions block) is "rendered" at once: the reply's footer and
  // its queued messages wait for every answer block to report it (see replyBlockRendered).
  $effect(() => {
    if (block.kind === 'response' && fade === undefined && !block.streaming && !answerText) onrendered?.();
  });

  // Thinking starts folded. The body animates by growing its grid row from 0fr to 1fr.
  let thinkingOpen = $state(false);
  // Reasoning often ends in blank lines; pre-wrap would show them as empty space under the text.
  const thinkingText = $derived(block.kind === 'thinking' ? block.text.trim() : '');

  let chip = $state<HTMLElement | null>(null);
  let cardOpen = $state(false);

  const statusIcon = { running: 'lucideLoaderCircle', ok: 'lucideCircleCheck', fail: 'lucideCircleX' } as const;
  const statusTone = { running: 'text-warning', ok: 'text-success', fail: 'text-destructive' } as const;
</script>

{#if block.kind === 'thinking' && block.text.trim()}
  <!-- Same flat style as the command blocks; the body slides open and shut. -->
  <div class="block-in rounded-lg border border-border bg-muted/30 text-sm text-muted-foreground">
    <button
      type="button"
      class="flex h-9 w-full cursor-pointer items-center gap-2 rounded-lg px-3 text-left font-medium outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
      aria-expanded={thinkingOpen}
      onclick={() => (thinkingOpen = !thinkingOpen)}
    >
      <ZIcon zType="lucideLightbulb" class="size-3.5 shrink-0" />
      {#if block.streaming && running}<ZLoading zLoading zSpinner={LOADER.reasoning} zSize="2xs" zColor="primary" />{/if}
      Thinking
      <ZIcon zType="lucideChevronDown" class="ml-auto size-3.5 shrink-0 transition-transform duration-300 {thinkingOpen ? 'rotate-180' : ''}" />
    </button>
    <div class="grid transition-[grid-template-rows] duration-300 ease-out motion-reduce:transition-none {thinkingOpen ? 'grid-rows-[1fr]' : 'grid-rows-[0fr]'}">
      <div class="min-h-0 overflow-hidden">
        <div class="px-3 pb-2">
          <!-- Short reasoning is shown as is: the show-more box keeps room for its button even when it has none. -->
          {#if thinkingText.length > 600}
            <!-- Match the card surface so the fade fully covers the clipped text without showing a background seam. -->
            <ZShowMore zMaxHeight={160} zButtonSize="sm" zBlurColor="color-mix(in oklab, var(--muted) 30%, transparent)" moreLabel="Show more" lessLabel="Show less">
              <Markdown text={thinkingText} />
            </ZShowMore>
          {:else}
            <Markdown text={thinkingText} />
          {/if}
        </div>
      </div>
    </div>
  </div>
{:else if block.kind === 'response' && fade === undefined}
  <!-- An agent's answer: rendered as markdown, full length. -->
  {#if answerText}<div class="block-in text-sm leading-relaxed text-foreground"><Markdown text={answerText} streaming={block.streaming} {onrendered} /></div>{/if}
  {#if asked.questions.length && onquestions && showQuestions}<QuestionCard questions={asked.questions} {answered} onsubmit={onquestions} />{/if}
{:else if block.kind === 'response'}
  <!-- Your own message: plain text, folded when long. -->
  <div class="block-in text-sm leading-relaxed text-foreground">
    <ZShowMore zMaxHeight={240} zButtonSize="sm" zBlurColor={fade}>
      <p class="m-0 whitespace-pre-wrap">{block.text}</p>
    </ZShowMore>
  </div>
{:else if block.kind === 'command'}
  <div class="block-in overflow-hidden rounded-lg border border-border bg-background">
    <div class="flex items-center gap-2 border-b border-border bg-muted/40 px-3 py-1.5 font-mono text-xs">
      <ZIcon zType="lucideTerminal" class="size-3.5 text-muted-foreground" />
      <span class="truncate">{block.command}</span>
      <span class="ml-auto flex items-center gap-1 {statusTone[block.status]}">
        {#if block.status === 'running'}
          <ZLoading zLoading zSpinner={loaderFor(block.command)} zSize="2xs" zColor="current" />
        {:else}
          <ZIcon zType={statusIcon[block.status]} class="size-3.5" />
        {/if}
        {block.status}
      </span>
    </div>
    {#if block.output}
      <div class="px-3 py-2 font-mono text-xs text-muted-foreground">
        <ZShowMore zMaxHeight={120} zButtonSize="sm" zBlurColor="var(--background)">
          <pre class="m-0 whitespace-pre-wrap">{block.output}</pre>
        </ZShowMore>
      </div>
    {/if}
  </div>
{:else if block.kind === 'permission'}
  <!-- Pending requests are shown above the input; the conversation keeps only the answered ones. -->
  {#if block.state !== 'pending'}<PermissionCard {block} {agent} {onanswer} />{/if}
{:else if block.kind === 'element'}
  <!-- Click for what was sent with the message, in a card (the native tooltip ran off the window with the long selector). -->
  <button type="button" bind:this={chip} class="block-in inline-flex max-w-full cursor-pointer items-center gap-1.5 self-start rounded-md border px-2 py-1 font-mono text-[11px] outline-none {cardOpen ? `${TRIGGER_ON} border-primary/60 ring-1 ring-primary/60` : 'border-border bg-muted/40 text-muted-foreground hover:bg-muted'}" aria-label="Details of {block.selector}" aria-expanded={cardOpen} onclick={() => (cardOpen = !cardOpen)}>
    <ZIcon zType="lucideMousePointerClick" class="size-3 shrink-0 text-primary" />
    {#if block.picture}<img src={block.picture} alt="" class="size-5 shrink-0 rounded-sm border border-border object-cover" />{/if}
    <span class="truncate">&lt;{block.tag} /&gt; {block.text}</span>
  </button>
  <ZPopover anchor={chip} bind:open={cardOpen} zTrigger="manual" zPosition="top-left" zOffset={8} zWidth={360}>
    {#snippet children()}
      <ElementCard tag={block.tag} text={block.text} selector={block.selector} url={block.url} note={block.note} box={block.box} style={block.style} picture={block.picture} />
    {/snippet}
  </ZPopover>
{:else if block.kind === 'image'}
  <img src={block.src} alt={block.name} class="block-in max-h-56 max-w-full self-start rounded-lg border border-border object-contain" />
{:else if block.kind === 'loading'}
  <div class="block-in flex items-center gap-2 text-xs text-muted-foreground">
    <ZLoading zLoading zSpinner={LOADER.thinking} zSize="xs" zColor="primary" />
    <span>{block.label}</span>
  </div>
{/if}
