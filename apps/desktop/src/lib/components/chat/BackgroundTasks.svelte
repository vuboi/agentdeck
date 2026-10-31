<script lang="ts" module>
  // Open state of the work list and of its rows, per chat. Kept for the app's lifetime, not per mount.
  const workOpen = $state<Record<string, boolean>>({});
  const workDetails = $state<Record<string, boolean>>({});
</script>

<script lang="ts">
  import { ZButton, ZIcon, ZLoading, ZShimmerText, zTooltip } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { lucidePlay } from '@ng-icons/lucide';
  import { cubicOut } from 'svelte/easing';
  import { flip } from 'svelte/animate';
  import { Tween } from 'svelte/motion';
  import { modelLabel } from '../../agents/models.svelte';
  import { agentDisplayName } from '../../agents/provider-registry';
  import { ticker } from '../../app/platform/clock.svelte';
  import { LOADER, LOADER_SIZE } from '../../app/platform/loaders';
  import { showSession } from '../../app/ui/content-tabs.svelte';
  import { elapsed } from '../../chat/content/blocks';
  import { requestFocusMessage } from '../../chat/content/focus-message.svelte';
  import { refreshProjectRun, stopProjectRun } from '../../chat/runs/activity.svelte';
  import { isWorkActive, stopTarget, type ChatWork } from '../../chat/runs/chat-work';
  import { returnTaskResult, pauseDelegateTask, resumeDelegateTask, stopDelegateTask } from '../../chat/session/delegate-task.svelte';
  import { stopTurnForResume, resumeStoppedTurn } from '../../chat/session/events.svelte';
  import { stopTurn } from '../../chat/session/turns.svelte';

  /** Outside the virtual transcript: scrolling and loading old messages cannot unmount the work list.
   *  `title` names the list; `stateKey` keys its open state (the chat by default, so each card can have its own). */
  // `layer` turns the list into a tray above the composer, in the same shell as the queue (ComposerQueue) and inset from it.
  // `embedded` drops the shell: the caller (Composer) draws one tray around the work list and the queue.
  let { items, sessionId, title = 'Work', stateKey = sessionId, layer = false, embedded = false }: { items: ChatWork[]; sessionId: string; title?: string; stateKey?: string; layer?: boolean; embedded?: boolean } = $props();
  // Folded by default: each chat keeps its own open state, so the list opens only when the user asks for it.
  // Module level (like the chat's scroll position): the state outlives this component, so switching to another chat
  // and back shows the list as the user left it.
  const open = workOpen;
  const details = workDetails;
  const isOpen = $derived(Boolean(open[stateKey]));
  const active = $derived(items.filter(isWorkActive));
  const paused = $derived(active.filter((item) => item.state === 'paused'));
  const finished = $derived(items.filter((item) => !isWorkActive(item) && item.state !== 'unknown'));
  const uncertain = $derived(items.filter((item) => item.state === 'unknown'));
  // Every task is listed (live first, then the saved ones); the list scrolls inside its fixed height.
  // Running work first, then everything else that is still live, then the saved results. Rows move to their place.
  const rank = (item: ChatWork) => item.state === 'running' ? 0 : isWorkActive(item) ? 1 : item.state === 'unknown' ? 2 : 3;
  const shown = $derived([...items].sort((a, b) => rank(a) - rank(b)));
  const running = $derived(active.length - paused.length);
  // The counts glide to their new value over 700ms, one digit at a time, so the change is easy to follow.
  const COUNT = { duration: 700, easing: cubicOut };
  const countRunning = Tween.of(() => running, COUNT);
  const countPaused = Tween.of(() => paused.length, COUNT);
  const countCheck = Tween.of(() => uncertain.length, COUNT);
  const countSaved = Tween.of(() => finished.length, COUNT);
  const labels = { queued: 'Queued', running: 'Working', paused: 'Paused', waiting: 'Returning result', input: 'Needs you', done: 'Completed', failed: 'Failed', stopped: 'Stopped', unknown: 'Check status' } as const;
  // The icon that leads each row, by state. Pending and waiting read as a clock.
  const STATE_ICON: Record<ChatWork['state'], ZIconName> = {
    queued: 'lucideListOrdered' as ZIconName,
    running: 'lucideClock' as ZIconName,
    paused: 'lucideCirclePause' as ZIconName,
    waiting: 'lucideHourglass' as ZIconName,
    input: 'lucideCircleAlert' as ZIconName,
    done: 'lucideCircleCheck' as ZIconName,
    failed: 'lucideCircleX' as ZIconName,
    stopped: 'lucideBan' as ZIconName,
    unknown: 'lucideCircleAlert' as ZIconName,
  };
  const tone = (item: ChatWork) => item.state === 'failed' ? 'text-destructive' : item.state === 'input' || item.state === 'unknown' ? 'text-amber-500' : item.state === 'done' ? 'text-emerald-500' : 'text-muted-foreground';
  const ICON_BUTTON = 'grid size-4 shrink-0 cursor-pointer place-items-center text-muted-foreground outline-none transition-colors hover:text-primary disabled:cursor-default disabled:opacity-40 disabled:hover:text-muted-foreground';
  // Stop all still uses each row's own handle; it never stops the main chat turn as a shortcut.
  const stoppable = $derived(active.filter((item) => stopTarget(item) !== null));
  // Work is also rendered without the transcript's LiveCommand card. Keep AgentDeck-owned project rows in step with
  // the Rust server registry so a process killed externally does not remain "active" forever in this tray.
  $effect(() => {
    const projectKeys = items.filter((item) => item.canStop && item.activityKey).map((item) => item.activityKey!);
    if (!projectKeys.length) return;
    const refresh = () => projectKeys.forEach((key) => void refreshProjectRun(key));
    void refresh();
    const timer = window.setInterval(refresh, 1000);
    return () => window.clearInterval(timer);
  });
  // Stop ends what the row runs: an AgentDeck-owned project server, a worker task, or a child chat's turn.
  function stopItem(item: ChatWork) {
    switch (stopTarget(item)) {
      case 'project':
        void stopProjectRun(item.activityKey!);
        break;
      case 'delegate':
        stopDelegateTask(item.delegateId!);
        break;
      case 'child':
        stopTurn(item.childId!);
        break;
    }
  }
  function stopAll() {
    for (const item of stoppable) stopItem(item);
  }
