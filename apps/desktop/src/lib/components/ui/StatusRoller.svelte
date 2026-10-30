<script lang="ts">
  import { fly } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';

  // A status word that changes in place: the old word slides up and out while the new one slides in from below, in one
  // line of the same cell. It never passes through the words between them.
  // `sizes`: the words it can show; they are laid out invisibly so the box is as wide as the widest one.
  // `align="end"` puts the word against the right edge of the widest-word box, so a short word sits next to what follows it.
  let { current, sizes = [], class: className = '', align = 'start' }: { current: string; sizes?: readonly string[]; class?: string; align?: 'start' | 'end' } = $props();

  // The colour follows what the word says: done, failed, waiting, or still going.
  const toneOf = (text: string) =>
    /complete|done|answered/i.test(text) ? 'text-emerald-500'
    : /fail|stop/i.test(text) ? 'text-destructive'
    : /wait|pend|queue|pause|input|approval|check|need|return/i.test(text) ? 'text-amber-500'
    : 'text-primary';
</script>

<span class="grid h-[1lh] overflow-hidden align-middle leading-[1lh] {align === 'end' ? 'justify-items-end' : ''} {className}" aria-label={current}>
  {#each sizes as size (size)}
    <span class="invisible col-start-1 row-start-1 whitespace-nowrap" aria-hidden="true">{size}</span>
  {/each}
  {#key current}
    <span
      class="col-start-1 row-start-1 whitespace-nowrap {toneOf(current)}"
      in:fly={{ y: 10, duration: 320, easing: cubicOut }}
      out:fly={{ y: -10, duration: 220 }}
      aria-hidden="true">{current}</span>
  {/key}
</span>
