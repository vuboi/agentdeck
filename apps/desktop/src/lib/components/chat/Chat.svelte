<script lang="ts">
  import { untrack, onMount, tick } from 'svelte';
  import { fade, fly } from 'svelte/transition';
  import { ZChatNavigator, ZGalleryPreview, ZIcon, ZLoading, ZShimmerText, createVirtualizer } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { layout } from '../../app/ui/layout.svelte';
  import { showSession, contentTabs, openChatTab } from '../../app/ui/content-tabs.svelte';
  import { LOADER, LOADER_SIZE } from '../../app/platform/loaders';
  import { rememberRowHeight, rowHeight } from '../../app/ui/row-size';
  import { setEagerColour, whenColoured } from '../../app/text/markdown';
  import { openSelectionMenu } from '../../app/ui/selection-menu.svelte';
  import { activeSession, AGENTS, delegateTasks, chat, loadOlder, needsApproval, openTranscript, pendingProject, prewarmChat, releaseReplyRender, workspace } from '../../chat/sessions.svelte';
  import { ticker } from '../../app/platform/clock.svelte';
  import { theme } from '../../app/ui/theme.svelte';
  import { hollow, markerText, neighbour, turnDivider } from '../../chat/content/chat-dividers';
  import { preview, releasePreview, revealPreviewed } from '../../browser/preview.svelte';
  import { logError } from '../../app/platform/error-log.svelte';
  import { parseQuestions } from '../../chat/recovery/questions';
  import FileView from '../files/FileView.svelte';
  import ChatHeader from './ChatHeader.svelte';
  import SubChatBar from './SubChatBar.svelte';
  import DelegateList from '../tasks/DelegateList.svelte';
  import JumpButton from './JumpButton.svelte';
  import UserMessage from './UserMessage.svelte';
  import AssistantMessage from './AssistantMessage.svelte';
  import ComposerDock from './ComposerDock.svelte';

  // Opening a chat starts its agent's CLI in the background: the CLI's own start-up is the longest part of a first reply
  // (about 4 s on a good day, sometimes up to a minute for Antigravity). A message typed later finds it ready.
  // A change of the chat's agent or model does the same for the new one, so the first message after it is not cold.
  $effect(() => {
    const id = workspace.activeId;
    void activeSession()?.agent;
    void activeSession()?.model;
    const timer = setTimeout(() => prewarmChat(id), 1200);
    return () => clearTimeout(timer);
  });

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

  let renderedSession = workspace.activeId;
  $effect(() => {
    const current = workspace.activeId;
    if (current === renderedSession) return;
    const previous = renderedSession;
    renderedSession = current;
    untrack(() => releaseReplyRender(previous));
  });

  let viewportEl: HTMLDivElement | undefined = $state();

  const fileActive = $derived(contentTabs.tabs.find((tab) => tab.id === contentTabs.active)?.kind === 'file');

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
  // Two messages with one id (a reply continued by a hand-back, a history import next to the live copy) would make the
  // list throw `each_key_duplicate` on every frame and stop the chat drawing: the later one gets its position in its key.
  let firstOf: { list: unknown; length: number; at: Map<string, number> } | null = null;
  const rowKey = (index: number): string | number => {
    const list = chat.messages;
    const id = list[index]?.id;
    if (id === undefined) return index;
    if (!firstOf || firstOf.list !== list || firstOf.length !== list.length) {
      const at = new Map<string, number>();
      list.forEach((message, position) => {
        if (!at.has(message.id)) at.set(message.id, position);
      });
      firstOf = { list, length: list.length, at };
    }
    return firstOf.at.get(id) === index ? id : `${id}#${index}`;
  };
  const virtualOptions = () => ({
    count: chat.messages.length - from,
    getScrollElement: () => viewportEl ?? null,
    estimateSize: (index: number) => {
      const message = chat.messages[index + from];
      return message ? rowHeight(message, listWidth) : 96;
    },
    getItemKey: (index: number) => rowKey(index + from),
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
      if (message.role !== 'assistant' || message.live || message.cancelled) continue;
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
  <ChatHeader />

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
      <SubChatBar {parentOfActive} {taskOfActive} {running} onback={backToParent} />
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
    <JumpButton show={showJump} {running} onclick={() => ((following = true), $virtualizer.scrollToEnd())} />

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
          <span class="grid size-6 place-items-center rounded-full bg-primary/12"><ZLoading zLoading zSpinner={LOADER.fetch} zSize={LOADER_SIZE} zColor="primary" /></span>
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
            <!-- One row that cannot be drawn must not blank the chat: it is reported (with its id) and the row says so. -->
            <svelte:boundary onerror={(error) => logError('app', 'chat row failed to draw', error instanceof Error ? `${error.message}\n${error.stack?.split('\n').slice(1, 4).join('\n') ?? ''}` : String(error), { chat: workspace.activeId, row: message.id })}>
            <div class="absolute top-0 left-0 w-full" data-index={row.index} style:transform="translateY({row.start}px)" {@attach measureRow}>
              <!-- A session line (resumed, compacted, new) or, between two replies of the agent, a thin line. Read from the roles and
                   the mark of the neighbours only, never the text, so a reply that is still streaming keeps one stable line. -->
              {#if theme.turnDividers && message.mark}
                <div class="chat-mark mx-auto flex w-full max-w-[52rem] items-center gap-3 px-3 pt-3 min-[720px]:px-8" role="separator" aria-label={markerText(message.mark)}>
                  <span class="chat-rule"></span>
                  <span class="shrink-0 text-[11px] text-muted-foreground">{markerText(message.mark)}</span>
                  <span class="chat-rule"></span>
                </div>
              {:else if hollow(message, chat.messages.at(-1) !== message)}
                <!-- Cut off before it wrote anything: nothing to draw. -->
              {:else if turnDivider(neighbour(chat.messages, row.index + from, -1), message, theme.turnDividers)}
                <div class="mx-auto w-full max-w-[52rem] px-3 pt-1 min-[720px]:px-8" role="separator" aria-hidden="true"><div class="chat-rule"></div></div>
              {/if}
              <!-- No avatars: your messages are a bubble (right side, or left in one-sided mode); the agent is just its coloured name. -->
              {#if !hollow(message, chat.messages.at(-1) !== message)}
              <article oncontextmenu={(event) => openSelectionMenu(event, message.blocks.map((block) => (block.kind === 'response' ? block.text : '')).filter(Boolean).join('\n\n'))} class="group/msg mx-auto flex w-full min-w-0 max-w-[52rem] flex-col break-words py-2.5 px-3 min-[720px]:px-8 {mine && theme.chatLayout === 'split' ? 'items-end' : 'items-start'}">
                {#if mine}
                  <UserMessage {message} />
                {:else}
                  <AssistantMessage {message} index={row.index + from} {agent} questionMessageId={pendingQuestion?.messageId} />
                {/if}
              </article>
              <!-- The hand-offs Claude made for this message (@agent mentions) are Claude's replies: their own row, apart from
                   the user's bubble, so hovering one does not light up the other. -->
              {#if mine && message.blocks.some((block) => block.kind === 'delegate')}
                {@const handed = message.blocks.filter((block) => block.kind === 'delegate')}
                <div class="mx-auto flex w-full min-w-0 max-w-[52rem] flex-col items-start px-3 pb-2.5 min-[720px]:px-8">
                  <DelegateList blocks={handed} />
                </div>
              {/if}
              {/if}
            </div>
            {#snippet failed()}
              <div class="absolute top-0 left-0 w-full" data-index={row.index} style:transform="translateY({row.start}px)" {@attach measureRow}>
                <p class="mx-auto my-2 w-full max-w-[52rem] px-3 text-xs text-muted-foreground min-[720px]:px-8">This message could not be drawn. It is saved; open the chat again to retry.</p>
              </div>
            {/snippet}
            </svelte:boundary>
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
      <div class="mx-auto w-full min-w-0 max-w-[52rem] px-3 min-[720px]:min-w-[440px] min-[720px]:px-8">
        <!-- A pending approval sits right above the input, where the eye already is. -->
        <ComposerDock {pendingApproval} {pendingQuestion} {stepApprovals} />
      </div>
    </div>
  {/if}
</main>

<ZGalleryPreview bind:file={preview.file} files={preview.files} onClose={releasePreview} onDownload={revealPreviewed} />
