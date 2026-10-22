<script lang="ts">
  import { tick, untrack } from 'svelte';
  import { ZButton, ZChatNavigator, ZDropdownMenu, ZGalleryPreview, ZIcon, ZLoading, createVirtualizer } from '@shival99/z-ui-svelte';
  import { layout } from '../layout.svelte';
  import { LOADER } from '../loaders';
  import { activeSession, AGENTS, answerPermission, chat, forkSession, newSession, openTranscript, workspace } from '../sessions.svelte';
  import { theme } from '../theme.svelte';
  import { segments } from '../blocks';
  import { openImages, preview } from '../preview.svelte';
  import ChatBlock from './ChatBlock.svelte';
  import CommandGroup from './CommandGroup.svelte';
  import LiveStatus from './LiveStatus.svelte';
  import MessageFooter from './MessageFooter.svelte';
  import Composer from './Composer.svelte';
  import ToolbarActions from './ToolbarActions.svelte';
  import FileView from './FileView.svelte';
  import { activateTab, closeTab, contentTabs, openChatTab, type ContentTab } from '../content-tabs.svelte';

  let viewportEl: HTMLDivElement | undefined = $state();

  // Tab strip: label per tab, and the dropdown that lists every tab once they overflow the strip.
  const tabLabel = (tab: ContentTab) =>
    tab.kind === 'chat' ? (workspace.sessions.find((item) => item.id === tab.sessionId)?.title ?? 'New session') : tab.name;
  let tabStrip: HTMLDivElement | undefined = $state();
  let tabStripWidth = $state(0);
  let tabsOverflow = $state(false);
  $effect(() => {
    void contentTabs.tabs.length;
    void tabStripWidth;
    tabsOverflow = contentTabs.tabs.length > 1 && !!tabStrip && tabStrip.scrollWidth > tabStrip.clientWidth + 1;
  });
  const fileActive = $derived(contentTabs.tabs.find((tab) => tab.id === contentTabs.active)?.kind === 'file');
  const tabMenuItems = $derived(
    contentTabs.tabs.map((tab) => ({
      label: tabLabel(tab),
      checked: contentTabs.active === tab.id,
      onClick: () => activateTab(tab.id),
    })),
  );

  // The session on screen always has a chat tab: sidebar picks and new sessions open one. Closing a
  // tab does not change workspace.activeId, so the last tab can be closed without reopening.
  let lastSession = '';
  $effect(() => {
    const id = workspace.activeId;
    if (!id || id === lastSession) return;
    untrack(() => {
      lastSession = id;
      openChatTab(id);
    });
  });


  const agentText = { claude: 'text-orange-500', codex: 'text-emerald-500', gemini: 'text-sky-500' } as const;
  const agentName = (agent: string) => AGENTS.find((item) => item.value === agent)?.label ?? agent;

  // ---------- virtual list (same approach as ZChat) ----------
  // Pinned to the bottom, follows new rows only when already there, so a reader scrolled into
  // history is never yanked down by an incoming reply.
  const virtualOptions = () => ({
    count: chat.messages.length,
    getScrollElement: () => viewportEl ?? null,
    estimateSize: () => 96,
    getItemKey: (index: number) => chat.messages[index]?.id ?? index,
    anchorTo: 'end' as const,
    followOnAppend: true,
    scrollEndThreshold: 80,
    overscan: 8,
    paddingStart: 8,
    paddingEnd: 24,
  });
  const virtualizer = createVirtualizer<HTMLDivElement, HTMLDivElement>(virtualOptions());

  $effect(() => {
    void viewportEl;
    const options = virtualOptions();
    untrack(() => $virtualizer.setOptions(options));
  });

  /** One scroll is not enough on a cold list: sizes move as rows are measured, so re-assert per frame. */
  let pinToken = 0;
  function pinToNewest() {
    const token = ++pinToken;
    let attemptsLeft = 12;
    const step = () => {
      if (token !== pinToken || attemptsLeft-- <= 0 || !viewportEl || chat.messages.length === 0) return;
      if ($virtualizer.isAtEnd(1)) return;
      $virtualizer.scrollToEnd();
      requestAnimationFrame(step);
    };
    void tick().then(() => requestAnimationFrame(step));
  }

  $effect(() => {
    if (viewportEl) untrack(pinToNewest);
  });

  // Switching session loads its transcript (from disk for imported Claude/Codex history).
  // Re-runs when the open chat changes, and when it first appears in the list (after F5 the id is restored
  // before the history index is in).
  // Gaining its file after the first answer must not reload the chat: the live messages are already right.
  const openKey = $derived(`${workspace.activeId}|${workspace.sessions.some((item) => item.id === workspace.activeId) ? 'found' : 'missing'}`);
  $effect(() => {
    void openKey;
    untrack(() => {
      void openTranscript(workspace.sessions.find((item) => item.id === workspace.activeId)).then(pinToNewest);
    });
  });

  function measureRow(node: HTMLDivElement) {
    $virtualizer.measureElement(node);
  }

  // ---------- navigator: one tick per user question ----------
  const turnRows = $derived(chat.messages.flatMap((item, index) => (item.role === 'user' ? [index] : [])));
  const navigatorTurns = $derived(
    turnRows.map((index) => {
      const answer = chat.messages[index + 1];
      const firstText = (blocks: typeof chat.messages[number]['blocks']) =>
        blocks.find((block) => block.kind === 'response');
      const question = firstText(chat.messages[index].blocks);
      const reply = answer?.role === 'assistant' ? firstText(answer.blocks) : undefined;
      return {
        question: question?.kind === 'response' ? question.text : '',
        answer: reply?.kind === 'response' ? reply.text : undefined,
      };
    }),
  );

  /** The turn being read: the last question at or above the first visible row (the last one at the end). */
  const activeTurn = $derived.by(() => {
    if (turnRows.length === 0) return 0;
    if ($virtualizer.isAtEnd(1)) return turnRows.length - 1;
    const first = $virtualizer.range?.startIndex ?? 0;
    return Math.max(0, turnRows.filter((row) => row <= first).length - 1);
  });

  // Jump-to-bottom button: shown while scrolled up or while the agent runs. Idle = arrow; running =
  // a loading ring that turns into the arrow on hover.
  // The button only appears once the reader is clearly away from the bottom, not at the first pixel.
  const JUMP_AFTER = 180;
  const atEnd = $derived($virtualizer.isAtEnd(JUMP_AFTER));
  const running = $derived(activeSession()?.status === 'working');

  // While a reply streams the view follows it, but only for a reader who is already at the bottom:
  // scrolling up to read stops the following, sending a message (or the jump button) resumes it.
  let following = true;
  function onViewportScroll() {
    if (viewportEl) following = viewportEl.scrollHeight - viewportEl.scrollTop - viewportEl.clientHeight < 120;
  }
  const tailSignature = $derived(
    (chat.messages.at(-1)?.blocks ?? []).map((block) => block.kind + ('text' in block ? block.text.length : 'output' in block ? block.output.length : 0)).join(),
  );
  $effect(() => {
    void tailSignature;
    if (!following || !running) return;
    untrack(() => requestAnimationFrame(() => $virtualizer.scrollToEnd()));
  });
  // Your own message always brings you back to the bottom.
  $effect(() => {
    if (chat.messages.at(-1)?.role === 'user') untrack(() => ((following = true), pinToNewest()));
  });

  function jumpToTurn(turn: number) {
    const row = turnRows[turn];
    if (row !== undefined) $virtualizer.scrollToIndex(row, { align: 'start', behavior: 'smooth' });
  }
