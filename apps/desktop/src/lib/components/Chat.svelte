<script lang="ts">
  import { customProviderName, modelLabel } from '../models.svelte';
  import { tick, untrack } from 'svelte';
  import { ZButton, ZChatNavigator, ZDropdownMenu, ZGalleryPreview, ZIcon, ZLoading, ZNumberTicker, createVirtualizer, zTooltip } from '@shival99/z-ui-svelte';
  import { layout } from '../layout.svelte';
  import { showSession } from '../content-tabs.svelte';
  import { LOADER } from '../loaders';
  import { toast } from '../toast';
  import { setEagerColour, whenColoured } from '../markdown';
  import { copyMessage } from '../attachments';
  import { openSelectionMenu } from '../selection-menu.svelte';
  import FileChip from './FileChip.svelte';
  import { activeSession, AGENTS, answerPermission, answerQuestions, attachmentsOf, chat, editMessage, forkSession, isRunning, loadOlder, needsApproval, newSession, openTranscript, pendingProject, workspace } from '../sessions.svelte';
  import { ticker } from '../clock.svelte';
  import { theme } from '../theme.svelte';
  import { segments } from '../blocks';
  import { openAttachment, openImages, preview, releasePreview, revealPreviewed } from '../preview.svelte';
  import ChatBlock from './ChatBlock.svelte';
  import DelegateList from './DelegateList.svelte';
  import CommandGroup from './CommandGroup.svelte';
  import PlanCard from './PlanCard.svelte';
  import LiveStatus from './LiveStatus.svelte';
  import MessageFooter from './MessageFooter.svelte';
  import type { Segment } from '../blocks';
  import type { Message } from '../mock';

  // Editing one of your messages in place (ChatGPT-style): Send replaces it and what came after it.
  let editingId = $state<string | null>(null);
  let editText = $state('');
  function startEdit(message: Message) {
    editText = message.blocks.map((block) => (block.kind === 'response' ? block.text : '')).join('\n').trim();
    editingId = message.id;
  }
  function finishEdit(id: string) {
    if (!editText.trim()) return;
    editMessage(id, editText);
    window.dispatchEvent(new Event('agentdeck:scrolltoend'));
    editingId = null;
  }

  // "Copy all" on a message with attachments: its text, pictures, notes and files go on the clipboard together, and
  // pasting into the message box restores them all (see attachments.ts).
  async function copyAll(message: Message) {
    const { images, elements, files } = attachmentsOf(message);
    const text = message.blocks.map((block) => (block.kind === 'response' ? block.text : '')).join('\n').trim();
    try {
      await copyMessage({ v: 1, text, images, elements, files });
      toast.success('Copied with attachments');
    } catch {
      toast.error('Could not copy');
    }
  }

  // Whether the part before this one, skipping blocks that draw nothing (empty reasoning), is also a step line.
  function afterSteps(parts: Segment[], position: number) {
    for (let index = position - 1; index >= 0; index--) {
      const part = parts[index];
      if (part.kind === 'commands') return true;
      if (part.kind === 'plan') return false;
      if (part.block.kind === 'thinking' && !part.block.text.trim()) continue;
      return false;
    }
    return false;
  }
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { costParts, estimateCost, shortCount, totalTokens, usd } from '../usage';
  import PermissionCard from './PermissionCard.svelte';
  import Composer from './Composer.svelte';
  import ToolbarActions from './ToolbarActions.svelte';
  import FileView from './FileView.svelte';
  import { activateTab, closeAllTabs, closeOtherTabs, closeTab, contentTabs, openChatTab, saveTab, type ContentTab } from '../content-tabs.svelte';
  import type { ZDropdownMenuItem } from '@shival99/z-ui-svelte';

  // Right click on a tab: close it, the others, or all of them.
  let tabMenu = $state<{ x: number; y: number; id: string } | null>(null);
  let tabMenuOpen = $state(false);
  let tabMenuKey = $state(0);
  function openTabMenu(event: MouseEvent, id: string) {
    event.preventDefault();
    tabMenuKey++;
    tabMenu = { x: event.clientX, y: event.clientY, id };
    tabMenuOpen = true;
  }
  // Save shows only for a file tab with unsaved edits; it saves that tab, in front or not.
  const menuTab = $derived(tabMenu ? contentTabs.tabs.find((tab) => tab.id === tabMenu?.id) : undefined);
  const tabContextItems = $derived<ZDropdownMenuItem[]>([
    ...(menuTab?.kind === 'file' && menuTab.dirty
      ? [{ label: 'Save', icon: 'lucideSave', iconSize: '12', shortcut: 'Ctrl+S', divide: 'after', class: 'text-xs! py-1', onClick: () => tabMenu && void saveTab(tabMenu.id) } as ZDropdownMenuItem]
      : []),
    { label: 'Close tab', icon: 'lucideX', iconSize: '12', shortcut: 'Ctrl+W', class: 'text-xs! py-1', onClick: () => tabMenu && closeTab(tabMenu.id) },
    { label: 'Close other tabs', icon: 'lucideX', iconSize: '12', shortcut: 'Ctrl+K Ctrl+O', class: 'text-xs! py-1', onClick: () => tabMenu && closeOtherTabs(tabMenu.id) },
    { label: 'Close all tabs', icon: 'lucideListX', iconSize: '12', shortcut: 'Ctrl+K Ctrl+W', divide: 'before', class: 'text-xs! py-1', onClick: () => closeAllTabs() },
  ]);

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
      class: contentTabs.active === tab.id ? 'bg-primary/15! text-primary!' : undefined,
      onClick: () => activateTab(tab.id),
    })),
  );

  // The session on screen always has a chat tab: sidebar picks and new sessions open one. Closing a
  // tab does not change workspace.activeId, so the last tab can be closed without reopening.
  let lastSession = '';
  $effect(() => {
    const id = workspace.activeId;
    // An unsent new session gets its tab with its first message (see content-tabs), not now.
    if (!id || id === lastSession || pendingProject()) return;
    untrack(() => {
      lastSession = id;
      openChatTab(id);
    });
  });


  const agentText = { claude: 'text-orange-500', codex: 'text-emerald-500', antigravity: 'text-sky-500', custom: 'text-violet-500' } as const;
  const agentName = (agent: string) => AGENTS.find((item) => item.value === agent)?.label ?? agent;

  // ---------- virtual list (same approach as ZChat) ----------
  // Pinned to the bottom, follows new rows only when already there, so a reader scrolled into
  // history is never yanked down by an incoming reply.
  // Every loaded message is in the list: the virtualizer itself mounts only the rows in view. (A second window of
  // mounted rows on top of it changed the list under the virtualizer at every step: that was the stutter.) Near the
  // top, the page before is read from the chat's file and put in front.
  const from = 0;
  function revealOlder() {
    if (chat.older > 0 && !chat.loadingOlder) {
      void loadOlder(activeSession()).then((added) => {
        if (added > 0) toast.info(`Loaded ${added} earlier messages${chat.older > 0 ? '' : ' · start of the conversation'}`);
      });
    }
  }
  // Rows not measured yet are guessed at the average of those that were: a wrong guess makes the list re-measure and
  // re-position over several rounds, every one a scroll event the virtualizer has to handle.
  let averageRow = 96;
  const virtualOptions = () => ({
    count: chat.messages.length - from,
    getScrollElement: () => viewportEl ?? null,
    estimateSize: () => averageRow,
    getItemKey: (index: number) => chat.messages[index + from]?.id ?? index + from,
    anchorTo: 'end' as const,
    followOnAppend: true,
    scrollEndThreshold: 80,
    overscan: 4,
    paddingStart: 8,
    // Clears the fade above the composer (h-12) so the last message ends above it.
    paddingEnd: 56,
  });
  const virtualizer = createVirtualizer<HTMLDivElement, HTMLDivElement>(virtualOptions());

  $effect(() => {
    void viewportEl;
    const options = virtualOptions();
    untrack(() => $virtualizer.setOptions(options));
  });

  // A file tab covers the conversation while it is open. Remember whether the reader was at the end, and where
  // they were otherwise, and put them back when the conversation shows again.
  let parked: { at: number; end: boolean } | null = null;
  $effect(() => {
    const covered = fileActive;
    untrack(() => {
      if (!viewportEl) return;
      if (covered) {
        parked = { at: viewportEl.scrollTop, end: following };
        return;
      }
      if (!parked) return;
      const { at, end } = parked;
      parked = null;
      requestAnimationFrame(() => {
        if (end) $virtualizer.scrollToEnd();
        else if (viewportEl) viewportEl.scrollTop = at;
      });
    });
  });

  /** One scroll is not enough on a cold list: sizes move as rows are measured, so re-assert per frame. */
  let pinToken = 0;
  function pinToNewest(): Promise<void> {
    const token = ++pinToken;
    let attemptsLeft = 12;
    return new Promise((resolve) => {
      const step = () => {
        if (token !== pinToken || attemptsLeft-- <= 0 || !viewportEl || chat.messages.length === 0 || $virtualizer.isAtEnd(1)) return resolve();
        $virtualizer.scrollToEnd();
        requestAnimationFrame(step);
      };
      void tick().then(() => requestAnimationFrame(step));
    });
  }

  // A chat is shown only once it sits at its end, and at once: no fade (a fade over the skeleton looked like a ghost),
  // and not first at the top and then at the end (which looked like two flashes). Hidden, it is still laid out and
  // measured, so the pin to the end works while nobody sees it.
  let revealed = $state(true);
  async function revealAtEnd() {
    const target = restoreTarget;
    restoreTarget = null;
    const at = target ? chat.messages.findIndex((message) => message.id === target.id) : -1;
    if (target && at >= 0) {
      // Back from a child chat: the view goes to the message it was on, at the same place on screen, not to the end.
      stopSticking();
      const put = () => {
        $virtualizer.scrollToIndex(at, { align: 'start' });
        if (viewportEl) viewportEl.scrollTop = Math.max(0, viewportEl.scrollTop - target.offset);
      };
      await tick();
      for (let pass = 0; pass < 4; pass++) {
        put();
        await new Promise((resolve) => requestAnimationFrame(resolve));
      }
      await whenColoured(300);
      setEagerColour(false);
      put();
      revealed = true;
      return;
    }
    await pinToNewest();
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    // Code on screen is coloured before it is seen (a long wait is not worth it: 300ms at most).
    await whenColoured(300);
    setEagerColour(false);
    await pinToNewest();
    revealed = true;
  }
  // As soon as there is something to show (the saved copy, or the file once it is read), not when the file has been
  // read as well: the skeleton stays until then, so there is no blank in between, and the swap from the saved copy to
  // the fresh one keeps the same rows (they are keyed by message), so it does not flash either.
  $effect(() => {
    if (!revealed && !chat.loading) untrack(() => void revealAtEnd());
  });

  $effect(() => {
    if (viewportEl) untrack(pinToNewest);
  });

  // Just after a chat opens (or a message is sent) the list is still settling: rows get their real height as they are
  // measured, code is coloured, images load. For a moment the view stays glued to the end whatever changes the height,
  // until the reader scrolls themselves. Without it the view is left somewhere in the middle of the chat.
  let stickUntil = 0;
  const stickToEnd = (ms = 3000) => (stickUntil = performance.now() + ms);
  const stopSticking = () => (stickUntil = 0);
  function watchListHeight(node: HTMLElement) {
    const observer = new ResizeObserver(() => {
      if (performance.now() < stickUntil && viewportEl) viewportEl.scrollTop = viewportEl.scrollHeight;
    });
    observer.observe(node);
    return () => observer.disconnect();
  }

  // Switching session loads its transcript (from disk for imported Claude/Codex history).
  // Re-runs when the open chat changes, and when it first appears in the list (after F5 the id is restored
  // before the history index is in).
  // Gaining its file after the first answer must not reload the chat: the live messages are already right.
  // A chat whose tab was closed and is opened again starts over too: at its end, with only its newest rows.
  const tabOpen = $derived(contentTabs.tabs.some((tab) => tab.kind === 'chat' && tab.sessionId === workspace.activeId));
  const openKey = $derived(`${workspace.activeId}|${workspace.sessions.some((item) => item.id === workspace.activeId) ? 'found' : 'missing'}|${tabOpen ? 'tab' : 'no tab'}`);
  // Where each chat was being read when it was left (the message at the top of the view, and how far from the top of
  // the view it sat). Used when coming back from a child chat with the Back button, which must not jump to the end.
  const readingAt = new Map<string, { id: string; offset: number }>();
  let shownSession = '';
  let restoreFor = '';
  let restoreTarget: { id: string; offset: number } | null = null;
  function rememberReading(sessionId: string) {
    if (!sessionId || !viewportEl || chat.messages.length === 0) return;
    const top = viewportEl.getBoundingClientRect().top;
    const first = [...viewportEl.querySelectorAll<HTMLElement>('[role="log"] > div[data-index]')].find((row) => row.getBoundingClientRect().bottom > top + 1);
    const message = first ? chat.messages[Number(first.dataset.index)] : undefined;
    if (first && message) readingAt.set(sessionId, { id: message.id, offset: Math.round(first.getBoundingClientRect().top - top) });
  }
  function backToParent(parentId: string) {
    restoreFor = parentId;
    showSession(parentId);
  }
  $effect(() => {
    void openKey;
    untrack(() => {
      const id = workspace.activeId;
      if (shownSession && shownSession !== id) rememberReading(shownSession);
      shownSession = id;
      restoreTarget = restoreFor === id ? (readingAt.get(id) ?? null) : null;
      restoreFor = '';
      revealed = false;
      setEagerColour(true);
      if (!restoreTarget) stickToEnd(6000);
      else stopSticking();
      void openTranscript(workspace.sessions.find((item) => item.id === id)).then(() => {
        if (!restoreTarget) stickToEnd();
      });
    });
  });

  function measureRow(node: HTMLDivElement) {
    $virtualizer.measureElement(node);
    const height = node.offsetHeight;
    // Only while the chat is still hidden: a guess that changes while the reader scrolls moves every row not measured
    // yet, the ones on screen included.
    if (height > 0 && !revealed) averageRow = Math.round(averageRow * 0.8 + height * 0.2);
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
    const first = ($virtualizer.range?.startIndex ?? 0) + from;
    return Math.max(0, turnRows.filter((row) => row <= first).length - 1);
  });

  // Jump-to-bottom button: shown while scrolled up or while the agent runs. Idle = arrow; running =
  // a loading ring that turns into the arrow on hover.
  // The button only appears once the reader is clearly away from the bottom, not at the first pixel.
  const JUMP_AFTER = 180;
  const pendingApproval = $derived.by(() => {
    for (const message of chat.messages.slice(-3).reverse()) {
      const index = message.blocks.findIndex((block) => block.kind === 'permission' && block.state === 'pending');
      const block = message.blocks[index];
      if (block?.kind === 'permission') return { block, messageId: message.id, index, agent: message.agent ?? activeSession()?.source ?? activeSession()?.agent };
    }
    return null;
  });
  /** Flow steps (chats this one handed work to) stopped on an approval card: they wait for a yes in their own chat. */
  const stepApprovals = $derived.by(() => {
    void ticker.sec;
    const found: { childId: string; step: string; agent: string }[] = [];
    for (const message of chat.messages.slice(-12)) {
      for (const block of message.blocks) {
        if (block.kind !== 'delegate' || found.some((item) => item.childId === block.childId) || !needsApproval(block.childId)) continue;
        found.push({ childId: block.childId, step: block.step ?? 'A step', agent: AGENTS.find((item) => item.value === block.agent)?.label ?? 'The agent' });
      }
    }
    return found;
  });
  const atEnd = $derived($virtualizer.isAtEnd(JUMP_AFTER));
  const running = $derived(activeSession()?.status === 'working');
  /** The chat the open one is a sub chat of, if it is one. */
  const parentOfActive = $derived.by(() => {
    const parentId = activeSession()?.parentId;
    return parentId ? workspace.sessions.find((item) => item.id === parentId) : undefined;
  });

  // While a reply streams the view follows it, but only for a reader who is already at the bottom:
  // scrolling up to read stops the following, sending a message (or the jump button) resumes it.
  let following = true;
  function onViewportScroll() {
    if (!viewportEl) return;
    following = viewportEl.scrollHeight - viewportEl.scrollTop - viewportEl.clientHeight < 120;
    if (viewportEl.scrollTop < 1500) revealOlder();
  }
  // Follows what is on screen, not what has arrived: the answer is revealed word by word, so the list grows a little
  // every frame. Each time its measured height changes the view is put at the bottom right away (same frame, no
  // animation frame in between), so the text glides up instead of the view catching up in jumps.
  const listHeight = $derived($virtualizer.getTotalSize());
  $effect(() => {
    void listHeight;
    if (!following || !running || !viewportEl) return;
    const view = viewportEl;
    untrack(() => {
      view.scrollTop = view.scrollHeight;
    });
  });
  // Your own message always brings you back to the bottom: only when one is added at the end. The list is also
  // replaced when older messages are loaded above, and a chat whose last message is yours would then jump to the end.
  // Only for a message sent by the person (the composer announces it with `agentdeck:scrolltoend`): a message that
  // goes out by itself from the queue must not pull a reader away from what they scrolled up to.
  let lastMessageId: string | undefined;
  let userSentUntil = 0;
  function toEnd() {
    following = true;
    stickToEnd(1500);
    void pinToNewest();
  }
  $effect(() => {
    const onSent = () => {
      userSentUntil = performance.now() + 800;
      toEnd();
    };
    window.addEventListener('agentdeck:scrolltoend', onSent);
    return () => window.removeEventListener('agentdeck:scrolltoend', onSent);
  });
  $effect(() => {
    const last = chat.messages.at(-1);
    const added = last?.id !== lastMessageId;
    lastMessageId = last?.id;
    if (added && revealed && last?.role === 'user' && performance.now() < userSentUntil) untrack(toEnd);
  });

  async function jumpToTurn(turn: number) {
    const row = turnRows[turn];
    if (row === undefined) return;
    // A turn above the mounted window is mounted first.
    // Instant, not smooth: rows between here and the target are measured on the way, the target moves under a smooth
    // scroll and it stops short. Jump, then put it right again once the rows around it have their real height.
    const target = row - from;
    $virtualizer.scrollToIndex(target, { align: 'start' });
    for (let pass = 0; pass < 3; pass++) {
      await new Promise((resolve) => requestAnimationFrame(resolve));
      $virtualizer.scrollToIndex(target, { align: 'start' });
    }
  }
