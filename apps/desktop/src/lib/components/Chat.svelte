<script lang="ts">
  import Menu from './Menu.svelte';
  import { closeOnScroll } from '../close-on-scroll';
  import { revealTab } from '../tab-strip';
  import { modelLabel } from '../models.svelte';
  import { onMount, tick, untrack } from 'svelte';
  import { fade, fly } from 'svelte/transition';
  import { ZButton, ZChatNavigator, ZDropdownMenu, ZGalleryPreview, ZIcon, ZLoading, ZNumberTicker, ZShimmerText, createVirtualizer, zTooltip } from '@shival99/z-ui-svelte';
  import { layout, leavePeek, peekSidebar, pinSidebar } from '../layout.svelte';
  import { showSession } from '../content-tabs.svelte';
  import { LOADER } from '../loaders';
  import { rememberRowHeight, rowHeight } from '../row-size';
  import { toast } from '../toast';
  import { setEagerColour, whenColoured } from '../markdown';
  import { copyMessage } from '../attachments';
  import { openSelectionMenu } from '../selection-menu.svelte';
  import FileChip from './FileChip.svelte';
  import TabListButton from './TabListButton.svelte';
  import { openWithItem, openWithOptions, type OpenWithOption } from '../open-with';
  import { openInViewer } from '../file-viewer.svelte';
  import type { PickerItem } from './PickerPopover.svelte';
  import { activeSession, AGENTS, answerPermission, delegateTasks, answerQuestions, attachmentsOf, chat, editMessage, forkSession, isRunning, loadOlder, needsApproval, openTranscript, pendingProject, projectOf, releaseReplyRender, replyBlockRendered, resumes, workspace } from '../sessions.svelte';
  import { ticker } from '../clock.svelte';
  import { agentDisplayName } from '../provider-registry';
  import AgentLogo from './AgentLogo.svelte';
  import { theme } from '../theme.svelte';
  import { elapsed, segments } from '../blocks';
  import { openAttachment, openImages, preview, releasePreview, revealPreviewed } from '../preview.svelte';
  import ArtifactCards from './ArtifactCards.svelte';
  import ChatBlock from './ChatBlock.svelte';
  import DelegateList from './DelegateList.svelte';
  import CommandGroup from './CommandGroup.svelte';
  import PlanCard from './PlanCard.svelte';
  import TaskCards from './TaskCards.svelte';
  import LiveStatus from './LiveStatus.svelte';
  import MessageFooter from './MessageFooter.svelte';
  import ResumeCard from './ResumeCard.svelte';
  import type { Segment } from '../blocks';
  import type { Message } from '../mock';
  import { listen } from '../tauri-event';

  type ComputerApproval = { id: string; chat: string; tool: string; summary: string };
  let computerApprovals = $state<ComputerApproval[]>([]);
  onMount(() => {
    const openAndFocusNewChat = () => {
      const id = workspace.activeId;
      if (!id) return;
      openChatTab(id);
      void tick().then(() => requestAnimationFrame(() => document.querySelector<HTMLTextAreaElement>('textarea[aria-label="Message"]')?.focus()));
    };
    window.addEventListener('agentdeck:new-chat-requested', openAndFocusNewChat);
    return () => window.removeEventListener('agentdeck:new-chat-requested', openAndFocusNewChat);
  });
  $effect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen<ComputerApproval>('computer-use-approval', (event) => {
      computerApprovals = [...computerApprovals, event.payload];
    }).then((stop) => { if (disposed) stop(); else unlisten = stop; });
    return () => { disposed = true; unlisten?.(); };
  });
  async function answerComputerApproval(request: ComputerApproval, allow: boolean) {
    computerApprovals = computerApprovals.filter((item) => item.id !== request.id);
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('computer_use_approval', { id: request.id, allow }).catch(() => {});
  }

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
      if (part.kind === 'plan' || part.kind === 'tasks') return false;
      if (part.block.kind === 'thinking' && !part.block.text.trim()) continue;
      return false;
    }
    return false;
  }
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { costParts, estimateCost, shortCount, totalTokens, usd } from '../usage';
  import PermissionCard from './PermissionCard.svelte';
  import QuestionCard from './QuestionCard.svelte';
  import { parseQuestions } from '../questions';

  let renderedSession = workspace.activeId;
  $effect(() => {
    const current = workspace.activeId;
    if (current === renderedSession) return;
    const previous = renderedSession;
    renderedSession = current;
    untrack(() => releaseReplyRender(previous));
  });
  import Composer from './Composer.svelte';
  import ToolbarActions from './ToolbarActions.svelte';
  import FileView from './FileView.svelte';
  import { activateTab, closeAllTabs, closeOtherTabs, closeTab, contentTabs, openChatTab, openNewChatTab, saveTab, type ContentTab } from '../content-tabs.svelte';
  import type { ZDropdownMenuItem } from '@shival99/z-ui-svelte';

  // The folded desktop sidebar: hovering its button shows the sidebar floating over the app (SidebarPeek) after a
  // short wait. A click pins it back into the layout, in the place and at the width of the peek.
  function railEnter() {
    if (!layout.sidebarCollapsed || layout.sidebarHidden) return;
    peekSidebar(layout.sidebarDrawer ? 0 : 200);
  }
  function railLeave() {
    if (layout.sidebarCollapsed && !layout.sidebarHidden) leavePeek();
  }
  // The peek covers the folded button: a press where the button is still pins the sidebar (it lands on the peek's
  // header otherwise).
  let railButton = $state<HTMLButtonElement | undefined>();
  $effect(() => {
    if (!layout.sidebarDrawer || !layout.sidebarCollapsed) return;
    const onPress = (event: PointerEvent) => {
      const box = railButton?.getBoundingClientRect();
      if (!box || event.clientX < box.left || event.clientX > box.right || event.clientY < box.top || event.clientY > box.bottom) return;
      event.preventDefault();
      event.stopPropagation();
      pinSidebar();
    };
    document.addEventListener('pointerdown', onPress, true);
    return () => document.removeEventListener('pointerdown', onPress, true);
  });

  // Right click on a tab: close it, the others, or all of them.
  let tabMenu = $state<{ x: number; y: number; id: string } | null>(null);
  let tabMenuOpen = $state(false);
  $effect(() => closeOnScroll(() => tabMenuOpen, () => (tabMenuOpen = false)));
  let tabMenuKey = $state(0);
  function openTabMenu(event: MouseEvent, id: string) {
    event.preventDefault();
    tabMenuKey++;
    tabMenu = { x: event.clientX, y: event.clientY, id };
    tabMenuOpen = true;
    // For a file tab: the editors this machine has, for "Open with".
    openWithList = [];
    const file = contentTabs.tabs.find((tab) => tab.id === id);
    if (file?.kind === 'file') void openWithOptions(file.path).then((found) => tabMenu?.id === id && (openWithList = found));
  }
  let openWithList = $state<OpenWithOption[]>([]);
  // Save shows only for a file tab with unsaved edits; it saves that tab, in front or not.
  const menuTab = $derived(tabMenu ? contentTabs.tabs.find((tab) => tab.id === tabMenu?.id) : undefined);
  const tabContextItems = $derived<ZDropdownMenuItem[]>([
    ...(menuTab?.kind === 'file' && menuTab.dirty
      ? [{ label: 'Save', icon: 'lucideSave', iconSize: '12', shortcut: 'Ctrl+S', divide: 'after', class: 'text-xs! py-1', onClick: () => tabMenu && void saveTab(tabMenu.id) } as ZDropdownMenuItem]
      : []),
    ...(menuTab?.kind === 'file'
      ? [
          { label: 'Open in the side panel', icon: 'lucidePanelRight', iconSize: '12', class: 'text-xs! py-1', onClick: () => openInViewer(menuTab.path, menuTab.root, menuTab.view) } as ZDropdownMenuItem,
          ...openWithItem(menuTab.path, openWithList),
          { label: 'Copy path', icon: 'lucideCopy', iconSize: '12', class: 'text-xs! py-1', divide: 'after', onClick: () => void navigator.clipboard.writeText(menuTab.path) } as ZDropdownMenuItem,
        ]
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
  // Tabs cut off on the left: the strip is scrolled past its first tab.
  let tabScrolled = $state(false);
  $effect(() => {
    void contentTabs.tabs.length;
    void tabStripWidth;
    tabsOverflow = contentTabs.tabs.length > 1 && !!tabStrip && tabStrip.scrollWidth > tabStrip.clientWidth + 1;
  });
  // The active tab (a click, or a pick in the arrow menu) scrolls into view.
  $effect(() => {
    const id = contentTabs.active;
    void contentTabs.tabs.length;
    void tabStripWidth;
    void tick().then(() => {
      revealTab(tabStrip, id);
      tabScrolled = (tabStrip?.scrollLeft ?? 0) > 1;
    });
  });
  const fileActive = $derived(contentTabs.tabs.find((tab) => tab.id === contentTabs.active)?.kind === 'file');
  const tabListItems = $derived<PickerItem<string>[]>(
    contentTabs.tabs.map((tab) => ({ id: tab.id, label: tabLabel(tab), icon: (tab.kind === 'chat' ? 'lucideMessageSquare' : 'lucideFile') as ZIconName, selected: contentTabs.active === tab.id, data: tab.id })),
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


  const agentText = { claude: 'text-orange-500', codex: 'text-emerald-500', antigravity: 'text-sky-500', opencode: 'text-foreground', custom: 'text-violet-500' } as const;
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
      // The pill at the top shows while it loads; no note after it.
      void loadOlder(activeSession());
    }
  }

  // The page before is read only when the reader scrolls up into the last screen and a half above the first row. Not
  // ahead of time and not on the virtualizer's own scrolls: putting a page in front moves scrollTop, and each of those
  // moves used to read the next page, and the next, until the whole chat was in.
  // During a scrollbar drag the browser owns scrollTop, and rows put in front would move it under the thumb: the page
  // is read once the button is released.
  let draggingBar = false;
  let lastTop = 0;
  function onViewportPointerDown(event: PointerEvent) {
    stopSticking();
    if (!viewportEl || event.offsetX < viewportEl.clientWidth) return;
    draggingBar = true;
    const release = () => {
      draggingBar = false;
      window.removeEventListener('pointerup', release);
      window.removeEventListener('pointercancel', release);
      if (viewportEl && viewportEl.scrollTop < nearTop()) revealOlder();
    };
    window.addEventListener('pointerup', release);
    window.addEventListener('pointercancel', release);
  }
  const nearTop = () => Math.max(600, (viewportEl?.clientHeight ?? 0) * 1.5);
  // Rows not measured yet are guessed from their own text (row-size.ts), or taken from the height they had when last
  // drawn at this width: a wrong guess makes the rows around it move once it is measured, which a fast scroll or a
  // scrollbar drag shows as a stutter.
  let listWidth = $state(800);
  let viewportHeight = $state(600);
  // The last height each row was measured at in this view, live replies included (row-size.ts keeps only finished ones).
  const measured = new Map<string, number>();
  const virtualOptions = () => ({
    count: chat.messages.length - from,
    getScrollElement: () => viewportEl ?? null,
    estimateSize: (index: number) => {
      const message = chat.messages[index + from];
      return message ? rowHeight(message, listWidth) : 96;
    },
    getItemKey: (index: number) => chat.messages[index + from]?.id ?? index + from,
    anchorTo: 'end' as const,
    followOnAppend: true,
    scrollEndThreshold: 80,
    // Sizes come from the ResizeObserver, which reports them after layout. The default reads offsetHeight when a row is
    // put in, which forces a layout per row in the middle of a scroll frame (profiled: most of the stutter). A row that
    // has no observer entry yet stands at the height it had before, or its guess, until the observer reports.
    measureElement: (element: Element, entry: ResizeObserverEntry | undefined) => {
      const message = chat.messages[Number((element as HTMLElement).dataset.index) + from];
      const box = entry?.borderBoxSize?.[0]?.blockSize;
      if (box === undefined) {
        // Asked again without an observer entry (the row was put in again): the height it was last measured at. A
        // reply being written has no saved height, and falling back to its guess shrank the list and grew it back on
        // the next report, so the text jumped up and down while it streamed.
        const known = message ? measured.get(`${message.id}|${listWidth}`) : undefined;
        return known ?? (message ? rowHeight(message, listWidth) : 96);
      }
      const height = Math.round(box);
      if (message) {
        measured.set(`${message.id}|${listWidth}`, height);
        if (measured.size > 2000) measured.delete(measured.keys().next().value as string);
        if (!message.live) rememberRowHeight(message.id, listWidth, height);
      }
      return height;
    },
    // Rows mounted beyond the screen: enough that a fast drag shows laid-out rows, not blanks, between frames.
    overscan: 8,
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

  /** Whether the "Scroll to latest" button may show: false for a moment after a chat opens at its end. */
  let jumpReady = $state(true);
  $effect(() => {
    if (!revealed || jumpReady) return;
    const timer = setTimeout(() => (jumpReady = true), 700);
    return () => clearTimeout(timer);
  });

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
  // Where each chat was being read when it was left (the message at the top of the view, how far from the top of the
  // view it sat, and whether it was at the end). Kept while the app is open, whatever the chat does meanwhile (running,
  // stopped, idle): coming back puts the view where it was. A chat left at its end opens at its end again, so the newest
  // lines show; the Back button of a child chat always returns to the place.
  const readingAt = new Map<string, { id: string; offset: number; atEnd: boolean }>();
  let shownSession = '';
  let restoreFor = '';
  let restoreTarget: { id: string; offset: number; atEnd: boolean } | null = null;
  function rememberReading(sessionId: string) {
    if (!sessionId || !viewportEl || chat.messages.length === 0) return;
    const top = viewportEl.getBoundingClientRect().top;
    const first = [...viewportEl.querySelectorAll<HTMLElement>('[role="log"] > div[data-index]')].find((row) => row.getBoundingClientRect().bottom > top + 1);
    const message = first ? chat.messages[Number(first.dataset.index)] : undefined;
    if (first && message) readingAt.set(sessionId, { id: message.id, offset: Math.round(first.getBoundingClientRect().top - top), atEnd: following });
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
      const left = readingAt.get(id);
      restoreTarget = left && (restoreFor === id || !left.atEnd) ? left : null;
      restoreFor = '';
      // Back at a place read earlier (scrolled up): the jump button belongs there at once. Opening at the end: it waits
      // until the list has settled, so it does not flash while rows are still measured.
      jumpReady = Boolean(restoreTarget);
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
    if (nearEnd(1)) return turnRows.length - 1;
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
  const pendingQuestion = $derived.by(() => {
    let lastUser = -1;
    for (let index = chat.messages.length - 1; index >= 0; index--) {
      if (chat.messages[index].role === 'user') {
        lastUser = index;
        break;
      }
    }
    for (let index = chat.messages.length - 1; index > lastUser; index--) {
      const message = chat.messages[index];
      if (message.role !== 'assistant' || message.live) continue;
      for (const block of message.blocks) {
        if (block.kind !== 'response') continue;
        const questions = parseQuestions(block.text).questions;
        if (questions.length) return { messageId: message.id, questions, agent: message.agent ?? activeSession()?.source ?? activeSession()?.agent };
      }
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
  const atEnd = $derived(nearEnd(JUMP_AFTER));
  // While rows are still being measured `atEnd` flips back and forth; following it made the button blink. It hides at
  // once, but shows only once the view has stayed away from the end for a moment, and never while the view is still
  // being pinned to the end after the chat opened.
  let showJump = $state(false);
  $effect(() => {
    if (atEnd || !jumpReady) {
      showJump = false;
      return;
    }
    const away = performance.now();
    let timer: ReturnType<typeof setTimeout>;
    // Checked again and again: scrolling up by hand ends the pinning early, and then it should not wait for it.
    const check = () => {
      const now = performance.now();
      if (now - away >= 300 && now >= stickUntil) showJump = true;
      else timer = setTimeout(check, 150);
    };
    timer = setTimeout(check, 300);
    return () => clearTimeout(timer);
  });
  const running = $derived(activeSession()?.status === 'working');
  /** The chat the open one is a sub chat of, if it is one. */
  const parentOfActive = $derived.by(() => {
    const parentId = activeSession()?.parentId;
    return parentId ? workspace.sessions.find((item) => item.id === parentId) : undefined;
  });
  /** The delegated task the open chat works on, when an agent handed it over with delegate_task. */
  const taskOfActive = $derived(Object.values(delegateTasks).find((record) => record.childId === workspace.activeId));
  // The brief is unfolded for one chat at a time: opening another chat folds it again.
  let briefFor = $state('');
  const briefOpen = $derived(briefFor !== '' && briefFor === workspace.activeId);
  /** "1 minute ago", read again every minute. */
  function ago(at: number): string {
    const minutes = Math.floor((ticker.min - at) / 60000);
    if (minutes < 1) return 'just now';
    if (minutes < 60) return `${minutes} minute${minutes === 1 ? '' : 's'} ago`;
    const hours = Math.floor(minutes / 60);
    return hours < 24 ? `${hours} hour${hours === 1 ? '' : 's'} ago` : `${Math.floor(hours / 24)} day${hours < 48 ? '' : 's'} ago`;
  }

  // While a reply streams the view follows it, but only for a reader who is already at the bottom:
  // scrolling up to read stops the following, sending a message (or the jump button) resumes it.
  let following = true;
  function onViewportScroll() {
    if (!viewportEl) return;
    // scrollTop only: scrollHeight here forced a layout on every scroll event. The list's height is the virtualizer's.
    const top = viewportEl.scrollTop;
    following = listHeight - top - viewportHeight < 120;
    // Upward only: a page put in front pushes scrollTop down, which never reads another one.
    const up = top < lastTop;
    lastTop = top;
    if (up && !draggingBar && top < nearTop()) revealOlder();
  }
  // Follows what is on screen, not what has arrived: the answer is revealed word by word, so the list grows a little
  // every frame. Each time its measured height changes the view is put at the bottom right away (same frame, no
  // animation frame in between), so the text glides up instead of the view catching up in jumps.
  const listHeight = $derived($virtualizer.getTotalSize());
  // The rows on screen through a derived: the virtualizer's store changes on every scroll event, but it hands back the
  // same array while the range stays the same, so the list is laid out again only when rows come or go, not on every
  // event (profiled at several ms a frame).
  const virtualRows = $derived($virtualizer.getVirtualItems());
  /** Within `px` of the end, from the virtualizer's own numbers: isAtEnd() reads scrollHeight, a forced layout on
   *  every scroll frame for what runs here each frame (the jump button, the ruler's current turn). */
  function nearEnd(px: number): boolean {
    return listHeight - ($virtualizer.scrollOffset ?? 0) - viewportHeight <= px;
  }
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
  <header class="relative z-40 flex h-10 shrink-0 items-center border-b border-border bg-background {layout.toolsVisible ? 'pr-[7px]' : 'pr-[106px]'}" data-tauri-drag-region="deep">
    <!-- Browser-style tabs: chats and files side by side, each with a close button; + starts a new chat.
         No scrollbar: when the tabs overflow, an arrow opens the full list. -->
    {#if layout.sidebarHidden || layout.sidebarCollapsed}
      <button
        bind:this={railButton}
        type="button"
        class="ml-2 grid size-7 shrink-0 cursor-pointer place-items-center rounded-md text-muted-foreground hover:bg-primary/10 hover:text-primary"
        aria-label={layout.sidebarHidden ? 'Open sessions' : 'Show sidebar (Ctrl+B)'}
        title={layout.sidebarHidden ? 'Sessions' : 'Show sidebar (Ctrl+B)'}
        data-sidebar-rail
        onpointerenter={railEnter}
        onpointerleave={railLeave}
        onclick={() => (layout.sidebarHidden ? (layout.sidebarDrawer = !layout.sidebarDrawer) : pinSidebar())}
      >
        <ZIcon zType={layout.sidebarHidden ? 'lucideMenu' : 'lucidePanelLeft'} class="size-4" />
      </button>
    {/if}
    <div class="flex min-w-0 flex-1 items-center gap-1 pl-2">
      <div class="relative flex min-w-0 shrink items-center">
        <div bind:this={tabStrip} bind:clientWidth={tabStripWidth} onscroll={() => (tabScrolled = (tabStrip?.scrollLeft ?? 0) > 1)} class="flex min-w-0 flex-1 items-center gap-1 overflow-hidden" role="tablist" aria-label="Open tabs" data-tauri-drag-region>
          {#each contentTabs.tabs as tab (tab.id)}
            {@const active = contentTabs.active === tab.id}
            {@const label = tabLabel(tab)}
            <div
              role="tab"
              tabindex="0"
              data-tab-id={tab.id}
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
        {#if tabScrolled}
          <div class="pointer-events-none absolute inset-y-0 left-0 z-10 w-10 bg-gradient-to-r from-background to-transparent" aria-hidden="true"></div>
        {/if}
      </div>
      {#if tabsOverflow}
        <div class="-mr-0.5 flex shrink-0 items-center bg-background">
          <TabListButton items={tabListItems} onpick={activateTab} label="Open tabs" />
        </div>
      {/if}
      {#if contentTabs.tabs.some((tab) => tab.kind === 'chat')}
        <button
          type="button"
          class="grid size-7 shrink-0 cursor-pointer place-items-center rounded-md text-muted-foreground outline-none focus-visible:ring-0 hover:bg-primary/10 hover:text-primary"
          aria-label="New tab"
          title="New tab"
          onclick={() => openNewChatTab()}
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
      <Menu items={tabContextItems} bind:open={tabMenuOpen} at={{ x: tabMenu.x, y: tabMenu.y }} position="bottom-left" width={260} />
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
      <!-- A sub chat (a step of a flow, a delegated task, a fork): who started it, from which chat (one click back),
           and for a delegated task the brief it was handed, folded under the line. -->
      <div class="relative z-30 shrink-0 border-b border-border bg-background px-3 text-xs text-muted-foreground">
        <div class="flex h-8 min-w-0 items-center gap-1.5">
          {#if taskOfActive}
            <span class="shrink-0">Started by</span>
            <AgentLogo agent={parentOfActive.agent} model={parentOfActive.model} class="size-3.5 shrink-0" />
            <span class="shrink-0 text-foreground">{agentDisplayName(parentOfActive.agent)}</span>
            <span class="shrink-0">from</span>
          {:else}
            <span class="shrink-0">Started from</span>
          {/if}
          <button type="button" class="min-w-0 cursor-pointer truncate rounded-sm font-medium text-foreground underline decoration-muted-foreground/50 underline-offset-2 outline-none hover:text-primary hover:decoration-primary" aria-label="Back to {parentOfActive.title}" onclick={() => backToParent(parentOfActive.id)}>{parentOfActive.title}</button>
          {#if taskOfActive}
            <span class="shrink-0 whitespace-nowrap">{ago(taskOfActive.startedAt)}</span>
            <button type="button" class="grid size-6 shrink-0 cursor-pointer place-items-center rounded-md outline-none hover:bg-primary/10 hover:text-primary" aria-expanded={briefOpen} aria-label={briefOpen ? 'Hide the brief' : 'Show the brief'} title={briefOpen ? 'Hide the brief' : 'Show the brief it was handed'} onclick={() => (briefFor = briefOpen ? '' : workspace.activeId)}>
              <ZIcon zType={'lucideChevronRight' as ZIconName} class="size-3.5 transition-transform duration-150 {briefOpen ? 'rotate-90' : ''}" />
            </button>
          {/if}
        </div>
        {#if taskOfActive && briefOpen}
          <div class="mb-2 max-h-48 overflow-auto rounded-md border border-border bg-muted/40 px-2.5 py-2 text-xs leading-relaxed whitespace-pre-wrap text-foreground">{taskOfActive.task}</div>
        {/if}
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
    {#if showJump}
      <!-- The wrapper carries the entrance (it rises into place while it fades in, and goes the same way); the button inside keeps its own look. -->
      <div transition:fly={{ y: 10, duration: 200 }} class="pointer-events-none absolute inset-x-0 bottom-3 z-20 flex justify-center">
        <ZButton
          zWave={false}
          zType="outline"
          class="group/jump pointer-events-auto size-8 rounded-full bg-card! p-0 shadow-md"
          aria-label="Scroll to latest"
          title="Scroll to latest"
          onclick={() => ((following = true), $virtualizer.scrollToEnd())}
        >
          <!-- Own grid so the spinner and the arrow sit dead centre; the button's inline content box drifted down-right. -->
          <span class="grid size-full place-items-center">
            {#if running}
              <!-- Both sit in the same cell and trade places on hover: the spinner fades and shrinks out, the arrow grows in. -->
              <span class="col-start-1 row-start-1 grid place-items-center transition duration-200 ease-out group-hover/jump:scale-50 group-hover/jump:opacity-0 motion-reduce:transition-none"><ZLoading zLoading zSpinner={LOADER.scroll} zSize="xs" zColor="primary" /></span>
              <ZIcon zType="lucideArrowDown" class="col-start-1 row-start-1 size-4 scale-50 opacity-0 transition duration-200 ease-out group-hover/jump:scale-100 group-hover/jump:opacity-100 motion-reduce:transition-none" />
            {:else}
              <ZIcon zType="lucideArrowDown" class="size-4" />
            {/if}
          </span>
        </ZButton>
      </div>
    {/if}

    <!-- Same fade as ShowMore: messages dissolve into the page just above the composer instead of being cut. -->
    <div
      class="pointer-events-none absolute right-2.5 bottom-0 left-0 z-10 h-12"
      style="background-image: linear-gradient(to top, var(--background) 0%, transparent 100%)"
      aria-hidden="true"
    ></div>

    {#if chat.loadingOlder}
      <!-- Drops in from the top while a page of older messages loads: a spinner in a soft primary ring and shimmering text. -->
      <div class="pointer-events-none absolute top-3 left-1/2 z-40 -translate-x-1/2" role="status" aria-label="Loading earlier messages" in:fly={{ y: -12, duration: 220 }} out:fade={{ duration: 160 }}>
        <div class="flex items-center gap-2.5 rounded-full border border-primary/25 bg-background/95 py-1.5 pr-4 pl-2 text-[13px] font-medium shadow-lg shadow-primary/10 backdrop-blur">
          <span class="grid size-6 place-items-center rounded-full bg-primary/12"><ZLoading zLoading zSpinner={LOADER.fetch} zSize="xs" zColor="primary" /></span>
          <ZShimmerText zText="Loading earlier messages…" />
        </div>
      </div>
    {/if}

    {#if chat.loading || (!revealed && chat.messages.length > 0)}
      <!-- A thin bar along the top while the chat opens. It shows only if the wait is long (the delay is in the animation),
           so a fast chat opens with nothing at all, and it never mimics the layout, so nothing can ghost over the text. -->
      <div class="chat-loading pointer-events-none absolute inset-x-0 top-0 z-20 h-[2px] overflow-hidden" role="status" aria-label="Loading conversation" out:fade={{ duration: 120 }}>
        <div class="chat-loading-bar"></div>
        <div class="chat-loading-bar chat-loading-bar-trail"></div>
      </div>
    {/if}

    {#if chat.messages.length === 0 && !chat.loading}
      <div class="pointer-events-none absolute inset-0 z-10 grid place-items-center text-xs text-muted-foreground">
        {#if chat.error}
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
    <div bind:this={viewportEl} bind:clientWidth={listWidth} bind:clientHeight={viewportHeight} onscroll={onViewportScroll} onwheel={stopSticking} ontouchmove={stopSticking} onpointerdown={onViewportPointerDown} onkeydown={stopSticking} class="min-h-0 flex-1 overflow-y-auto overflow-x-hidden pl-2.5 [overflow-anchor:none] [scrollbar-gutter:stable] {chat.loading || !revealed ? 'pointer-events-none opacity-0' : 'motion-safe:transition-opacity motion-safe:duration-150 motion-safe:ease-out'}" aria-hidden={chat.loading || !revealed}>
      <div class="relative w-full" style:height="{listHeight}px" {@attach watchListHeight} role="log" aria-live="polite" aria-label="Conversation">
        <!-- A theme change repaints every row from scratch: a row kept from before can show the old theme's colours until the page is reloaded. -->
        {#key `${theme.dark}|${theme.primary}|${theme.neutral}`}
        {#each virtualRows as row (row.key)}
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
                    <!-- Editing: the message becomes a box; Send replaces it and everything after it. The text runs into the
                         box's right padding, so a long message's scrollbar sits flush with the box's edge. -->
                    <div in:fly={{ y: 4, duration: 220 }} class="flex w-full max-w-[85%] flex-col gap-2 rounded-2xl bg-muted px-4 pt-3 pb-2.5 text-sm">
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
                        class="-mr-4 w-[calc(100%+1rem)] resize-none border-0 bg-transparent p-0 pr-3 text-sm leading-relaxed outline-none"
                      ></textarea>
                      <div class="flex justify-end gap-2">
                        <ZButton zWave={false} zType="outline" zSize="sm" class="h-7 px-3 text-xs" onclick={() => (editingId = null)}>Cancel</ZButton>
                        <ZButton zWave={false} zSize="sm" class="h-7 px-3 text-xs" disabled={!editText.trim()} onclick={() => finishEdit(message.id)}>Send</ZButton>
                      </div>
                    </div>
                  {:else}
                    {#if words.length > 0}
                      <div in:fade={{ duration: 220 }} class="flex max-w-[85%] flex-col gap-2 rounded-2xl bg-muted p-3 text-sm {theme.chatLayout === 'split' ? 'rounded-br-md' : 'rounded-bl-md'}">
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
                    <span class="inline-flex items-center gap-1 font-semibold {agentText[agent]}">
                      <AgentLogo agent={agent} model={message.model} class="size-3 shrink-0" />
                      {agent === 'custom' ? agentDisplayName(agent, message.model) : agentName(agent)}
                    </span>
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
                      <!-- The figures roll only while the reply is being written; a loaded chat shows them plainly. -->
                      {@const rolling = Boolean(message.live) && isRunning(workspace.activeId)}
                      <!-- Tokens and cost, quiet and on the right; hover for the breakdown. -->
                      <span class="ml-auto flex items-baseline gap-1.5 text-[11px] tabular-nums" {@attach zTooltip({ content: usageTip, position: 'top' })}>
                        <span class="text-muted-foreground">{#if rolling}<ZNumberTicker zValue={totalTokens(message.tokens)} zFormat="compact" zDuration={600} />{:else}{shortCount(totalTokens(message.tokens))}{/if} tokens</span>
                        {#if cost != null}
                          <span class="text-muted-foreground/40" aria-hidden="true">·</span>
                          <span class="font-medium text-success">{message.cost == null ? '≈ ' : ''}{#if rolling}<ZNumberTicker zValue={cost} zFormat="currency" zCurrency="USD" zLocale="en-US" zMinimumFractionDigits={2} zMaximumFractionDigits={cost < 0.01 ? 3 : 2} zDuration={600} />{:else}{usd(cost)}{/if}</span>
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
                          <CommandGroup items={segment.items} thoughts={segment.thoughts} live={false} working={Boolean(message.live) && isRunning(workspace.activeId) && !parts.slice(position + 1).some((part) => part.kind === 'block' && part.block.kind !== 'loading')} startedAt={message.startedAt} stateKey="{workspace.activeId}:{message.id}:{position}" />
                        </div>
                      {:else if segment.kind === 'plan'}
                        <PlanCard items={segment.items} />
                      {:else if segment.kind === 'tasks'}
                        <TaskCards items={segment.items} />
                      {:else}
                        <ChatBlock block={segment.block} agent={message.agent ?? activeSession()?.source ?? activeSession()?.agent} onanswer={(answer) => answerPermission(message.id, message.blocks.indexOf(segment.block), answer)} onquestions={(text) => answerQuestions(workspace.activeId, text)} onrendered={segment.block.kind === 'response' ? () => replyBlockRendered(workspace.activeId, message.id, message.blocks.indexOf(segment.block)) : undefined} answered={!message.live && pendingQuestion?.messageId !== message.id} showQuestions={!message.live && pendingQuestion?.messageId !== message.id} />
                      {/if}
                    {/each}
                    <ArtifactCards blocks={message.blocks} root={(activeSession() && projectOf(activeSession()!)?.path) || null} live={Boolean(message.live) && isRunning(workspace.activeId)} />
                    <!-- One status line for the whole reply: what the agent is doing now (thinking, running a command,
                         editing…), as in Claude and Codex; the steps above stay plain, finished lines. -->
                    {#if message.stoppedAfter !== undefined && !message.live}
                      <p class="m-0 text-xs text-muted-foreground">You stopped after {elapsed(message.stoppedAfter)}</p>
                    {/if}
</div>
                  <!-- The status line and the footer share one fixed-height slot, so the row does not jump when the reply ends. -->
                  <!-- Live only while the chat really runs: a reply left marked live (a lost "done") shows its footer. -->
                  {@const showLive = message.live && isRunning(workspace.activeId)}
                  {#if showLive || message.blocks.length > 0 || message.stoppedAfter !== undefined}
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
        {/key}
      </div>
    </div>
  </div>

  <!-- The composer sits on its own layer, above anything the conversation area paints at its edge. -->
  <!-- Hidden while a file tab is on screen: the file gets the full height. -->
  {#if !fileActive}
    <div class="relative z-30 shrink-0 bg-background px-2 pt-4 pb-2 min-[720px]:px-6">
      <div class="mx-auto w-full min-w-0 max-w-3xl px-0 min-[720px]:min-w-[440px] min-[720px]:pr-6 min-[720px]:pl-12">
        <!-- A pending approval sits right above the input, where the eye already is. -->
        <!-- The chat stopped at a usage limit: it carries on by itself when the limit resets. -->
        {#if resumes[workspace.activeId]}<ResumeCard resume={resumes[workspace.activeId]} />{/if}
        {#each computerApprovals as request (request.id)}
          <section class="mb-2 rounded-xl border border-amber-500/40 bg-amber-500/5 p-3" aria-label="Computer use approval request">
            <div class="flex items-start gap-2">
              <ZIcon zType="lucideMonitor" class="mt-0.5 size-4 shrink-0 text-amber-500" />
              <div class="min-w-0 flex-1">
                <div class="text-xs font-semibold">{request.tool} · {request.chat === workspace.activeId ? 'This chat' : 'Another chat'}</div>
                <p class="mt-1 break-words text-xs text-muted-foreground">{request.summary}</p>
              </div>
            </div>
            <div class="mt-2 flex justify-end gap-2">
              <ZButton zWave={false} zType="ghost" zSize="sm" class="h-7 px-3 text-xs" onclick={() => void answerComputerApproval(request, false)}>Deny</ZButton>
              <ZButton zWave={false} zType="default" zSize="sm" class="h-7 px-3 text-xs" onclick={() => void answerComputerApproval(request, true)}>Approve once</ZButton>
            </div>
          </section>
        {/each}
        {#if pendingApproval}
          <div class="mb-2">
            <PermissionCard block={pendingApproval.block} agent={pendingApproval.agent} onanswer={(answer) => answerPermission(pendingApproval.messageId, pendingApproval.index, answer)} />
          </div>
        {/if}
        {#if pendingQuestion}
          <div class="mb-2">
            <QuestionCard questions={pendingQuestion.questions} from={pendingQuestion.agent ? agentName(pendingQuestion.agent) : ''} onsubmit={(text) => answerQuestions(workspace.activeId, text)} />
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
