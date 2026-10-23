<script lang="ts">
  import { slide } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { ZButton, ZIcon, zTooltip } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { terminal, newTerminal, closeTerminal, toggleTerminal } from '../terminal.svelte';
  import { activeSession, projectOf } from '../sessions.svelte';
  import { attachSession, clearSession, copyOrPaste, findInSession, fitSession, focusSession, terminalAvailable, type SessionHooks } from '../terminal-session';

  let searching = $state(false);
  let query = $state('');
  let finder: HTMLInputElement | undefined = $state();

  /** The folder of the open chat's project: a new shell starts there. */
  const folder = () => {
    const session = activeSession();
    return session ? projectOf(session)?.path || undefined : undefined;
  };

  const hooksOf = (id: string): SessionHooks => ({
    title: (name) => {
      const tab = terminal.tabs.find((item) => item.id === id);
      if (tab) tab.name = name;
    },
    status: (status) => {
      const tab = terminal.tabs.find((item) => item.id === id);
      if (tab) tab.status = status;
    },
    find: () => openFind(),
  });

  /** Puts a tab's terminal into its box, keeps it fitted to it, and gives it the keyboard when it is the open one. */
  function mount(element: HTMLElement, id: string) {
    attachSession(id, element, folder(), hooksOf(id));
    let frame = 0;
    const observer = new ResizeObserver(() => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => fitSession(id));
    });
    observer.observe(element);
    return () => {
      cancelAnimationFrame(frame);
      observer.disconnect();
    };
  }

  // Switching tab: the shown one is fitted (it had no size while hidden) and takes the keyboard.
  $effect(() => {
    const id = terminal.active;
    if (!terminal.open) return;
    requestAnimationFrame(() => {
      fitSession(id);
      if (!searching) focusSession(id);
    });
  });

  function openFind() {
    searching = true;
    queueMicrotask(() => finder?.select());
  }

  function closeFind() {
    searching = false;
    findInSession(terminal.active, '');
    focusSession(terminal.active);
  }

  function findKey(event: KeyboardEvent) {
    if (event.key === 'Escape') closeFind();
    else if (event.key === 'Enter') {
      event.preventDefault();
      findInSession(terminal.active, query, event.shiftKey);
    }
  }

  const iconButton = 'size-7! shrink-0 p-0 !bg-transparent !text-muted-foreground hover:!bg-primary/10 hover:!text-primary';
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
            zType="ghost"
            zSize="sm"
            class="h-7 gap-1.5 px-2 text-xs {tab.id === terminal.active ? 'bg-primary/15 text-primary hover:bg-primary/20' : 'hover:bg-primary/10 hover:text-primary'}"
            onclick={() => (terminal.active = tab.id)}
          >
            <ZIcon zType="lucideSquareTerminal" class="size-3.5" />
            <span class="max-w-32 truncate">{tab.name}</span>
            {#if tab.status === 'exited'}<span class="size-1.5 shrink-0 rounded-full bg-muted-foreground" aria-label="Ended"></span>{/if}
            <span
              role="button"
              tabindex="-1"
              class="ml-0.5 grid size-4.5 shrink-0 cursor-pointer place-items-center rounded-full hover:bg-primary/20"
              aria-label="Close terminal"
              onclick={(event) => { event.stopPropagation(); closeTerminal(tab.id); }}
              onkeydown={() => {}}
            ><ZIcon zType="lucideX" class="size-3" /></span>
          </ZButton>
        {/each}
      </div>
      <ZButton zWave={false} zType="ghost" zSize="sm" class={iconButton} aria-label="New terminal" {@attach zTooltip({ content: 'New terminal (Ctrl+Shift+`)', position: 'top' })} onclick={newTerminal}>
        <ZIcon zType="lucidePlus" class="size-4" />
      </ZButton>
      {#if terminalAvailable()}
        <ZButton zWave={false} zType="ghost" zSize="sm" class={iconButton} aria-label="Find" {@attach zTooltip({ content: 'Find (Ctrl+F)', position: 'top' })} onclick={openFind}>
          <ZIcon zType="lucideSearch" class="size-4" />
        </ZButton>
        <ZButton zWave={false} zType="ghost" zSize="sm" class={iconButton} aria-label="Clear" {@attach zTooltip({ content: 'Clear', position: 'top' })} onclick={() => { clearSession(terminal.active); focusSession(terminal.active); }}>
          <ZIcon zType={'lucideBan' as ZIconName} class="size-4" />
        </ZButton>
      {/if}
      <ZButton zWave={false} zType="ghost" zSize="sm" class={iconButton} aria-label="Hide terminal" {@attach zTooltip({ content: 'Hide (Ctrl+`)', position: 'top' })} onclick={toggleTerminal}>
        <ZIcon zType="lucideChevronDown" class="size-4" />
      </ZButton>
    </div>

    {#if !terminalAvailable()}
      <div class="flex min-h-0 flex-1 items-center justify-center px-4 text-center text-xs text-muted-foreground">The terminal runs in the desktop app.</div>
    {:else}
      <div class="relative min-h-0 flex-1">
        {#if searching}
          <div class="absolute top-1 right-4 z-10 flex items-center gap-1 rounded-md border border-border bg-popover p-1 shadow-md">
            <input
              bind:this={finder}
              bind:value={query}
              aria-label="Find in terminal"
              placeholder="Find"
              class="h-6 w-44 rounded bg-transparent px-1.5 text-xs text-foreground outline-none placeholder:text-muted-foreground"
              autocomplete="off"
              spellcheck="false"
              oninput={() => findInSession(terminal.active, query)}
              onkeydown={findKey}
            />
            <button type="button" class="grid size-6 cursor-pointer place-items-center rounded text-muted-foreground hover:bg-primary/10 hover:text-primary" aria-label="Previous match" onclick={() => findInSession(terminal.active, query, true)}>
              <ZIcon zType={'lucideChevronUp' as ZIconName} class="size-3.5" />
            </button>
            <button type="button" class="grid size-6 cursor-pointer place-items-center rounded text-muted-foreground hover:bg-primary/10 hover:text-primary" aria-label="Next match" onclick={() => findInSession(terminal.active, query)}>
              <ZIcon zType="lucideChevronDown" class="size-3.5" />
            </button>
            <button type="button" class="grid size-6 cursor-pointer place-items-center rounded text-muted-foreground hover:bg-primary/10 hover:text-primary" aria-label="Close find" onclick={closeFind}>
              <ZIcon zType="lucideX" class="size-3.5" />
            </button>
          </div>
        {/if}
        {#each terminal.tabs as tab (tab.id)}
          <!-- Every tab keeps its box (a hidden one has no size), so its shell and scrollback stay. -->
          <div
            class="absolute inset-0 px-3 pb-2 {tab.id === terminal.active ? '' : 'hidden'}"
            role="tabpanel"
            tabindex="-1"
            aria-label={tab.name}
            oncontextmenu={(event) => { event.preventDefault(); void copyOrPaste(tab.id); }}
            {@attach (element) => mount(element, tab.id)}
          ></div>
        {/each}
      </div>
    {/if}
  </section>
{/if}