</script>

<main class="flex h-full min-h-0 min-w-0 flex-col overflow-hidden" aria-label="Conversation">
  <!-- With tools open, the window controls sit over the tools header, so this row only needs a small inset. -->
  <!-- Above the conversation body: its turn ruler (ZChatNavigator) is taller than the body and must not paint over the tabs. -->
  <header class="relative z-40 flex h-10 shrink-0 items-center border-b border-border bg-background {layout.toolsVisible ? 'pr-[7px]' : 'pr-[106px]'}" data-tauri-drag-region>
    <!-- Browser-style tabs: chats and files side by side, each with a close button; + starts a new chat.
         No scrollbar: when the tabs overflow, an arrow opens the full list. -->
    {#if layout.sidebarHidden || layout.sidebarCollapsed}
      <button
        type="button"
        class="ml-2 grid size-7 shrink-0 cursor-pointer place-items-center rounded-md text-muted-foreground hover:bg-primary/10 hover:text-primary"
        aria-label={layout.sidebarHidden ? 'Open sessions' : 'Show sidebar (Ctrl+B)'}
        title={layout.sidebarHidden ? 'Sessions' : 'Show sidebar (Ctrl+B)'}
        onclick={() => (layout.sidebarHidden ? (layout.sidebarDrawer = !layout.sidebarDrawer) : (layout.sidebarCollapsed = false))}
      >
        <ZIcon zType={layout.sidebarHidden ? 'lucideMenu' : 'lucidePanelLeft'} class="size-4" />
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
              data-tip-clip
              class="flex h-7 max-w-56 shrink-0 cursor-pointer items-center justify-between gap-1.5 rounded-md py-0 pr-2.5 pl-2.5 text-xs outline-none focus-visible:ring-0 {active ? 'bg-primary/15 text-primary' : 'text-muted-foreground hover:bg-primary/10 hover:text-primary'}"
              onclick={() => activateTab(tab.id)}
              oncontextmenu={(event) => openTabMenu(event, tab.id)}
              onkeydown={(event) => event.key === 'Enter' && activateTab(tab.id)}
            >
              <span class="truncate">{label}</span>
              {#if tab.kind === 'file' && tab.dirty}<span class="size-1.5 shrink-0 rounded-full bg-current" title="Unsaved changes" aria-label="Unsaved changes"></span>{/if}
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
        <div class="-mr-0.5 flex shrink-0 items-center bg-background [&_button]:size-7! [&_button]:p-0 [&_button]:!bg-transparent [&_button]:!text-muted-foreground [&_button:hover]:!bg-primary/10 [&_button:hover]:!text-primary [&_button[aria-expanded=true]]:!bg-primary/15 [&_button[aria-expanded=true]]:!text-primary [&_button]:outline-none [&_button]:focus-visible:ring-0">
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
            newSession();
            openChatTab(workspace.activeId);
            requestAnimationFrame(() => document.querySelector<HTMLTextAreaElement>('main[aria-label="Conversation"] form textarea')?.focus());
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

  {#if tabMenu}
    {#key tabMenuKey}
      <div class="pointer-events-none fixed z-50 size-px" style:left="{tabMenu.x}px" style:top="{tabMenu.y}px">
        <ZDropdownMenu zItems={tabContextItems} bind:open={tabMenuOpen} zPosition="bottom-left" zOffset={4} zMinWidth={180} zContentClass="p-1 text-xs">
          <span class="block size-px"></span>
        </ZDropdownMenu>
      </div>
    {/key}
  {/if}

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
      {#if contentTabs.tabs.length === 0 && !pendingProject()}
        <div class="absolute inset-0 z-30 flex flex-col items-center justify-center gap-3 bg-background p-6 text-center text-muted-foreground">
          <ZIcon zType={'lucideMessagesSquare' as ZIconName} zSize="28" class="size-7 opacity-60" />
          <p class="m-0 text-xs">No open tabs. Start a chat from the sidebar.</p>
        </div>
      {/if}
    {/if}
    {#if contentTabs.active.startsWith('chat:') && parentOfActive}
      <!-- A sub chat (a step of a flow, a delegated task, a fork): one click back to the chat it came from. -->
      <div class="relative z-30 flex h-8 shrink-0 items-center gap-2 border-b border-border bg-background px-3 text-xs">
        <button type="button" class="inline-flex h-6 min-w-0 cursor-pointer items-center gap-1.5 rounded-md px-1.5 text-muted-foreground outline-none hover:bg-primary/10 hover:text-primary" aria-label="Back to {parentOfActive.title}" onclick={() => backToParent(parentOfActive.id)}>
          <ZIcon zType={'lucideArrowLeft' as ZIconName} class="size-3.5 shrink-0" />
          <span class="shrink-0">Back to</span>
          <span class="truncate font-medium text-foreground">{parentOfActive.title}</span>
        </button>
      </div>
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

    {#if chat.loadingOlder}
      <div class="pointer-events-none absolute top-3 left-1/2 z-20 flex -translate-x-1/2 items-center gap-2 rounded-full border border-border bg-background/90 px-3 py-1 text-xs text-muted-foreground shadow-sm backdrop-blur" role="status">
        <span class="size-3 animate-spin rounded-full border-2 border-muted-foreground/40 border-t-transparent"></span>
        Loading earlier messages…
      </div>
    {/if}

    {#if chat.messages.length === 0 || !revealed}
      <div class="pointer-events-none absolute inset-0 z-10 grid place-items-center text-xs text-muted-foreground">
        {#if chat.loading || (!revealed && chat.messages.length > 0)}
          <div class="flex w-full max-w-3xl animate-pulse flex-col gap-6 px-6" role="status" aria-label="Loading conversation">
            {#each [60, 90, 75, 40, 85] as width, index}
              <div class="flex flex-col gap-2 {index % 2 === 0 ? 'items-end' : ''}">
                <div class="h-3 rounded bg-muted" style:width="{width}%"></div>
                <div class="h-3 rounded bg-muted" style:width="{Math.max(25, width - 30)}%"></div>
              </div>
            {/each}
          </div>
        {:else if chat.error}
          <span class="max-w-sm text-center text-destructive">{chat.error}</span>
        {:else}
          <div class="flex flex-col items-center gap-3">
            <ZIcon zType={'lucideMessagesSquare' as ZIconName} zSize="28" class="size-7 opacity-60" />
            <p class="m-0 text-xs">No messages yet. Say something to start.</p>
          </div>
        {/if}
      </div>
    {/if}

    <!-- svelte-ignore a11y_no_static_element_interactions (the events only tell that the reader took over the scrolling) -->
    <div bind:this={viewportEl} onscroll={onViewportScroll} onwheel={stopSticking} ontouchmove={stopSticking} onpointerdown={stopSticking} onkeydown={stopSticking} class="min-h-0 flex-1 overflow-y-auto overflow-x-hidden pl-2.5 [scrollbar-gutter:stable] {chat.loading || !revealed ? 'invisible' : ''}">
      <div class="relative w-full" style:height="{$virtualizer.getTotalSize()}px" {@attach watchListHeight} role="log" aria-live="polite" aria-label="Conversation">
        {#each $virtualizer.getVirtualItems() as row (row.key)}
          {@const message = chat.messages[row.index + from]}
          {@const mine = message?.role === 'user'}
          {@const agent = message?.agent ?? activeSession()?.source ?? activeSession()?.agent ?? 'claude'}
          {#if message}
            <div class="absolute top-0 left-0 w-full" data-index={row.index} style:transform="translateY({row.start}px)" {@attach measureRow}>
              <!-- No avatars: your messages are a bubble (right side, or left in one-sided mode); the agent is just its coloured name. -->
              <article oncontextmenu={(event) => openSelectionMenu(event, message.blocks.map((block) => (block.kind === 'response' ? block.text : '')).filter(Boolean).join('\n\n'))} class="group/msg mx-auto flex w-full min-w-0 max-w-3xl flex-col break-words py-2.5 px-3 min-[720px]:pr-6 min-[720px]:pl-12 {mine && theme.chatLayout === 'split' ? 'items-end' : 'items-start'}">
                {#if mine}
                  {@const pictures = message.blocks.flatMap((block) => (block.kind === 'image' ? [block] : []))}
                  {@const words = message.blocks.filter((block) => block.kind !== 'image' && block.kind !== 'delegate' && block.kind !== 'file')}
                  {@const documents = message.blocks.flatMap((block) => (block.kind === 'file' ? [block] : []))}
                  {@const handed = message.blocks.filter((block) => block.kind === 'delegate')}
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
                  {#if documents.length > 0}
                    <div class="mb-1.5 flex max-w-[85%] flex-wrap gap-1.5 {theme.chatLayout === 'split' ? 'justify-end' : ''}">
                      {#each documents as file}
                        <FileChip name={file.name} size={file.size} onclick={() => void openAttachment(file.path, file.name, file.size)} />
                      {/each}
                    </div>
                  {/if}
                  {#if editingId === message.id}
                    <!-- Editing: the message becomes a box; Send replaces it and everything after it. -->
                    <div class="flex w-full max-w-[85%] flex-col gap-2 rounded-2xl bg-muted px-4 pt-3 pb-2.5 text-sm">
                      {#each message.blocks.filter((block) => block.kind === 'element') as attached, at (at)}
                        <ChatBlock block={attached} />
                      {/each}
                      <textarea
                        bind:value={editText}
                        {@attach (node: HTMLTextAreaElement) => {
                          node.focus();
                          node.setSelectionRange(node.value.length, node.value.length);
                          node.style.height = `${Math.min(node.scrollHeight, 240)}px`;
                        }}
                        oninput={(event) => {
                          const node = event.currentTarget;
                          node.style.height = 'auto';
                          node.style.height = `${Math.min(node.scrollHeight, 240)}px`;
                        }}
                        onkeydown={(event) => {
                          if (event.key === 'Escape') editingId = null;
                          if (event.key === 'Enter' && !event.shiftKey && !event.isComposing) {
                            event.preventDefault();
                            finishEdit(message.id);
                          }
                        }}
                        aria-label="Edit message"
                        rows="1"
                        class="w-full resize-none border-0 bg-transparent p-0 text-sm leading-relaxed outline-none"
                      ></textarea>
                      <div class="flex justify-end gap-2">
                        <ZButton zWave={false} zType="outline" zSize="sm" class="h-7 px-3 text-xs" onclick={() => (editingId = null)}>Cancel</ZButton>
                        <ZButton zWave={false} zSize="sm" class="h-7 px-3 text-xs" disabled={!editText.trim()} onclick={() => finishEdit(message.id)}>Send</ZButton>
                      </div>
                    </div>
                  {:else}
                    {#if words.length > 0}
                      <div class="flex max-w-[85%] flex-col gap-2 rounded-2xl bg-muted px-4 py-2.5 text-sm {theme.chatLayout === 'split' ? 'rounded-br-md' : 'rounded-bl-md'}">
                        {#each words as block}
                          <ChatBlock {block} fade="var(--muted)" />
                        {/each}
                      </div>
                    {/if}
                    {#if handed.length > 0}<div class="mt-3 w-full"><DelegateList blocks={handed} /></div>{/if}
                    <MessageFooter {message} end={theme.chatLayout === 'split'} onfork={() => forkSession(workspace.activeId)} onedit={() => startEdit(message)} oncopyall={pictures.length + documents.length + message.blocks.filter((block) => block.kind === 'element').length > 0 ? () => copyAll(message) : undefined} />
                  {/if}
                {:else}
                  {@const parts = segments(message.blocks)}
                  {@const tail = parts.at(-1)}
                  {@const working = tail?.kind === 'commands' && tail.items.some((item) => item.status === 'running')}
                  <div class="mb-1.5 flex flex-wrap items-baseline gap-x-1.5 gap-y-0.5 text-xs">
                    <span class="font-semibold {agentText[agent]}">{(agent === 'custom' && customProviderName(message.model)) || agentName(agent)}</span>
                    {#if message.model}
                      <span class="text-muted-foreground/40" aria-hidden="true">/</span>
                      <span class="text-muted-foreground" title={message.model}>{modelLabel(message.model)}</span>
                    {/if}
                    <span class="text-muted-foreground/40" aria-hidden="true">·</span>
                    <span class="text-muted-foreground/80 tabular-nums">{message.time}</span>
                    {#if message.tokens}
                      {@const cost = message.cost ?? estimateCost(message.tokens, message.model)}
                      {@const tokens = message.tokens}
                      {@const parts = costParts(tokens, message.model)}
                      {#snippet usageTip()}
                        <div class="flex min-w-44 flex-col gap-1 py-0.5 text-[11px] tabular-nums">
                          {#each [
                            { label: 'Input', count: tokens.input, cost: parts?.input, icon: 'lucideArrowDownToLine', tone: 'text-sky-500' },
                            { label: 'Cache read', count: tokens.cacheRead, cost: parts?.cacheRead, icon: 'lucideDatabase', tone: 'text-violet-500' },
                            { label: 'Cache write', count: tokens.cacheWrite, cost: parts?.cacheWrite, icon: 'lucideDatabaseZap', tone: 'text-amber-500' },
                            { label: 'Output', count: tokens.output, cost: parts?.output, icon: 'lucideArrowUpFromLine', tone: 'text-emerald-500' },
                          ] as row (row.label)}
                            <div class="flex items-center gap-2">
                              <ZIcon zType={row.icon as ZIconName} zSize="12" class="size-3! shrink-0 {row.tone}" />
                              <span class="flex-1 text-muted-foreground">{row.label}</span>
                              <span class="font-medium text-foreground">{shortCount(row.count)}</span>
                              {#if row.cost != null}<span class="w-12 text-right text-muted-foreground">{usd(row.cost)}</span>{/if}
                            </div>
                          {/each}
                          {#if cost != null}
                            <div class="-mx-[0.6rem] mt-0.5 flex items-center gap-2 border-t border-border px-[0.6rem] pt-1.5">
                              <ZIcon zType={'lucideCoins' as ZIconName} zSize="12" class="size-3! shrink-0 text-success" />
                              <span class="flex-1 text-muted-foreground">Total</span>
                              <span class="font-semibold text-success">{message.cost == null ? '≈ ' : ''}{usd(cost)}</span>
                            </div>
                          {/if}
                        </div>
                      {/snippet}
                      <!-- Tokens and cost, quiet and on the right; hover for the breakdown. -->
                      <span class="ml-auto flex items-baseline gap-1.5 text-[11px] tabular-nums" {@attach zTooltip({ content: usageTip, position: 'top' })}>
                        <span class="text-muted-foreground"><ZNumberTicker zValue={totalTokens(message.tokens)} zFormat="compact" zDuration={600} /> tokens</span>
                        {#if cost != null}
                          <span class="text-muted-foreground/40" aria-hidden="true">·</span>
                          <span class="font-medium text-success">{message.cost == null ? '≈ ' : ''}<ZNumberTicker zValue={cost} zFormat="currency" zCurrency="USD" zLocale="en-US" zMinimumFractionDigits={2} zMaximumFractionDigits={cost < 0.01 ? 3 : 2} zDuration={600} /></span>
                        {/if}
                      </span>
                    {/if}
                  </div>
                  <div class="flex w-full min-w-0 flex-col gap-2">
                    {#each parts as segment, position (position)}
                      {#if segment.kind === 'commands'}
                        <!-- The newest group of a running reply is the live line; it carries the only loader. -->
                        <!-- Step lines in a row sit close together, like one list. -->
                        <div class="{afterSteps(parts, position) ? '-mt-1.5' : ''} {parts[position + 1]?.kind === 'block' ? '-mb-0.5' : ''}">
                          <CommandGroup items={segment.items} thoughts={segment.thoughts} live={false} startedAt={message.startedAt} />
                        </div>
                      {:else if segment.kind === 'plan'}
                        <PlanCard items={segment.items} />
                      {:else}
                        <ChatBlock block={segment.block} agent={message.agent ?? activeSession()?.source ?? activeSession()?.agent} onanswer={(answer) => answerPermission(message.id, message.blocks.indexOf(segment.block), answer)} onquestions={(text) => answerQuestions(workspace.activeId, text)} answered={chat.messages.at(-1)?.id !== message.id} />
                      {/if}
                    {/each}
                    <!-- One status line for the whole reply: what the agent is doing now (thinking, running a command,
                         editing…), as in Claude and Codex; the steps above stay plain, finished lines. -->
</div>
                  <!-- The status line and the footer share one fixed-height slot, so the row does not jump when the reply ends. -->
                  <!-- Live only while the chat really runs: a reply left marked live (a lost "done") shows its footer. -->
                  {@const showLive = message.live && isRunning(workspace.activeId)}
                  {#if showLive || message.blocks.length > 0}
                    <div class="relative h-[30px] w-full shrink-0">
                      {#if showLive}
                        <div class="absolute inset-x-0 bottom-0 flex h-5 items-center"><LiveStatus {message} /></div>
                      {:else}
                        <MessageFooter {message} onfork={() => forkSession(workspace.activeId)} />
                      {/if}
                    </div>
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
    <div class="relative z-30 shrink-0 bg-background px-2 pt-4 pb-2 min-[720px]:px-6">
      <div class="mx-auto w-full min-w-0 max-w-3xl px-0 min-[720px]:min-w-[440px] min-[720px]:pr-6 min-[720px]:pl-12">
        <!-- A pending approval sits right above the input, where the eye already is. -->
        {#if pendingApproval}
          <div class="mb-2">
            <PermissionCard block={pendingApproval.block} agent={pendingApproval.agent} onanswer={(answer) => answerPermission(pendingApproval.messageId, pendingApproval.index, answer)} />
          </div>
        {/if}
        <!-- A flow step that needs approval waits in its own chat: say which, and take the user there. -->
        {#each stepApprovals as item (item.childId)}
          <div class="mb-2 flex items-center gap-2 rounded-xl border border-warning/50 bg-warning/5 px-3 py-2 text-xs" role="status">
            <ZIcon zType="lucideShieldAlert" class="size-3.5 shrink-0 text-warning" />
            <span class="min-w-0 flex-1 truncate"><span class="font-medium">{item.step}</span> ({item.agent}) needs your approval to continue</span>
            <ZButton zWave={false} zType="default" zSize="sm" class="h-7 shrink-0 px-3 text-xs" onclick={() => showSession(item.childId)}>Open step</ZButton>
          </div>
        {/each}
        <Composer />
      </div>
    </div>
  {/if}
</main>

<ZGalleryPreview bind:file={preview.file} files={preview.files} onClose={releasePreview} onDownload={revealPreviewed} />
