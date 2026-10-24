<script lang="ts">
  // The changes of a file along its scrollbar, as in VS Code: a green tick for added lines, a blue one for changed
  // lines, a red one where lines were taken out. Click a tick to go to that spot.
  let { total, added, removed, modified, ongo }: { total: number; added?: ReadonlySet<number>; removed?: ReadonlySet<number>; modified?: ReadonlySet<number>; ongo: (line: number) => void } = $props();

  type Mark = { from: number; to: number; kind: 'add' | 'mod' | 'del' };
  const marks = $derived.by(() => {
    const out: Mark[] = [];
    const run = (set: ReadonlySet<number> | undefined, kind: Mark['kind']) => {
      if (!set) return;
      let start = -1;
      let prev = -1;
      for (const line of [...set].sort((a, b) => a - b)) {
        if (start === -1) start = prev = line;
        else if (line === prev + 1) prev = line;
        else {
          out.push({ from: start, to: prev, kind });
          start = prev = line;
        }
      }
      if (start !== -1) out.push({ from: start, to: prev, kind });
    };
    run(added, 'add');
    run(modified, 'mod');
    run(removed, 'del');
    return out;
  });
</script>

{#if total > 0 && marks.length > 0}
  <div class="pointer-events-none absolute inset-y-0 right-0 z-10 w-2.5" aria-hidden="true">
    {#each marks as mark, index (index)}
      <button
        type="button"
        tabindex="-1"
        class="pointer-events-auto absolute right-0.5 w-1.5 cursor-pointer rounded-[1px] opacity-80 outline-none hover:opacity-100 {mark.kind === 'add' ? 'bg-emerald-500' : mark.kind === 'mod' ? 'bg-blue-500' : 'bg-red-500'}"
        style:top="{((mark.from - 1) / total) * 100}%"
        style:height="max(3px, {((mark.to - mark.from + 1) / total) * 100}%)"
        title={mark.kind === 'del' ? `Removed before line ${mark.from}` : `${mark.kind === 'add' ? 'Added' : 'Modified'}: line ${mark.from}${mark.to > mark.from ? `–${mark.to}` : ''}`}
        onclick={() => ongo(mark.from)}
      ></button>
    {/each}
  </div>
{/if}
