<script lang="ts">
  import { ZButton, ZIcon } from '@shival99/z-ui-svelte';

  let draft = $state('');
  let area: HTMLTextAreaElement | undefined = $state();

  function resize() {
    if (!area) return;
    area.style.height = 'auto';
    area.style.height = `${Math.min(area.scrollHeight, 180)}px`;
  }
  function submit() { draft = ''; resize(); }
</script>

<form class="rounded-2xl border border-input bg-card p-3 shadow-lg transition-shadow focus-within:ring-2 focus-within:ring-ring/40" onsubmit={(event) => { event.preventDefault(); submit(); }}>
  <textarea
    bind:this={area}
    bind:value={draft}
    oninput={resize}
    onkeydown={(event) => { if (event.key === 'Enter' && !event.shiftKey) { event.preventDefault(); submit(); } }}
    rows="1"
    aria-label="Message"
    placeholder="Message AgentDeck…"
    class="max-h-45 min-h-6 w-full resize-none border-0 bg-transparent px-1 pt-1 pb-2 text-sm leading-normal outline-none placeholder:text-muted-foreground"
  ></textarea>
  <div class="flex items-center justify-between">
    <div class="flex items-center gap-1">
      <ZButton zWave={false} zType="ghost" zSize="sm" class="h-7 w-7" aria-label="Attach file">
        <ZIcon zType="lucidePaperclip" class="size-4" />
      </ZButton>
      <ZButton zWave={false} zType="ghost" zSize="sm" class="h-7 gap-1 px-2 text-xs text-primary">
        <ZIcon zType="lucideSparkles" class="size-3.5" />
        Claude
        <ZIcon zType="lucideChevronDown" class="size-3" />
      </ZButton>
      <ZButton zWave={false} zType="ghost" zSize="sm" class="h-7 gap-1 px-2 text-xs text-muted-foreground">
        Sonnet 4
        <ZIcon zType="lucideChevronDown" class="size-3" />
      </ZButton>
    </div>
    <ZButton zWave={false} type="submit" zType="default" zSize="default" class="size-8 rounded-full p-0" disabled={!draft.trim()} aria-label="Send message">
      <ZIcon zType="lucideArrowUp" class="size-4" />
    </ZButton>
  </div>
</form>
