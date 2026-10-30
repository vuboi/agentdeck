<script lang="ts">
  import { answerPermission, answerQuestions, activeSession, chat, forkSession, isRunning, projectOf, replyBlockRendered, replyRendering, workspace } from '../../chat/sessions.svelte';
  import { fly } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { runPosition } from '../../chat/content/chat-dividers';
  import { runUsage } from '../../chat/runs/run-usage';
  import { elapsed, segments } from '../../chat/content/blocks';
  import { estimateCost } from '../../chat/runs/usage';
  import ArtifactCards from './ArtifactCards.svelte';
  import ChatBlock from './ChatBlock.svelte';
  import CommandGroup from './CommandGroup.svelte';
  import TurnChanges from './TurnChanges.svelte';
  import PlanCard from './PlanCard.svelte';
  import TaskCards from '../tasks/TaskCards.svelte';
  import AgentActivityCards from './AgentActivityCards.svelte';
  import LiveStatus from './LiveStatus.svelte';
  import MessageFooter from './MessageFooter.svelte';
  import ReplyHeader from './ReplyHeader.svelte';
  import { afterSteps, spokenRun } from './chat-helpers';
  import type { Message } from '../../chat/mock';

  /** `index` is the message's place in chat.messages; `questionMessageId` the reply the open question card belongs to. */
  let { message, index, agent, questionMessageId }: { message: Message; index: number; agent: string; questionMessageId: string | undefined } = $props();

  const parts = $derived(segments(message.blocks));
  // Replies of the same agent and model that follow one another (a divider between them) share one name line
  // and one footer: only the first carries the name, only the last the copy button.
  const place = $derived(runPosition(chat.messages, index, agent));
  const quiet = $derived(place.quiet);
  const grouped = $derived(place.grouped);
  const lastOfRun = $derived(place.lastOfRun);
  // The numbers on the name line are the whole run's: the replies a newer message cut off are in them too.
  const usage = $derived(grouped || quiet ? undefined : runUsage(chat.messages, index, estimateCost));
  // Live only while the chat really runs: a reply left marked live (a lost "done") shows its footer.
  const showLive = $derived(message.live && isRunning(workspace.activeId));
  // The footer (copy, fork, time) waits until the text has been written out on screen, not only until the run ends.
  const typing = $derived(!showLive && replyRendering(workspace.activeId, message.id));
</script>

{#if !quiet && !grouped}
  <ReplyHeader {message} {agent} {usage} />
{/if}
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
    {:else if segment.kind === 'agent_activity'}
      <AgentActivityCards items={segment.items} agent={message.agent ?? activeSession()?.source ?? activeSession()?.agent ?? 'codex'} />
    {:else}
      <ChatBlock block={segment.block} running={Boolean(message.live) && isRunning(workspace.activeId)} flush={chat.messages.at(-1)?.id !== message.id} agent={message.agent ?? activeSession()?.source ?? activeSession()?.agent} onanswer={(answer) => answerPermission(message.id, message.blocks.indexOf(segment.block), answer)} onquestions={(text) => answerQuestions(workspace.activeId, text)} onrendered={segment.block.kind === 'response' ? () => replyBlockRendered(workspace.activeId, message.id, message.blocks.indexOf(segment.block)) : undefined} answered={!message.live && questionMessageId !== message.id} showQuestions={false} />
    {/if}
  {/each}
  <TurnChanges blocks={message.blocks} projectRoot={(activeSession() && projectOf(activeSession()!)?.path) || ''} />
  <ArtifactCards blocks={message.blocks} root={(activeSession() && projectOf(activeSession()!)?.path) || null} live={Boolean(message.live) && isRunning(workspace.activeId)} />
  <!-- One status line for the whole reply: what the agent is doing now (thinking, running a command,
       editing…), as in Claude and Codex; the steps above stay plain, finished lines. -->
  {#if message.stoppedAfter !== undefined && !message.live}
    <!-- Rises into place as the reply ends, so the text above moves up with it instead of jumping. -->
    <p class="m-0 text-sm text-muted-foreground" in:fly={{ y: 8, duration: 260, easing: cubicOut }}>You stopped after {elapsed(message.stoppedAfter)}</p>
  {/if}
</div>
<!-- The status line and the footer share one fixed-height slot, so the row does not jump when the reply ends. -->
{#if showLive || ((message.blocks.length > 0 || message.stoppedAfter !== undefined) && lastOfRun)}
  <div class="relative h-[30px] w-full shrink-0">
    {#if showLive}
      <!-- Before any content, match the first response line's gap and line height below the agent header. -->
      <div class="absolute inset-x-0 flex items-center {parts.length ? 'bottom-0 h-5' : 'top-0 [&_[role=status]]:leading-relaxed'}"><LiveStatus {message} /></div>
    {:else if typing}
      <!-- Nothing yet: the slot keeps its height. -->
    {:else}
      <MessageFooter {message} onfork={() => forkSession(workspace.activeId, message.id)} speak={spokenRun(index)} />
    {/if}
  </div>
{/if}