</script>

<main class="flex h-full min-h-0 min-w-0 flex-col overflow-hidden" aria-label="Conversation">
  <!-- With tools open, the window controls sit over the tools header, so this row only needs a small inset. -->
  <!-- Above the conversation body: its turn ruler (ZChatNavigator) is taller than the body and must not paint over the tabs. -->
  <header class="relative z-40 flex h-10 shrink-0 items-center border-b border-border bg-background {layout.toolsVisible ? 'pr-3' : 'pr-[106px]'}" data-tauri-drag-region>
    <!-- Browser-style tabs: chats and files side by side, each with a close button; + starts a new chat.
         No scrollbar: when the tabs overflow, an arrow opens the full list. -->
    {#if layout.sidebarHidden}
      <button
        type="button"
        class="ml-2 grid size-7 shrink-0 cursor-pointer place-items-center rounded-md text-muted-foreground hover:bg-primary/10 hover:text-primary"
        aria-label="Open sessions"
        title="Sessions"
        onclick={() => (layout.sidebarDrawer = !layout.sidebarDrawer)}
      >
        <ZIcon zType="lucideMenu" class="size-4" />
      </button>
    {/if}
    <div class="flex min-w-0 flex-1 items-center gap-1 pl-2">
      <div class="relative flex min-w-0 shrink items-center">
        <div bind:this={tabStrip} bind:clientWidth={tabStripWidth} class="flex min-w-0 flex-1 items-center gap-1 overflow-hidden" role="tablist" aria-label="Open tabs" data-tauri-drag-region>
          {#each contentTabs.tabs as tab (tab.id)}
            {@const active = contentTabs.active === tab.id}
            {@const label = tabLabel(tab)}
            <div
              role="tab"
              tabindex="0"
              aria-selected={active}
              title={tab.kind === 'file' ? tab.path : label}
              class="flex h-7 min-w-[56px] max-w-56 shrink cursor-pointer min-[720px]:min-w-[96px] items-center gap-1 rounded-md py-0 pr-2.5 pl-2.5 text-xs outline-none focus-visible:ring-0 {active ? 'bg-muted font-medium text-foreground' : 'text-muted-foreground hover:bg-muted/60'}"
              onclick={() => activateTab(tab.id)}
              onkeydown={(event) => event.key === 'Enter' && activateTab(tab.id)}
            >
              <span class="truncate">{label}</span>
              <button type="button" class="grid size-4.5 shrink-0 cursor-pointer place-items-center rounded-full outline-none focus-visible:ring-0 hover:bg-primary/20" aria-label="Close {label}" onclick={(event) => { event.stopPropagation(); closeTab(tab.id); }}>
                <ZIcon zType="lucideX" zSize="12" class="size-3" />
              </button>
            </div>
          {/each}
        </div>
        {#if tabsOverflow}
          <!-- Fade into a solid backing: the strip continues under it, the arrow sits on solid background. -->
          <div class="pointer-events-none absolute inset-y-0 right-0 z-10 w-10 bg-gradient-to-r from-transparent to-background" aria-hidden="true"></div>
        {/if}
      </div>
      {#if tabsOverflow}
        <!-- Trigger styled like the other ghost icon buttons: same size, same hover colour. -->
        <div class="flex shrink-0 items-center bg-background pr-1 [&_button]:h-7 [&_button]:w-7 [&_button]:bg-transparent [&_button]:text-muted-foreground [&_button:hover]:bg-primary/10 [&_button:hover]:text-primary">
          <ZDropdownMenu
          zItems={tabMenuItems}
          zIcon="lucideChevronDown"
          zLabel=""
          zButtonType="ghost"
          zButtonSize="sm"
          zPosition="bottom-right"
          zOffset={6}
          zMinWidth={200}
          zWave={false}
        />
        </div>
      {/if}
      {#if contentTabs.tabs.some((tab) => tab.kind === 'chat')}
        <button
          type="button"
          class="grid size-7 shrink-0 cursor-pointer place-items-center rounded-md text-muted-foreground outline-none focus-visible:ring-0 hover:bg-primary/10 hover:text-primary"
          aria-label="New tab"
          title="New tab"
          onclick={() => {
            newSession(undefined, { reuseDraft: false });
            openChatTab(workspace.activeId);
          }}
        >
          <ZIcon zType="lucidePlus" class="size-4" />
        </button>
      {/if}
    </div>
    <div class="flex shrink-0 items-center">
      <ToolbarActions divider={false} />
    </div>
  </header>

  <!-- overflow-hidden: the turn ruler can never paint past the conversation area, down onto the composer. -->
  <div class="relative isolate flex min-h-0 flex-1 flex-col overflow-hidden">
    {#if contentTabs.active.startsWith('chat:') === false}
      <!-- A file tab covers the conversation; no tabs at all shows an empty state. -->
      {#each contentTabs.tabs.filter((tab) => tab.id === contentTabs.active && tab.kind === 'file') as tab (tab.id)}
        {#if tab.kind === 'file'}
          <div class="absolute inset-0 z-30 bg-background">
            <FileView file={tab} />
          </div>
        {/if}
      {/each}
      {#if contentTabs.tabs.length === 0}
        <div class="absolute inset-0 z-30 flex flex-col items-center justify-center gap-3 bg-background p-6 text-center text-muted-foreground">
          <ZIcon zType="lucideMessageSquare" class="size-8 opacity-60" />
          <p class="m-0 text-xs">No open tabs. Start a chat from the sidebar.</p>
        </div>
      {/if}
    {/if}
    <!-- Keyed by session so the ruler never carries scroll/hover state over from the previous chat. -->
    {#key workspace.activeId}
      <!-- Full-height strip, ruler centred in it: it always sits in the conversation area, never over the
           header or the composer. Only the ruler itself takes pointer events. -->
      <div class="pointer-events-none absolute inset-y-0 left-1 z-20 flex items-center">
        <ZChatNavigator class="pointer-events-auto w-6" turns={navigatorTurns} activeIndex={activeTurn} onSelect={jumpToTurn} />
      </div>
    {/key}

    <!-- Only while scrolled up: at the bottom there is nothing to jump to, running or not. -->
    {#if !atEnd}
      <ZButton
        zWave={false}
        zType="outline"
        class="group/jump absolute bottom-3 left-1/2 z-20 size-8 -translate-x-1/2 rounded-full bg-card! p-0 shadow-md"
        aria-label="Scroll to latest"
        title="Scroll to latest"
        onclick={() => ((following = true), $virtualizer.scrollToEnd())}
      >
        <!-- Own grid so the spinner and the arrow sit dead centre; the button's inline content box drifted down-right. -->
        <span class="grid size-full place-items-center">
          {#if running}
            <span class="group-hover/jump:hidden grid place-items-center"><ZLoading zLoading zSpinner={LOADER.scroll} zSize="xs" zColor="primary" /></span>
            <ZIcon zType="lucideArrowDown" class="hidden size-4 group-hover/jump:block" />
          {:else}
            <ZIcon zType="lucideArrowDown" class="size-4" />
          {/if}
        </span>
      </ZButton>
    {/if}

    <!-- Same fade as ShowMore: messages dissolve into the page just above the composer instead of being cut. -->
    <div
      class="pointer-events-none absolute right-2.5 bottom-0 left-0 z-10 h-12"
      style="background-image: linear-gradient(to top, var(--background) 0%, transparent 100%)"
      aria-hidden="true"
    ></div>

    {#if chat.messages.length === 0}
      <div class="pointer-events-none absolute inset-0 z-10 grid place-items-center text-xs text-muted-foreground">
        {#if chat.loading}
          Loading conversation…
        {:else if chat.error}
          <span class="max-w-sm text-center text-destructive">{chat.error}</span>
        {:else}
          <div class="flex flex-col items-center gap-3">
            <ZIcon zType="lucideMessageSquare" class="size-8 opacity-60" />
            <p class="m-0 text-xs">No messages yet. Say something to start.</p>
          </div>
        {/if}
      </div>
    {/if}

    <div bind:this={viewportEl} onscroll={onViewportScroll} class="min-h-0 flex-1 overflow-y-auto overflow-x-hidden">
      <div class="relative w-full" style:height="{$virtualizer.getTotalSize()}px" role="log" aria-live="polite" aria-label="Conversation">
        {#each $virtualizer.getVirtualItems() as row (row.key)}
          {@const message = chat.messages[row.index]}
          {@const mine = message?.role === 'user'}
          {@const agent = activeSession()?.agent ?? 'claude'}
          {#if message}
            <div class="absolute top-0 left-0 w-full" data-index={row.index} style:transform="translateY({row.start}px)" {@attach measureRow}>
              <!-- No avatars: your messages are a bubble (right side, or left in one-sided mode); the agent is just its coloured name. -->
              <article class="mx-auto flex w-full min-w-0 max-w-3xl flex-col break-words py-2.5 px-3 min-[720px]:px-6 {mine && theme.chatLayout === 'split' ? 'items-end' : 'items-start'}">
                {#if mine}
                  {@const pictures = message.blocks.flatMap((block) => (block.kind === 'image' ? [block] : []))}
                  {@const words = message.blocks.filter((block) => block.kind !== 'image')}
                  <!-- Pictures sit apart from the bubble as small tiles; click one to see it large. -->
                  {#if pictures.length > 0}
                    <div class="mb-1.5 flex max-w-[85%] flex-wrap gap-1.5 {theme.chatLayout === 'split' ? 'justify-end' : ''}">
                      {#each pictures as picture, index}
                        <button
                          type="button"
                          class="size-[4.5rem] cursor-zoom-in overflow-hidden rounded-xl border border-border bg-muted outline-none transition hover:ring-2 hover:ring-primary/50 focus-visible:ring-2 focus-visible:ring-ring"
                          aria-label="Preview {picture.name}"
                          onclick={() => openImages(pictures, index)}
                        >
                          <img src={picture.src} alt={picture.name} class="size-full object-cover" />
                        </button>
                      {/each}
                    </div>
                  {/if}
                  {#if words.length > 0}
                    <div class="flex max-w-[85%] flex-col gap-2 rounded-2xl bg-muted px-4 py-2.5 text-sm {theme.chatLayout === 'split' ? 'rounded-br-md' : 'rounded-bl-md'}">
                      {#each words as block}
                        <ChatBlock {block} fade="var(--muted)" />
                      {/each}
                    </div>
                  {/if}
                  <span class="mt-1 px-1 text-[10px] text-muted-foreground">{message.time}</span>
                {:else}
                  {@const parts = segments(message.blocks)}
                  {@const tail = parts.at(-1)}
                  {@const working = tail?.kind === 'commands' && tail.items.some((item) => item.status === 'running')}
                  <div class="mb-1.5 flex items-baseline gap-2 text-xs">
                    <span class="font-semibold {agentText[agent]}">{agentName(agent)}</span>
                    <span class="text-muted-foreground">{message.time}</span>
                  </div>
                  <div class="flex w-full min-w-0 flex-col gap-3">
                    {#each parts as segment, position (position)}
                      {#if segment.kind === 'commands'}
                        <!-- The newest group of a running reply is the live line; it carries the only loader. -->
                        <CommandGroup items={segment.items} live={Boolean(message.live) && working && position === parts.length - 1} startedAt={message.startedAt} />
                      {:else}
                        <ChatBlock block={segment.block} onanswer={(answer) => answerPermission(message.id, message.blocks.indexOf(segment.block), answer)} />
                      {/if}
                    {/each}
                    {#if message.live && !working}
                      <LiveStatus {message} />
                    {/if}
                  </div>
                  {#if !message.live && message.blocks.length > 0}
                    <MessageFooter {message} onfork={() => forkSession(workspace.activeId)} />
                  {/if}
                {/if}
              </article>
            </div>
          {/if}
        {/each}
      </div>
    </div>
  </div>

  <!-- The composer sits on its own layer, above anything the conversation area paints at its edge. -->
  <!-- Hidden while a file tab is on screen: the file gets the full height. -->
  {#if !fileActive}
    <div class="relative z-30 shrink-0 bg-background px-2 pt-4 pb-4 min-[720px]:px-6">
      <div class="mx-auto w-full min-w-0 max-w-3xl px-0 min-[720px]:min-w-[440px] min-[720px]:px-6">
        <Composer />
        <p class="mt-2 hidden text-center text-[10px] text-muted-foreground min-[720px]:block">AgentDeck can make mistakes. Review important changes.</p>
      </div>
    </div>
  {/if}
</main>

<ZGalleryPreview bind:file={preview.file} files={preview.files} />
