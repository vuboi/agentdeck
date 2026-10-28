<script lang="ts">
  import { untrack } from 'svelte';
  import { fade, fly } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { ZButton, ZIcon, ZInput } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { formatAnswers, isQuick, type Answer, type Question } from '../questions';
  import { toast } from '../toast';

  // Questions an agent asked before going on, one card at a time: pick an option (or several), or type your own,
  // then Next. The arrows go back to an earlier question to change its answer. Send puts every answer in one message.
  // One question with one choice among two or three options is a quick card instead: the options are buttons, a click
  // answers and sends at once, and "Other…" opens a line to type your own.
  let { questions, answered = false, from = '', onsubmit, oncancel }: { questions: Question[]; answered?: boolean; from?: string; /** Cancels the questions: nothing is sent to the agent. */ oncancel?: () => void; /** Sends the answers; false when they could not go out now (the card then stays open). */ onsubmit: (text: string) => boolean | void } = $props();

  const icon = (name: string) => name as ZIconName;
  let at = $state(0);
  // One answer per question, made once: the questions of a card do not change.
  let answers = $state<Answer[]>(untrack(() => questions.map(() => ({ picked: [], text: '' }))));
  let sent = $state(false);
  const done = $derived(answered || sent);
  const current = $derived(questions[at]);
  const chosen = (index: number) => answers[index].picked.length > 0 || answers[index].text.trim().length > 0;
  const last = $derived(at === questions.length - 1);
  const quick = $derived(isQuick(questions));
  let other = $state(false);
  let chosenLabel = $state('');

  function choose(label: string) {
    if (done) return;
    chosenLabel = label;
    answers[0].picked = [label];
    send();
  }

  const reducedMotion = typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;
  const motion = (ms: number) => (reducedMotion ? 0 : ms);

  /** Puts the cursor in the field when "Other…" opens it. */
  function focusField(node: HTMLElement) {
    queueMicrotask(() => node.querySelector('input')?.focus());
  }

  function sendOther() {
    if (done || !answers[0].text.trim()) return;
    answers[0].picked = [];
    chosenLabel = answers[0].text.trim();
    send();
  }

  function pick(label: string) {
    const answer = answers[at];
    if (current.multiSelect) {
      answer.picked = answer.picked.includes(label) ? answer.picked.filter((item) => item !== label) : [...answer.picked, label];
      return;
    }
    answer.picked = answer.picked[0] === label ? [] : [label];
    // One choice answers the question: go on to the next one by itself.
    if (answer.picked.length && !last) setTimeout(() => (at = Math.min(at + 1, questions.length - 1)), 160);
  }

  /** Hands the text over; the card turns to "answered" only when it really went out. */
  function submit(text: string): boolean {
    if (onsubmit(text) === false) {
      toast.info('A reply is still running. Answer again when it has finished.');
      chosenLabel = '';
      return false;
    }
    sent = true;
    return true;
  }

  function send() {
    if (done) return;
    submit(formatAnswers(questions, answers));
  }

  /** Skipping answers nothing: the agent is told to decide for itself and the card closes. */
  function skip() {
    if (done) return;
    submit(formatAnswers(questions, questions.map(() => ({ picked: [], text: '' }))));
  }

  function onkeydown(event: KeyboardEvent) {
    if (done || (event.target as HTMLElement).tagName === 'TEXTAREA' || (event.target as HTMLElement).tagName === 'INPUT') return;
    const number = Number(event.key);
    if (number >= 1 && number <= current.options.length) {
      event.preventDefault();
      if (quick) choose(current.options[number - 1].label);
      else pick(current.options[number - 1].label);
    } else if (event.key === 'ArrowLeft' && at > 0) at--;
    else if (event.key === 'ArrowRight' && !last) at++;
  }
</script>

