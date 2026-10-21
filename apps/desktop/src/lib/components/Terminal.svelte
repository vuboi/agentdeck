<script lang="ts">
  import { slide } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { ZButton, ZIcon } from '@shival99/z-ui-svelte';
  import { terminal, newTerminal, closeTerminal, toggleTerminal } from '../terminal.svelte';

  let input = $state('');
  let body: HTMLDivElement | undefined = $state();

  const active = $derived(terminal.tabs.find((tab) => tab.id === terminal.active));

  function submit(event: SubmitEvent) {
    event.preventDefault();
    const command = input.trim();
    input = '';
    if (!active) return;
    active.lines.push(`$ ${command}`);
    if (command) active.lines.push('(mock) command queued. Real execution arrives with agentd.');
    queueMicrotask(() => body?.scrollTo({ top: body.scrollHeight }));
  }
</script>

{#if terminal.open}
  <section
    class="flex min-h-0 flex-1 flex-col bg-background"
    transition:slide={{ duration: 260, easing: cubicOut }}
    aria-label="Terminal"
  >
    <div class="flex h-9 shrink-0 items-center gap-1 px-2">
      <div class="flex min-w-0 flex-1 items-center gap-1 overflow-x-auto" role="tablist" aria-label="Terminals">
        {#each terminal.tabs as tab (tab.id)}
          <ZButton zWave={false}
            role="tab"
            aria-selected={tab.id === terminal.active}
            zType={tab.id === terminal.active ? 'subtle' : 'ghost'}
            zSize="sm"
            class="h-7 gap-1.5 px-2 text-xs"
            onclick={() => (terminal.active = tab.id)}
          >
            <ZIcon zType="lucideSquareTerminal" class="size-3.5" />
            {tab.name}
            <span
              role="button"
              tabindex="-1"
              class="ml-1 inline-flex items-center rounded p-0.5 hover:bg-muted"
              aria-label="Close terminal"
              onclick={(event) => { event.stopPropagation(); closeTerminal(tab.id); }}
              onkeydown={() => {}}
            ><ZIcon zType="lucideX" class="size-3" /></span>
          </ZButton>
        {/each}
      </div>
      <ZButton zWave={false} zType="ghost" zSize="sm" aria-label="New terminal (Ctrl+Shift+`)" onclick={newTerminal}>
        <ZIcon zType="lucidePlus" class="size-4" />
      </ZButton>
      <ZButton zWave={false} zType="ghost" zSize="sm" aria-label="Hide terminal (Ctrl+`)" onclick={toggleTerminal}>
        <ZIcon zType="lucideChevronDown" class="size-4" />
      </ZButton>
    </div>

    <div bind:this={body} class="min-h-0 flex-1 overflow-y-auto px-4 py-3 font-mono text-xs leading-relaxed text-muted-foreground">
      {#each active?.lines ?? [] as line}
        <div class="whitespace-pre-wrap">{line}</div>
      {/each}
      <form class="flex items-center gap-2" onsubmit={submit}>
        <span class="text-success">❯</span>
        <input
          bind:value={input}
          aria-label="Terminal input"
          class="min-w-0 flex-1 bg-transparent text-foreground outline-none"
          autocomplete="off"
          spellcheck="false"
        />
      </form>
    </div>
  </section>
{/if}