</script>

<section class="min-w-0 overflow-hidden text-xs {embedded ? '' : layer ? '-mb-4 mx-2 rounded-t-2xl border border-b-0 border-border bg-card pb-4 shadow-md' : 'mb-2 rounded-lg border border-border/70'}" aria-label="Chat tasks">
  <div class="flex h-8 w-full min-w-0 items-center gap-1 pr-2">
  <button type="button" class="flex h-8 min-w-0 flex-1 cursor-pointer items-center gap-2 px-3 text-left text-muted-foreground outline-none hover:text-foreground" aria-label="Toggle {title}" aria-expanded={isOpen} onclick={() => (open[stateKey] = !isOpen)}>
    {#if running > 0}
      <ZLoading zLoading zSpinner={LOADER.working} zSize={LOADER_SIZE} zColor="current" />
    {:else}
      <ZIcon zType={'lucideListChecks' as ZIconName} class="size-3.5 shrink-0" />
    {/if}
    <ZShimmerText class="font-medium" zBase="var(--muted-foreground)" zPeak="var(--foreground)" zText={title} />
    <span class="flex min-w-0 flex-1 items-center gap-2 truncate tabular-nums">
      {#if running > 0}<span>{Math.round(countRunning.current)} active</span>{/if}
      {#if paused.length > 0}<span>{Math.round(countPaused.current)} paused</span>{/if}
      {#if uncertain.length > 0}<span class="text-warning">{Math.round(countCheck.current)} to check</span>{/if}
      {#if finished.length > 0}<span>{Math.round(countSaved.current)} saved</span>{/if}
      {#if items.length === 0}<span>All work settled</span>{/if}
    </span>
  </button>
  {#if stoppable.length > 0}
    <button type="button" class="inline-flex h-6 shrink-0 cursor-pointer items-center gap-1 rounded-md px-2 text-[0.6875rem] text-muted-foreground outline-none transition-colors hover:bg-destructive/10 hover:text-destructive focus-visible:ring-2 focus-visible:ring-ring/60" aria-label="Stop all running work" title="Stop all running work" onclick={stopAll}>
      <svg class="size-3" viewBox="0 0 16 16" aria-hidden="true"><rect x="2.5" y="2.5" width="11" height="11" rx="2" fill="currentColor" /></svg>
      Stop all
    </button>
  {/if}
  <button type="button" class="grid size-6 shrink-0 cursor-pointer place-items-center rounded-md text-muted-foreground outline-none transition-colors hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/60" aria-label="Toggle {title}" aria-expanded={isOpen} onclick={() => (open[stateKey] = !isOpen)}>
    <ZIcon zType={'lucideChevronDown' as ZIconName} class="size-3 shrink-0 transition-transform duration-300 motion-reduce:transition-none {isOpen ? '' : '-rotate-90'}" />
  </button>
  </div>
  <!-- The list folds in place with a grid-row transition: 0fr closed, 1fr open. Closed, it is also inert. -->
  <div class="grid transition-[grid-template-rows] duration-300 ease-in-out motion-reduce:transition-none {isOpen ? 'grid-rows-[1fr]' : 'grid-rows-[0fr]'}">
    <div class="min-h-0 overflow-hidden" inert={!isOpen}>
      <div class="max-h-48 overflow-y-auto overscroll-contain border-t border-border/60" aria-label="Task list">
        {#each shown as item (item.id)}
          <div class="border-b border-border/50 last:border-b-0" data-work-id={item.id} animate:flip={{ duration: 300, easing: cubicOut }}>
            <div class="flex min-h-9 min-w-0 items-center gap-3 px-3 py-1.5">
              <button type="button" class="flex min-w-0 flex-1 cursor-pointer items-center gap-3 text-left outline-none" aria-label="Details: {item.title}" aria-expanded={Boolean(details[item.id])} onclick={() => (details[item.id] = !details[item.id])}>
                <!-- The state leads the row: the loader while it works, then a check, a cross, a clock while it waits. -->
                <span class="grid size-4 shrink-0 place-items-center {tone(item)}" role="img" aria-label={labels[item.state]}>
                  {#if item.state === 'running'}<ZLoading zLoading zSpinner={LOADER.working} zSize={LOADER_SIZE} zColor="current" />{:else}<ZIcon zType={STATE_ICON[item.state]} class="size-3.5" />{/if}
                </span>
                <span class="flex min-w-0 flex-1 flex-col gap-0.5">
                  {#if item.state === 'running'}
                    <ZShimmerText class="truncate text-[0.8125rem] font-medium" zBase="var(--primary)" zPeak="var(--foreground)" zText={item.title} />
                  {:else}
                    <span class="truncate text-[0.8125rem] font-medium text-foreground">{item.title}</span>
                  {/if}
                  <span class="truncate text-[0.6875rem] text-muted-foreground">{item.agent ? agentDisplayName(item.agent, item.model) : item.kind === 'command' ? 'Command' : 'Agent'}{item.model ? ` · ${modelLabel(item.model)}` : ''}{item.sourceTitle ? ` · ${item.sourceTitle}` : ''}</span>
                </span>
                <!-- The leading icon says the state; the time sits at the end of the row. -->
                <span class="flex w-[3.75rem] shrink-0 items-center justify-end gap-1.5 whitespace-nowrap text-[0.6875rem] tabular-nums text-muted-foreground">
                  {elapsed((item.state === 'paused' ? item.pausedAt ?? ticker.sec : isWorkActive(item) ? ticker.sec : item.endedAt ?? ticker.sec) - item.startedAt)}
                </span>
              </button>
              {#if item.sourceMessageId}
                <button type="button" class={ICON_BUTTON} aria-label="Show in chat" onclick={() => requestFocusMessage(item.sourceMessageId!)}>
                  <ZIcon zType={'lucideMessageSquare' as ZIconName} class="size-3.5" />
                </button>
              {/if}
              {#if item.canPause || item.canResume}
                <button type="button" class={ICON_BUTTON} disabled={!item.canPause && !item.canResume} aria-label="{item.canResume ? 'Resume' : 'Pause'} worker: {item.title}" {@attach zTooltip({ content: item.canResume ? 'Resume worker' : item.canPause ? 'Pause worker' : 'This provider cannot pause this worker', position: 'top' })} onclick={() => {
                  if (item.delegateId) {
                    if (item.canResume) void resumeDelegateTask(item.delegateId);
                    if (item.canPause) pauseDelegateTask(item.delegateId);
                    return;
                  }
                  if (!item.childId) return;
                  if (item.canResume) resumeStoppedTurn(item.childId);
                  if (item.canPause) stopTurnForResume(item.childId);
                }}>
                  {#if item.canResume}
                    <ZIcon zSvg={lucidePlay} zSize="12" class="[&>svg]:fill-current" />
                  {:else}
                    <ZIcon zType={'lucideSquarePause' as ZIconName} zSize="12" class="size-3" />
                  {/if}
                </button>
              {/if}
              {#if item.canStop}
                <button type="button" class={ICON_BUTTON} aria-label="Stop: {item.title}" {@attach zTooltip({ content: 'Stop this task', position: 'top' })} onclick={() => stopItem(item)}>
                  <svg class="size-3" viewBox="0 0 16 16" aria-hidden="true"><rect x="2.5" y="2.5" width="11" height="11" rx="2" fill="currentColor" /></svg>
                </button>
              {/if}
              {#if item.childId}
                <button type="button" class={ICON_BUTTON} aria-label="Open worker" onclick={() => showSession(item.childId!)}>
                  <ZIcon zType={'lucideArrowUpRight' as ZIconName} class="size-3.5" />
                </button>
              {/if}
            </div>
            <!-- The details fold open under their own item, in a box that belongs to it: a grid-row transition, 0fr closed. -->
            <div class="grid transition-[grid-template-rows] duration-300 ease-in-out motion-reduce:transition-none {details[item.id] ? 'grid-rows-[1fr]' : 'grid-rows-[0fr]'}">
              <div class="min-h-0 overflow-hidden {details[item.id] ? 'border-t border-border/50' : ''}" inert={!details[item.id]}>
                <div class="m-2 flex flex-col gap-2 rounded-md border border-border/60 bg-muted/30 p-1.5 text-xs leading-snug text-muted-foreground">
                  {#if item.sourceTitle}<p class="m-0 whitespace-pre-wrap break-words"><span class="font-medium text-foreground">Request:</span> {item.sourceTitle}</p>{/if}
                  <p class="m-0 whitespace-pre-wrap break-words">{item.detail || (item.state === 'paused' ? 'Paused by you. Resume to continue this task.' : item.state === 'unknown' ? 'No live status after reload. Open the worker to check its result.' : isWorkActive(item) ? 'Waiting for the next update from the worker.' : 'No additional output was reported.')}</p>
                  {#if item.delegateId && item.canReturn}
                    <div class="flex flex-wrap gap-2">
                      <ZButton zWave={false} zType="outline" zSize="sm" type="button" class="h-6! px-2! text-xs!" onclick={() => returnTaskResult(item.delegateId!)}>Return saved result</ZButton>
                    </div>
                  {/if}
                </div>
              </div>
            </div>
          </div>
        {/each}
      </div>
    </div>
  </div>
</section>