{#if quick}
  <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions (keys 1-3 answer) -->
  <section class="block-in w-full max-w-[400px] rounded-lg border border-border bg-card px-2.5 py-2 text-sm shadow-sm" aria-label="Question" tabindex="0" {onkeydown}>
    <p class="m-0 flex items-start gap-1.5 text-[13px] font-medium text-foreground">
      <ZIcon zType={icon(done ? 'lucideCircleCheck' : 'lucideMessageCircleQuestion')} class="mt-0.5 size-3.5 shrink-0 {done ? 'text-success' : 'text-primary'}" />
      <span class="min-w-0">{current.question}</span>
    </p>
    {#if done}
      <p class="m-0 mt-1 pl-5 text-xs text-muted-foreground">{chosenLabel || 'Answered'}</p>
    {:else}
      <div class="mt-2 flex flex-col gap-y-2">
        {#each current.options as option, index (option.label)}
          <button
            type="button"
            class="flex min-h-10 w-full min-w-0 cursor-pointer flex-col justify-center rounded-md border px-2.5 py-1.5 text-left outline-none sm:min-h-9 transition-colors focus-visible:ring-2 focus-visible:ring-ring {index === 0 ? 'border-primary/50 bg-primary/10 hover:bg-primary/15' : 'border-border hover:border-primary/40 hover:bg-muted/40'}"
            onclick={() => choose(option.label)}
          >
            <span class="flex items-start gap-2 text-xs font-medium {index === 0 ? 'text-primary' : 'text-foreground'}">
              <span class="min-w-0 flex-1 break-words whitespace-normal">{option.label}</span>
              <kbd class="mt-px shrink-0 rounded border border-border px-1 font-mono text-[10px] font-normal text-muted-foreground max-sm:hidden">{index + 1}</kbd>
            </span>
            {#if option.description}<span class="mt-0.5 break-words whitespace-normal text-[11px] leading-snug text-muted-foreground">{option.description}</span>{/if}
          </button>
        {/each}
        <!-- "Other..." turns into the field itself, in the same place and as tall as the options: the two cross-fade in one
             grid cell, so nothing around them moves. Submit unfolds under it (grid rows 0fr to 1fr, no gap left behind). -->
        <div class="grid">
          {#if other}
            <div class="ask-field col-start-1 row-start-1 min-h-10 sm:min-h-9 [&_input]:min-h-10 sm:[&_input]:min-h-9 max-sm:[&_input]:text-base max-sm:[&_.z-input-container]:h-10" in:fly={{ y: 6, duration: motion(200), easing: cubicOut }} {@attach focusField}>
              <ZInput zSize="default" bind:value={answers[0].text} zPlaceholder="Type your own answer…" onEnter={sendOther} />
            </div>
          {:else}
            <button type="button" class="col-start-1 row-start-1 flex min-h-10 w-full cursor-pointer items-center rounded-md border border-dashed sm:min-h-9 border-border px-2.5 py-1.5 text-left text-xs text-muted-foreground outline-none transition-colors hover:border-primary/40 hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring" out:fade={{ duration: motion(110) }} onclick={() => (other = true)}>Other…</button>
          {/if}
        </div>
        <div class="-mt-2 grid transition-[grid-template-rows] duration-200 ease-out motion-reduce:transition-none {other ? 'grid-rows-[1fr]' : 'grid-rows-[0fr]'}" inert={!other}>
          <div class="min-h-0 overflow-hidden">
            <div class="flex justify-end pt-2 transition-opacity duration-200 motion-reduce:transition-none {other ? 'opacity-100 delay-75' : 'opacity-0'}">
              <ZButton zWave={false} zType="default" zSize="sm" onclick={sendOther} disabled={!answers[0].text.trim()}>Submit</ZButton>
            </div>
          </div>
        </div>
      </div>
    {/if}
    {#if oncancel && !done}
      <button type="button" class="mt-1 ml-auto block cursor-pointer rounded-md px-1.5 py-0.5 text-[11px] text-muted-foreground outline-none transition-colors hover:text-destructive focus-visible:ring-2 focus-visible:ring-ring" onclick={oncancel}>Cancel</button>
    {/if}
  </section>
{:else}
<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions (keys 1-4 pick, arrows move) -->
<section class="block-in w-full max-w-[520px] overflow-hidden rounded-lg border border-border bg-card text-sm shadow-sm" aria-label="Questions" tabindex="0" {onkeydown}>
  <header class="flex items-center gap-2 border-b border-border bg-muted/40 px-2.5 py-1.5 text-xs">
    <ZIcon zType={icon(done ? 'lucideCircleCheck' : 'lucideMessageCircleQuestion')} class="size-3.5 shrink-0 {done ? 'text-success' : 'text-primary'}" />
    <span class="font-medium">{done ? 'Answered' : from ? `${from} needs a few answers` : 'A few questions first'}</span>
    <span class="ml-auto flex items-center gap-1 text-muted-foreground tabular-nums">
      <button type="button" class="grid size-5 cursor-pointer place-items-center rounded-md outline-none hover:bg-primary/10 hover:text-primary focus-visible:ring-2 focus-visible:ring-ring disabled:pointer-events-none disabled:opacity-35" aria-label="Previous question" disabled={at === 0} onclick={() => at--}>
        <ZIcon zType={icon('lucideChevronLeft')} class="size-3.5" />
      </button>
      {at + 1} / {questions.length}
      <button type="button" class="grid size-5 cursor-pointer place-items-center rounded-md outline-none hover:bg-primary/10 hover:text-primary focus-visible:ring-2 focus-visible:ring-ring disabled:pointer-events-none disabled:opacity-35" aria-label="Next question" disabled={last} onclick={() => at++}>
        <ZIcon zType={icon('lucideChevronRight')} class="size-3.5" />
      </button>
    </span>
  </header>

  <div class="flex flex-col gap-2 p-2.5">
    <div class="flex flex-col gap-1">
      {#if current.header}<span class="w-fit rounded bg-primary/10 px-1.5 py-px text-[10px] font-medium tracking-wide text-primary uppercase">{current.header}</span>{/if}
      <p class="m-0 font-medium text-foreground">{current.question}</p>
      {#if current.multiSelect && !done}<span class="text-[11px] text-muted-foreground">Pick any that apply</span>{/if}
    </div>

    <div class="flex flex-col gap-1" role={current.multiSelect ? 'group' : 'radiogroup'} aria-label={current.question}>
      {#each current.options as option, index (option.label)}
        {@const on = answers[at].picked.includes(option.label)}
        <button
          type="button"
          role={current.multiSelect ? 'checkbox' : 'radio'}
          aria-checked={on}
          disabled={done}
          class="flex w-full cursor-pointer items-start gap-2 rounded-md border px-2 py-1.5 text-left outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-default {on ? 'border-primary/60 bg-primary/8' : 'border-border hover:border-primary/40 hover:bg-muted/40'}"
          onclick={() => pick(option.label)}
        >
          <span class="mt-0.5 grid size-4 shrink-0 place-items-center border {current.multiSelect ? 'rounded' : 'rounded-full'} {on ? 'border-primary bg-primary text-primary-foreground' : 'border-muted-foreground/40'}">
            {#if on}<ZIcon zType={icon('lucideCheck')} class="size-3" />{/if}
          </span>
          <span class="min-w-0 flex-1">
            <span class="block text-[13px] font-medium text-foreground">{option.label}</span>
            {#if option.description}<span class="block text-xs text-muted-foreground">{option.description}</span>{/if}
          </span>
          {#if !done}<kbd class="shrink-0 rounded border border-border px-1 font-mono text-[10px] text-muted-foreground">{index + 1}</kbd>{/if}
        </button>
      {/each}
      <textarea
        rows="1"
        class="field-sizing-content max-h-32 min-h-8 w-full resize-none rounded-md border border-border bg-background px-2 py-1.5 text-[13px] outline-none placeholder:text-muted-foreground focus:border-primary/60 disabled:opacity-60"
        placeholder={current.options.length ? 'Or type your own answer…' : 'Type your answer…'}
        disabled={done}
        bind:value={answers[at].text}
      ></textarea>
    </div>
  </div>

  {#if !done}
    <footer class="flex items-center gap-1.5 border-t border-border px-2.5 py-1.5">
      <span class="flex gap-1" aria-hidden="true">
        {#each questions as _, index (index)}
          <button type="button" tabindex="-1" class="h-1.5 cursor-pointer rounded-full transition-all {index === at ? 'w-4 bg-primary' : chosen(index) ? 'w-1.5 bg-primary/50' : 'w-1.5 bg-muted-foreground/25'}" aria-label="Question {index + 1}" onclick={() => (at = index)}></button>
        {/each}
      </span>
      <span class="flex-1"></span>
      {#if oncancel}<ZButton zWave={false} zType="ghost" zSize="sm" onclick={oncancel}>Cancel</ZButton>{/if}
      <ZButton zWave={false} zType="ghost" zSize="sm" onclick={skip}>Skip</ZButton>
      {#if at > 0}<ZButton zWave={false} zType="ghost" zSize="sm" onclick={() => at--}>Back</ZButton>{/if}
      {#if last}
        <ZButton zWave={false} zType="default" zSize="sm" onclick={send}><ZIcon zType={icon('lucideSend')} class="size-3.5" />Send answers</ZButton>
      {:else}
        <ZButton zWave={false} zType="default" zSize="sm" onclick={() => at++}>Next</ZButton>
      {/if}
    </footer>
  {/if}
</section>
{/if}

<style>
  /* The field's own focus ring: the library's utilities for it do not reach the box here, so it is drawn by hand. */
  .ask-field :global(.z-input-container) {
    transition: border-color 150ms ease-out, box-shadow 150ms ease-out;
  }
  .ask-field :global(.z-input-container:focus-within) {
    border-color: var(--primary);
    box-shadow: 0 0 0 3px color-mix(in oklab, var(--primary) 25%, transparent);
  }
  @media (prefers-reduced-motion: reduce) {
    .ask-field :global(.z-input-container) { transition: none; }
  }
</style>
