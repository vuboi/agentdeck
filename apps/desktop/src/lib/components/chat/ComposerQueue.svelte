<script lang="ts">
  import { ZIcon, zSortable, zTooltip } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import {
    queued,
    queuePaused,
    pauseQueue,
    resumeQueue,
    clearQueue,
    steerQueued,
    type Queued,
  } from '../../chat/sessions.svelte';

  // Queued messages sit behind the composer until the agent is free; editing one hands it back to the composer.
  let {
    sessionId,
    waiting,
    waitingForUser,
    onedit,
  }: {
    sessionId: string;
    waiting: Queued[];
    waitingForUser: boolean;
    onedit: (item: Queued) => void;
  } = $props();

  // Queued messages are reordered by dragging their handle (zSortable: the rows glide, Escape cancels).
  function moveQueued(from: number, to: number) {
    if (from === to) return;
    const list = [...waiting];
    const [item] = list.splice(from, 1);
    list.splice(to, 0, item);
    queued[sessionId] = list;
  }
</script>

{#if waiting.length > 0 || queuePaused[sessionId]}
  <!-- Queued messages sit behind the composer until the agent is free and any open question or permission is resolved. -->
  <div
    class="mx-3 -mb-4 overflow-hidden rounded-t-2xl border border-b-0 border-border bg-card pb-4 shadow-md"
  >
    <!-- Outside the sortable list, so the status and clear action are not queue rows. -->
    <div
      class="flex items-center justify-between border-b border-border px-4 py-1 text-[0.6875rem] text-muted-foreground"
    >
      <span>{queuePaused[sessionId] === 'interrupted' ? 'Queue paused because you interrupted' : queuePaused[sessionId] ? 'Queue paused' : `${waiting.length} queued`}{waitingForUser ? ' · waiting for your answer' : ''}</span>
      <span class="flex items-center gap-1">
        <button
          type="button"
          class="inline-flex cursor-pointer items-center gap-1 rounded px-1.5 py-0.5 hover:bg-primary/10 hover:text-primary"
          aria-label={queuePaused[sessionId] ? 'Resume queue' : 'Pause queue'}
          onclick={() => queuePaused[sessionId] ? resumeQueue(sessionId) : pauseQueue(sessionId)}
        >
          <ZIcon zType={(queuePaused[sessionId] ? 'lucidePlay' : 'lucideSquarePause') as ZIconName} zSize="12" class="size-3" />
          <span>{queuePaused[sessionId] ? 'Resume' : 'Pause'}</span>
        </button>
        <button
          type="button"
          class="inline-flex cursor-pointer items-center gap-1 rounded px-1.5 py-0.5 hover:bg-destructive/10 hover:text-destructive"
          onclick={() => clearQueue(sessionId)}
        >
          <ZIcon zType={'lucideTrash2' as ZIconName} zSize="12" class="size-3" />
          <span>Clear all</span>
        </button>
      </span>
    </div>
    <div
      role="list"
      aria-label="Queued messages"
      {@attach zSortable({ onSort: moveQueued, handle: '[data-handle]' })}
    >
      {#each waiting as item (item)}
        <div
          class="flex items-center gap-2.5 border-t border-border bg-card px-4 py-1 text-sm first:border-t-0"
          role="listitem"
        >
          <!-- The list icon turns into a drag handle while the pointer is on it. -->
          <span
            data-handle
            class="group/handle grid size-3.5 shrink-0 cursor-grab touch-none place-items-center text-muted-foreground active:cursor-grabbing"
            aria-label="Drag to reorder"
          >
            <ZIcon
              zType={'lucideListChecks' as ZIconName}
              class="size-3.5 group-hover/handle:hidden"
            />
            <ZIcon
              zType={'lucideGripVertical' as ZIconName}
              class="hidden size-3.5 group-hover/handle:block"
            />
          </span>
          <span class="min-w-0 flex-1 truncate text-foreground"
            >{item.text ||
              (item.images.length
                ? 'Image'
                : item.files?.length
                  ? 'File'
                  : 'Element')}</span
          >
          {#if item.priority}<span class="shrink-0 text-[0.6875rem] text-primary">Next</span>{/if}
          <span class="flex shrink-0 items-center gap-0.5">
            <button
              type="button"
              disabled={waitingForUser}
              title={waitingForUser ? 'Answer the pending question or permission first' : 'Send this message now'}
              class="inline-flex shrink-0 cursor-pointer items-center gap-1 rounded px-1.5 py-0.5 text-xs text-muted-foreground hover:bg-accent hover:text-foreground disabled:cursor-not-allowed disabled:opacity-40"
              onclick={() => steerQueued(sessionId, item)}
            >
              <ZIcon zType={'lucideArrowRight' as ZIconName} class="size-3" />
              <span>Send now</span>
            </button>
            <button
              type="button"
              class="grid size-5 shrink-0 cursor-pointer place-items-center rounded text-muted-foreground hover:bg-accent hover:text-foreground"
              aria-label="Edit in the message box"
              {@attach zTooltip({
                content: 'Edit',
                position: 'top',
              })}
              onclick={() => onedit(item)}
            >
              <ZIcon
                zType={'lucideSquarePen' as ZIconName}
                zSize="13"
                class="size-[13px]"
              />
            </button>
            <button
              type="button"
              class="grid size-5 shrink-0 cursor-pointer place-items-center rounded text-muted-foreground hover:bg-accent hover:text-destructive"
              aria-label="Remove from the queue"
              onclick={() =>
                (queued[sessionId] = waiting.filter((entry) => entry !== item))}
            >
              <ZIcon
                zType={'lucideTrash2' as ZIconName}
                zSize="13"
                class="size-[13px]"
              />
            </button>
          </span>
        </div>
      {/each}
    </div>
  </div>
{/if}
