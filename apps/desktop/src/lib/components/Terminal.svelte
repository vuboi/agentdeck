<script lang="ts">
  import Menu from './Menu.svelte';
  import { closeOnScroll } from '../close-on-scroll';
  import { slide } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { ZButton, ZDropdownMenu, ZIcon, ZPopover, zTooltip } from '@shival99/z-ui-svelte';
  import type { ZDropdownMenuItem, ZIconName } from '@shival99/z-ui-svelte';
  import { terminal, newTerminal, closeTerminal, toggleTerminal, shells, loadShells, defaultShell } from '../terminal.svelte';
  import PickerPopover, { type PickerItem } from './PickerPopover.svelte';
  import { TRIGGER_ON } from '../popover';
  import { activeSession, projectOf } from '../sessions.svelte';
  import { saveFileAttachment } from '../agent';
  import { toast } from '../toast';
  import { activeSession as openChat, draftFiles } from '../sessions.svelte';
  import { lastCommandOutput, attachSession, clearSession, copySelection, findInSession, fitSession, focusSession, hasSelection, pasteInto, scrollSession, selectAllIn, terminalAvailable, type SessionHooks } from '../terminal-session';

  // The profile menu: the shells installed here, the one new terminals start with ticked.
  let shellAnchor = $state<HTMLElement | null>(null);
  let shellOpen = $state(false);
  $effect(() => {
    if (terminal.open) void loadShells();
  });
  const shellItems = $derived<PickerItem<string>[]>(
    shells.list.map((item) => ({ id: item.id, label: item.name, hint: item.id === defaultShell() ? 'default' : undefined, icon: (item.id === defaultShell() ? 'lucideCheck' : 'lucideSquareTerminal') as ZIconName, data: item.id })),
  );
  function pickShell(item: PickerItem<string>) {
    shellOpen = false;
    newTerminal(item.data);
  }

  /** The last command and what it printed goes to the open chat as a file attachment (the agent reads it with its tools). */
  async function attachLastOutput() {
    const text = lastCommandOutput(terminal.active);
    if (!text) return void toast.info('Nothing to attach yet: run a command first');
    const chat = openChat();
    if (!chat) return void toast.info('Open a chat to attach it to');
    try {
      const bytes = new TextEncoder().encode(text);
      let binary = '';
      for (const byte of bytes) binary += String.fromCharCode(byte);
      const stamp = new Date().toISOString().replace(/[:.]/g, '-').slice(0, 19);
      const path = await saveFileAttachment(`terminal-${stamp}.log`, btoa(binary));
      draftFiles[chat.id] = [...(draftFiles[chat.id] ?? []), { id: crypto.randomUUID(), name: 'terminal-output.log', path, size: bytes.length }];
      toast.success('Last command and its output added to the message');
    } catch (failure) {
      toast.error(`Could not attach the output: ${String(failure)}`);
    }
  }

  // The row of buttons folds into a menu when it is short.
  let headWidth = $state(0);
  const compact = $derived(headWidth > 0 && headWidth < 460);
  let toolsAnchor = $state<HTMLElement | null>(null);
  let toolsOpen = $state(false);
  const toolsItems = $derived<PickerItem<string>[]>([
    { id: 'new', label: 'New terminal', hint: 'Ctrl+Shift+`', icon: 'lucidePlus' as ZIconName, group: 'Terminal', data: 'new' },
    ...(terminalAvailable()
      ? [
          { id: 'find', label: 'Find', hint: 'Ctrl+F', icon: 'lucideSearch' as ZIconName, group: 'Terminal', data: 'find' },
          { id: 'attach', label: 'Attach last output to chat', icon: 'lucidePaperclip' as ZIconName, group: 'Terminal', data: 'attach' },
          { id: 'clear', label: 'Clear', icon: 'lucideBan' as ZIconName, group: 'Terminal', data: 'clear' },
        ]
      : []),
    ...(shells.list.length > 1 ? shells.list.map((item) => ({ id: `shell:${item.id}`, label: item.name, hint: item.id === defaultShell() ? 'default' : undefined, icon: 'lucideSquareTerminal' as ZIconName, group: 'New terminal with', data: `shell:${item.id}` })) : []),
  ]);
  function pickTools(item: PickerItem<string>) {
    toolsOpen = false;
    if (item.data === 'new') newTerminal();
    else if (item.data === 'find') openFind();
    else if (item.data === 'attach') void attachLastOutput();
    else if (item.data === 'clear') {
      clearSession(terminal.active);
      focusSession(terminal.active);
    } else if (item.data.startsWith('shell:')) newTerminal(item.data.slice(6));
  }

  let searching = $state(false);
  let query = $state('');
  let finder: HTMLInputElement | undefined = $state();
  let findButton = $state<HTMLElement | null>(null);
  // The box is ready to type in as soon as the popover is up.
  $effect(() => {
    if (!searching) return;
    const focus = () => finder?.focus({ preventScroll: true });
    const frame = requestAnimationFrame(focus);
    const late = setTimeout(focus, 60);
    return () => {
      cancelAnimationFrame(frame);
      clearTimeout(late);
    };
  });

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
    scrolled: (top, bottom) => {
      ends[id] = { top, bottom };
    },
  });

  // Where each tab's view is, for its scroll arrows.
  let ends = $state<Record<string, { top: boolean; bottom: boolean }>>({});

  // Press and hold: a step at once, then steps every 50 ms after 300 ms.
  let hold = 0;
  let holdTimer = 0;
  function press(event: PointerEvent, id: string, lines: number) {
    if (event.button !== 0) return;
    event.preventDefault();
    release();
    scrollSession(id, lines);
    hold = window.setTimeout(() => {
      holdTimer = window.setInterval(() => scrollSession(id, lines), 50);
    }, 300);
    window.addEventListener('pointerup', release, { once: true });
    window.addEventListener('pointercancel', release, { once: true });
  }
  function release() {
    clearTimeout(hold);
    clearInterval(holdTimer);
  }
  const arrow = 'absolute right-0 z-10 grid size-2.5 cursor-pointer place-items-center text-muted-foreground opacity-70 outline-none hover:opacity-100 disabled:cursor-default disabled:opacity-25 disabled:hover:opacity-25';

  /** Puts a tab's terminal into its box, keeps it fitted to it, and gives it the keyboard when it is the open one. */
  function mount(element: HTMLElement, id: string) {
    attachSession(id, element, folder(), hooksOf(id), terminal.tabs.find((item) => item.id === id)?.shell);
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

  // The right-click menu, at the pointer: Copy (only while text is selected), Paste, Select all, Clear.
  let menu = $state<{ x: number; y: number; id: string; selected: boolean } | null>(null);
  let menuOpen = $state(false);
  $effect(() => closeOnScroll(() => menuOpen, () => (menuOpen = false)));
  let menuKey = $state(0);

  function onContextMenu(event: MouseEvent, id: string) {
    event.preventDefault();
    menuKey++;
    menu = { x: event.clientX, y: event.clientY, id, selected: hasSelection(id) };
    menuOpen = true;
  }

  const item = (label: string, icon: string, shortcut: string | undefined, run: (id: string) => unknown, extra: Partial<ZDropdownMenuItem> = {}): ZDropdownMenuItem => ({
    label,
    icon: icon as ZIconName,
    iconSize: '12',
    shortcut,
    class: 'text-xs! py-1',
    onClick: () => {
      const id = menu?.id;
      if (!id) return;
      void Promise.resolve(run(id)).finally(() => focusSession(id));
    },
    ...extra,
  });

  const menuItems = $derived<ZDropdownMenuItem[]>([
    item('Copy', 'lucideCopy', 'Ctrl+Shift+C', copySelection, { hidden: !menu?.selected }),
    item('Paste', 'lucideClipboardPaste', 'Ctrl+Shift+V', pasteInto),
    item('Select all', 'lucideTextSelect', undefined, selectAllIn, { divide: 'before' }),
    item('Clear', 'lucideBan', undefined, clearSession),
  ]);

  const iconButton = 'size-7! shrink-0 p-0 !bg-transparent !text-muted-foreground hover:!bg-primary/10 hover:!text-primary';
</script>

{#if terminal.open}
  <section
    class="flex min-h-0 flex-1 flex-col bg-background"
    transition:slide={{ duration: 260, easing: cubicOut }}
    aria-label="Terminal"
  >
    <div class="flex h-9 shrink-0 items-center gap-1 px-2" bind:clientWidth={headWidth}>
      <div class="flex min-w-0 flex-1 items-center gap-1 overflow-x-auto" role="tablist" aria-label="Terminals">
        {#each terminal.tabs as tab (tab.id)}
          <ZButton zWave={false}
            role="tab"
            aria-selected={tab.id === terminal.active}
            zType="ghost"
            zSize="sm"
            class="h-7 gap-1.5 px-2 text-xs {tab.id === terminal.active ? '!bg-primary/15 !text-primary hover:!bg-primary/20 hover:!text-primary' : 'hover:bg-primary/10 hover:text-primary'}"
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
      {#if compact}
        <!-- Short row: the actions go into one menu (the same list as everywhere, icon and text). -->
        <ZButton bind:ref={toolsAnchor} zWave={false} zType="ghost" zSize="sm" class="{iconButton} {toolsOpen ? TRIGGER_ON : ''}" aria-label="Terminal menu" aria-haspopup="listbox" aria-expanded={toolsOpen} onclick={() => (toolsOpen = !toolsOpen)}>
          <ZIcon zType={'lucideMenu' as ZIconName} class="size-4" />
        </ZButton>
      {:else}
        <ZButton zWave={false} zType="ghost" zSize="sm" class={iconButton} aria-label="New terminal" {@attach zTooltip({ content: 'New terminal (Ctrl+Shift+`)', position: 'top' })} onclick={() => newTerminal()}>
          <ZIcon zType="lucidePlus" class="size-4" />
        </ZButton>
        {#if shells.list.length > 1}
          <!-- The shell new terminals start with, and a list of the others installed here (PowerShell, Git Bash, WSL, …). -->
          <ZButton bind:ref={shellAnchor} zWave={false} zType="ghost" zSize="sm" class="{iconButton} {shellOpen ? TRIGGER_ON : ''}" aria-label="Choose a shell" aria-haspopup="listbox" aria-expanded={shellOpen} {@attach zTooltip({ content: 'Choose a shell', position: 'top' })} onclick={() => (shellOpen = !shellOpen)}>
            <ZIcon zType={'lucideChevronsUpDown' as ZIconName} class="size-4" />
          </ZButton>
        {/if}
        {#if terminalAvailable()}
          <ZButton bind:ref={findButton} zWave={false} zType="ghost" zSize="sm" class="{iconButton} {searching ? TRIGGER_ON : ''}" aria-label="Find" aria-expanded={searching} {@attach zTooltip({ content: 'Find (Ctrl+F)', position: 'top' })} onclick={() => (searching ? closeFind() : openFind())}>
            <ZIcon zType="lucideSearch" class="size-4" />
          </ZButton>
          <ZButton zWave={false} zType="ghost" zSize="sm" class={iconButton} aria-label="Attach the last command's output to the chat" {@attach zTooltip({ content: "Attach the last command's output to the chat", position: 'top' })} onclick={() => void attachLastOutput()}>
            <ZIcon zType={'lucidePaperclip' as ZIconName} class="size-4" />
          </ZButton>
          <ZButton zWave={false} zType="ghost" zSize="sm" class={iconButton} aria-label="Clear" {@attach zTooltip({ content: 'Clear', position: 'top' })} onclick={() => { clearSession(terminal.active); focusSession(terminal.active); }}>
            <ZIcon zType={'lucideBan' as ZIconName} class="size-4" />
          </ZButton>
        {/if}
      {/if}
      <ZButton zWave={false} zType="ghost" zSize="sm" class={iconButton} aria-label="Hide terminal" {@attach zTooltip({ content: 'Hide (Ctrl+`)', position: 'top' })} onclick={toggleTerminal}>
        <ZIcon zType="lucideX" class="size-4" />
      </ZButton>
    </div>

    {#if !terminalAvailable()}
      <div class="flex min-h-0 flex-1 items-center justify-center px-4 text-center text-xs text-muted-foreground">The terminal runs in the desktop app.</div>
    {:else}
      <div class="relative min-h-0 flex-1">
        {#each terminal.tabs as tab (tab.id)}
          <!-- Every tab keeps its box (a hidden one has no size), so its shell and scrollback stay. -->
          <div
            class="absolute inset-0 {tab.id === terminal.active ? '' : 'hidden'}"
            role="tabpanel"
            tabindex="-1"
            aria-label={tab.name}
            oncontextmenu={(event) => onContextMenu(event, tab.id)}
            {@attach (element) => mount(element, tab.id)}
          >
            <!-- The scroll arrows sit in the terminal's own top and bottom padding, at the right edge, in line with its scrollbar. -->
            <button type="button" tabindex="-1" aria-label="Scroll up" class="{arrow} top-0" disabled={ends[tab.id]?.top ?? true} onpointerdown={(event) => press(event, tab.id, -3)}>
              <svg viewBox="0 0 8 8" width="8" height="8" class="block" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M1.5 5.2 4 2.8 6.5 5.2" /></svg>
            </button>
            <button type="button" tabindex="-1" aria-label="Scroll down" class="{arrow} bottom-0" disabled={ends[tab.id]?.bottom ?? true} onpointerdown={(event) => press(event, tab.id, 3)}>
              <svg viewBox="0 0 8 8" width="8" height="8" class="block" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M1.5 2.8 4 5.2 6.5 2.8" /></svg>
            </button>
          </div>
        {/each}
      </div>
    {/if}
  </section>
{/if}

{#if menu}
  {#key menuKey}
    <Menu items={menuItems} bind:open={menuOpen} at={{ x: menu.x, y: menu.y }} position="bottom-left" width={220} />
  {/key}
{/if}

<PickerPopover anchor={shellAnchor} bind:open={shellOpen} items={shellItems} label="Shells" position="top-right" size="sm" width={200} onpick={pickShell} />

<!-- Find is a popover on its button: it opens with the popover's motion, over the terminal, and stays open while you
     work in the terminal (a press there does not close it; Esc or the X does). -->
<ZPopover anchor={findButton ?? toolsAnchor} bind:open={searching} zTrigger="manual" zPosition="bottom-right" zOffset={6} zWidth="auto" zCloseOnOutside={false} zScrollClose={false} zCloseOnEscape={false}>
  {#snippet children()}
    <div class="flex items-center gap-1 p-1" role="search">
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
  {/snippet}
</ZPopover>

<PickerPopover anchor={toolsAnchor} bind:open={toolsOpen} items={toolsItems} label="Terminal menu" position="bottom-right" size="md" width={240} onpick={pickTools} />
