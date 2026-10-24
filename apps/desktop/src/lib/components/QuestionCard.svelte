<script lang="ts">
  import { untrack } from 'svelte';
  import { ZButton, ZIcon } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { formatAnswers, type Answer, type Question } from '../questions';

  // Questions an agent asked before going on, one card at a time: pick an option (or several), or type your own,
  // then Next. The arrows go back to an earlier question to change its answer. Send puts every answer in one message.
  let { questions, answered = false, from = '', onsubmit }: { questions: Question[]; answered?: boolean; from?: string; onsubmit: (text: string) => void } = $props();

  const icon = (name: string) => name as ZIconName;
  let at = $state(0);
  // One answer per question, made once: the questions of a card do not change.
  let answers = $state<Answer[]>(untrack(() => questions.map(() => ({ picked: [], text: '' }))));
  let sent = $state(false);
  const done = $derived(answered || sent);
  const current = $derived(questions[at]);
  const chosen = (index: number) => answers[index].picked.length > 0 || answers[index].text.trim().length > 0;
  const last = $derived(at === questions.length - 1);

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

  function send() {
    if (done) return;
    sent = true;
    onsubmit(formatAnswers(questions, answers));
  }

  function onkeydown(event: KeyboardEvent) {
    if (done || (event.target as HTMLElement).tagName === 'TEXTAREA') return;
    const number = Number(event.key);
    if (number >= 1 && number <= current.options.length) {
      event.preventDefault();
      pick(current.options[number - 1].label);
    } else if (event.key === 'ArrowLeft' && at > 0) at--;
    else if (event.key === 'ArrowRight' && !last) at++;
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions (keys 1-4 pick, arrows move) -->
<section class="block-in w-full max-w-[560px] overflow-hidden rounded-xl border border-border bg-card text-sm shadow-sm" aria-label="Questions" tabindex="0" {onkeydown}>
  <header class="flex items-center gap-2 border-b border-border bg-muted/40 px-3 py-2 text-xs">
    <ZIcon zType={icon(done ? 'lucideCircleCheck' : 'lucideMessageCircleQuestion')} class="size-3.5 shrink-0 {done ? 'text-success' : 'text-primary'}" />
    <span class="font-medium">{done ? 'Answered' : from ? `${from} needs a few answers` : 'A few questions first'}</span>
    <span class="ml-auto flex items-center gap-1 text-muted-foreground tabular-nums">
      <button type="button" class="grid size-6 cursor-pointer place-items-center rounded-md outline-none hover:bg-primary/10 hover:text-primary focus-visible:ring-2 focus-visible:ring-ring disabled:pointer-events-none disabled:opacity-35" aria-label="Previous question" disabled={at === 0} onclick={() => at--}>
        <ZIcon zType={icon('lucideChevronLeft')} class="size-3.5" />
      </button>
      {at + 1} / {questions.length}
      <button type="button" class="grid size-6 cursor-pointer place-items-center rounded-md outline-none hover:bg-primary/10 hover:text-primary focus-visible:ring-2 focus-visible:ring-ring disabled:pointer-events-none disabled:opacity-35" aria-label="Next question" disabled={last} onclick={() => at++}>
        <ZIcon zType={icon('lucideChevronRight')} class="size-3.5" />
      </button>
    </span>
  </header>

  <div class="flex flex-col gap-2.5 p-3">
    <div class="flex flex-col gap-1">
      {#if current.header}<span class="w-fit rounded bg-primary/10 px-1.5 py-px text-[10px] font-medium tracking-wide text-primary uppercase">{current.header}</span>{/if}
      <p class="m-0 font-medium text-foreground">{current.question}</p>
      {#if current.multiSelect && !done}<span class="text-[11px] text-muted-foreground">Pick any that apply</span>{/if}
    </div>

    <div class="flex flex-col gap-1.5" role={current.multiSelect ? 'group' : 'radiogroup'} aria-label={current.question}>
      {#each current.options as option, index (option.label)}
        {@const on = answers[at].picked.includes(option.label)}
        <button
          type="button"
          role={current.multiSelect ? 'checkbox' : 'radio'}
          aria-checked={on}
          disabled={done}
          class="flex w-full cursor-pointer items-start gap-2.5 rounded-lg border px-2.5 py-2 text-left outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-default {on ? 'border-primary/60 bg-primary/8' : 'border-border hover:border-primary/40 hover:bg-muted/40'}"
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
        class="field-sizing-content max-h-32 min-h-9 w-full resize-none rounded-lg border border-border bg-background px-2.5 py-2 text-[13px] outline-none placeholder:text-muted-foreground focus:border-primary/60 disabled:opacity-60"
        placeholder={current.options.length ? 'Or type your own answer…' : 'Type your answer…'}
        disabled={done}
        bind:value={answers[at].text}
      ></textarea>
    </div>
  </div>

  {#if !done}
    <footer class="flex items-center gap-2 border-t border-border px-3 py-2">
      <span class="flex gap-1" aria-hidden="true">
        {#each questions as _, index (index)}
          <button type="button" tabindex="-1" class="h-1.5 cursor-pointer rounded-full transition-all {index === at ? 'w-4 bg-primary' : chosen(index) ? 'w-1.5 bg-primary/50' : 'w-1.5 bg-muted-foreground/25'}" aria-label="Question {index + 1}" onclick={() => (at = index)}></button>
        {/each}
      </span>
      <span class="flex-1"></span>
      {#if at > 0}<ZButton zWave={false} zType="ghost" zSize="sm" onclick={() => at--}>Back</ZButton>{/if}
      {#if last}
        <ZButton zWave={false} zType="default" zSize="sm" onclick={send}><ZIcon zType={icon('lucideSend')} class="size-3.5" />Send answers</ZButton>
      {:else}
        <ZButton zWave={false} zType="default" zSize="sm" onclick={() => at++}>Next</ZButton>
      {/if}
    </footer>
  {/if}
</section>
