import { connectors, agentHasRoom, limitResetAt } from '../../agents/connectors.svelte';
import { type FailureKind, type Recovery, classifyFailure, planRecovery, continuePrompt, recoveryNote } from '../recovery/turn-failure';
import { inTauri, clock } from '../../app/platform/history';
import { type Message, type Block } from '../mock';
import { modelLabel } from '../../agents/models.svelte';
import { getPref, setPref } from '../../app/platform/prefs';
import { parseResetAt } from '../recovery/limit-reset';
import { type AgentEvent, listenAgentEvents } from '../agent';
import { logError } from '../../app/platform/error-log.svelte';
import { logFlow } from '../../tasks/flow-log';
import { trackActivity } from '../runs/activity.svelte';
import { guide } from '../../app/ui/guide.svelte';
import { addUsage, applyAgentEvent } from '../agent-reply';
import { finishCommandImages, persistCommandImage } from '../content/command-images';
import { recordUsage, estimateCost } from '../runs/usage';
import { toolKindOf } from '../../tasks/delegate-task';
import { archiveTurn } from '../content/archive';
import { MODELS, advanceRun, nextId, persistLocal, runEpochs, workspace, type Agent, type SessionItem } from './state.svelte';
import { agentLabel, continuationGroups, failSessionWork, hiddenPrompt, isRunning, openServedPage, queued, turns } from './turns.svelte';
import { NO_HANDOUT, RENDER_WAIT_LIMIT_MS, liveAsks, liveHandouts, liveReplies, messagesOf, needsUserInput, renderWaiters, replies, runReplies, sendNextQueued, sessionLanes, updateLane } from './run.svelte';
import { onDelegateDone } from './delegation.svelte';
import { bindTasks, delegateTasks, endWaitingTasks } from './delegate-task.svelte';
import { clipText, handleSideEvent, handoffFor, hasSideTurn, onHandbackDone, send, sideTurns } from './sending.svelte';
import { importHistory, keepOtherTurn, rememberMixed, transcripts } from './history.svelte';

// ---------- keeping work going when a turn fails ----------
// A turn that ends in an error is not the end of the work: a passing fault is tried again, and otherwise (a usage
// limit, a sign-out, a crash) the next agent of the user's chain, another provider, takes over the chat (it is told
// what the others did, see `missedTurns`). Accounts are never switched here: that stays a manual step of the user. The decision is `planRecovery` (turn-failure.ts); the chat shows
// one line for each step. A new message of the user starts the count again.
type RecoveryState = { retries: number; tried: string[] };
export const recoveries: Record<string, RecoveryState> = {};
const CLI_OF: Record<string, string> = { claude: 'claude', codex: 'codex', antigravity: 'agy', opencode: 'opencode', copilot: 'gh' };

/** The agents a chat can move to now, in the user's order: installed, with models, not at their limit. */
export function recoveryChain(): string[] {
  return connectors.recovery.chain.filter((agent) => {
    const tool = CLI_OF[agent];
    if (!tool) return false;
    const installed = connectors.detected?.tools.find((item) => item.id === tool)?.path !== null;
    return installed && (MODELS[agent as Agent] ?? []).length > 0 && agentHasRoom(agent);
  });
}

/** What to do about a turn of `session` that ended with `message`; null when nothing is (off, or stopped by the user). */
function recoveryFor(session: SessionItem, message: string): { kind: FailureKind; plan: Recovery } | null {
  if (!connectors.recovery.enabled || !inTauri()) return null;
  const kind = classifyFailure(message);
  if (kind === 'stopped') return null;
  const state = (recoveries[session.id] ??= { retries: 0, tried: [] });
  if (!state.tried.includes(session.agent)) state.tried.push(session.agent);
  const plan = planRecovery({
    kind,
    retries: state.retries,
    chain: recoveryChain(),
    current: session.agent,
    tried: state.tried,
  });
  return { kind, plan };
}

/**
 * The CLI answered (part of) a turn with another model than the chat's. A fallback of the whole session ("session" scope,
 * or none: older CLIs only had that) is followed: the chat's model becomes the one now serving, so the picker shows the
 * truth and the next turn does not switch back. A "local" one (a sub-agent, a side question) leaves the chat's model as it
 * is. Either way the reply gets one line saying what happened.
 */
function noteModelFallback(session: SessionItem, reply: Message, line: string) {
  let data: Record<string, unknown>;
  try {
    data = JSON.parse(line) as Record<string, unknown>;
  } catch {
    return;
  }
  const text = (key: string, camel: string) => (typeof data[key] === 'string' ? data[key] : typeof data[camel] === 'string' ? data[camel] : '') as string;
  const to = text('fallback_model', 'fallbackModel');
  const from = text('original_model', 'originalModel');
  const local = data.scope === 'local';
  // Only a model the picker knows is taken over: `modelFor` would replace any other one with the agent's first model.
  const known = (MODELS[session.agent] ?? []).some((item) => item.value === to);
  const moves = !local && data.direction !== 'revert' && known;
  if (moves && session.model !== to) {
    session.model = to;
    session.conversationModel = to;
    window.dispatchEvent(new CustomEvent('agentdeck:agent-moved', { detail: { sessionId: session.id, agent: session.agent, model: to } }));
  }
  const said = text('content', 'content').trim();
  const note =
    said ||
    (local
      ? `Only that response came from ${modelLabel(to)}; the chat stays on ${modelLabel(from)}.`
      : `${modelLabel(from) || 'The model'} could not answer: the chat continues on ${modelLabel(to)}.`);
  reply.blocks = [...reply.blocks, { kind: 'response', text: `> ↻ ${note}` }];
}

/** Moves a chat to another agent (its first model), as picking it in the composer would. */
export function moveChatTo(session: SessionItem, agent: Agent) {
  const model = MODELS[agent]?.[0]?.value ?? session.model;
  if (session.agent !== agent && !session.draft) {
    session.agents = [...new Set([...(session.agents ?? [session.source ?? session.agent]), agent])];
    rememberMixed(session);
  }
  session.agent = agent;
  session.model = model;
  // A worker chat that moved on (after a failure) is handled by that agent now: the task says so, so its card and its
  // report name the agent that answers.
  const task = Object.values(delegateTasks).find((record) => record.childId === session.id);
  if (task) {
    task.agent = agent;
    task.agentId = agent;
    task.model = model;
    task.label = `${agentLabel(agent, model)} · ${modelLabel(model)}`;
  }
  // The composer keeps a picker per chat: it follows, or it would put the old agent back.
  window.dispatchEvent(new CustomEvent('agentdeck:agent-moved', { detail: { sessionId: session.id, agent, model } }));
}

/** Carries out the plan after the failed turn has been put away. */
async function runRecovery(sessionId: string, plan: Recovery, reason: string) {
  const session = workspace.sessions.find((item) => item.id === sessionId);
  const state = recoveries[sessionId];
  if (!session || !state || plan.action === 'stop') return;
  if (plan.action === 'retry') {
    state.retries++;
    await new Promise((resolve) => setTimeout(resolve, plan.delayMs));
    forgetUnstartedConversation(session);
  } else {
    moveChatTo(session, plan.agent as Agent);
    state.retries = 0;
    state.tried.push(plan.agent);
  }
  // The user may have sent something meanwhile, or stopped the chat: their message wins. A newer failure has its own
  // state, so this older plan is dropped too.
  if (replies[sessionId] || recoveries[sessionId] !== state) return;
  const same = plan.action !== 'fallback';
  send(sessionId, '', [], [], [], { prompt: continuePrompt(reason, same, lastRequest(sessionId)), silent: NO_HANDOUT });
}

// ---------- carrying on when a usage limit ends ----------
// A turn that ends at a usage limit with no other agent to move to waits for the limit to reset (its time is in the error,
// or in the account's usage windows) and then carries on by itself, as Claude Code does. The chat shows a card with the
// time, a switch to turn the automatic part off, and Try again / Cancel. A message of the user cancels it: they took over.
export type Resume = { sessionId: string; agent: string; at: number; reason: string; auto: boolean };
export const resumes = $state<Record<string, Resume>>({});
const RESUMES_KEY = 'chat.resumes';
/** A little after the reset, so the first request is not refused for being early. */
const RESUME_BUFFER_MS = 60_000;
const RESUME_MAX_WAIT_MS = 8 * 86_400_000;
const RESUME_MAX_AGE_MS = 36 * 3_600_000;
const resumeStore = { ready: false };
/** Automatic resumes made since the user last wrote: a limit that is still on after three of them stops the loop. */
export const resumeTries: Record<string, number> = {};
const MAX_RESUME_TRIES = 3;
void getPref<Resume[]>(RESUMES_KEY)
  .then((saved) => {
    for (const item of Array.isArray(saved) ? saved : []) if (!resumes[item.sessionId] && item.at + RESUME_MAX_AGE_MS > Date.now()) resumes[item.sessionId] = item;
  })
  .catch(() => undefined)
  .finally(() => {
    resumeStore.ready = true;
  });
$effect.root(() => {
  $effect(() => {
    if (!resumeStore.ready) return;
    setPref(RESUMES_KEY, JSON.parse(JSON.stringify(Object.values(resumes))) as Resume[]);
  });
});

function scheduleResume(session: SessionItem, message: string) {
  const now = Date.now();
  const end = parseResetAt(message, now) ?? limitResetAt(session.agent, now);
  if (end === null || end - now > RESUME_MAX_WAIT_MS || (resumeTries[session.id] ?? 0) >= MAX_RESUME_TRIES) return;
  resumes[session.id] = {
    sessionId: session.id,
    agent: session.agent,
    at: Math.max(end, now) + RESUME_BUFFER_MS,
    reason: 'the usage limit was reached',
    auto: connectors.recovery.autoResume,
  };
}

/** Carries on now (the "Try again" button, or the time has come). */
export function resumeNow(sessionId: string) {
  const resume = resumes[sessionId];
  delete resumes[sessionId];
  if (resume) resumeTries[sessionId] = (resumeTries[sessionId] ?? 0) + 1;
  const session = workspace.sessions.find((item) => item.id === sessionId);
  if (!resume || !session || replies[sessionId]) return;
  send(sessionId, '', [], [], [], { prompt: continuePrompt(resume.reason, true, lastRequest(sessionId)), silent: NO_HANDOUT });
}
export const cancelResume = (sessionId: string) => void delete resumes[sessionId];
export function setResumeAuto(sessionId: string, auto: boolean) {
  const resume = resumes[sessionId];
  if (resume) resume.auto = auto;
}

function checkResumes() {
  const now = Date.now();
  for (const resume of Object.values(resumes)) {
    if (resume.at + RESUME_MAX_AGE_MS < now) {
      delete resumes[resume.sessionId];
      continue;
    }
    if (!resume.auto || resume.at > now) continue;
    const session = workspace.sessions.find((item) => item.id === resume.sessionId);
    // Wait for the chat to be loaded and idle: a turn that is running will have its own end.
    if (!session || isRunning(resume.sessionId) || replies[resume.sessionId]) continue;
    resumeNow(resume.sessionId);
  }
}
if (typeof window !== 'undefined') setInterval(checkResumes, 10_000);

/** The words of the user's latest message, for a turn that carries on without it. */
function lastRequest(sessionId: string): string {
  const asked = [...messagesOf(sessionId)].reverse().find((message) => message.role === 'user' && !hiddenPrompt(message));
  return asked?.blocks.flatMap((block) => (block.kind === 'response' ? [block.text] : [])).join('\n').trim() ?? '';
}

/**
 * A turn that failed before the CLI ever answered leaves an id with no conversation behind it (or, for Claude, one
 * it may refuse to reuse with `--session-id`). The retry then starts a new conversation instead.
 */
function forgetUnstartedConversation(session: SessionItem) {
  const first = session.source ?? session.agents?.[0] ?? session.agent;
  if (session.agent === first) {
    if (session.agentStarted === true || session.path) return;
    session.agentSession = undefined;
    session.agentStarted = false;
  } else if (session.agentIds?.[session.agent] && !session.agentIds[session.agent]?.started) {
    delete session.agentIds[session.agent];
  }
}

/** Apply one agent event to the reply it belongs to. */
/** Where a problem happened, for the error log (error-log.svelte.ts). No text of the conversation. */
export function logCtx(sessionId: string, session?: SessionItem) {
  const found = session ?? workspace.sessions.find((item) => item.id === sessionId);
  return {
    chat: sessionId,
    agent: found?.agent,
    model: found?.model,
    parent: found?.parentId,
    step: found?.title && found.parentId ? found.title.split(':')[0] : undefined,
  };
}

// A turn that says nothing for this long is written to the log once (an agent that hangs, a tool that never returns,
// a member waiting on a message that never comes): that is the line to look for when the app seems frozen.
const SILENT_AFTER_MS = 5 * 60_000;
export const lastEventAt = new Map<string, number>();
export const lastEventKind = new Map<string, string>();
const silentLogged = new Set<string>();
// After this long the reply itself says so, with what it waits for: a chat that looks frozen is one the user can stop.
const SILENT_SHOWN_MS = 2 * 60_000;
const SILENT_PREFIX = 'No output for ';
const isSilenceNote = (block: Block | undefined) => block?.kind === 'loading' && block.label.startsWith(SILENT_PREFIX);

/** What the quiet reply is waiting for, from the last thing it did. */
function waitingOn(kind: string | undefined): string {
  if (!kind) return 'the agent';
  if (/^tool_call (Agent|Task)$/.test(kind)) return 'a sub-agent';
  if (kind.startsWith('tool_call ')) return `the ${kind.slice('tool_call '.length)} tool`;
  if (kind === 'task') return 'a sub-agent or background job';
  return 'the agent';
}

function noteSilence(sessionId: string, session: SessionItem, event: AgentEvent) {
  if (event.kind === 'raw') return;
  // The "no output" line goes as soon as anything is heard again (the reducer clears it on most events; usage does not).
  const quiet = liveReplies[sessionId];
  if (quiet && isSilenceNote(quiet.blocks.at(-1))) quiet.blocks = quiet.blocks.slice(0, -1);
  lastEventAt.set(sessionId, Date.now());
  lastEventKind.set(sessionId, event.kind === 'tool_call' ? `tool_call ${event.name}` : event.kind);
  silentLogged.delete(sessionId);
  if (event.kind === 'error') logError('turn', 'turn failed', event.message, logCtx(sessionId, session));
  else if (event.kind === 'tool_result' && event.is_error) logError('mcp', 'tool failed', event.output.slice(0, 600), logCtx(sessionId, session));
}
if (typeof window !== 'undefined') {
  setInterval(() => {
    const now = Date.now();
    for (const [id, at] of lastEventAt) {
      if (!replies[id]) {
        lastEventAt.delete(id);
        lastEventKind.delete(id);
        silentLogged.delete(id);
        continue;
      }
      if (now - at > SILENT_SHOWN_MS) {
        const live = liveReplies[id];
        if (live?.live) {
          const label = `${SILENT_PREFIX}${Math.round((now - at) / 60_000)} min: waiting on ${waitingOn(lastEventKind.get(id))}. Press Stop to cancel.`;
          const last = live.blocks.at(-1);
          if (last && isSilenceNote(last) && last.kind === 'loading') last.label = label;
          else live.blocks = [...live.blocks, { kind: 'loading', label }];
        }
      }
      if (now - at > SILENT_AFTER_MS && !silentLogged.has(id)) {
        silentLogged.add(id);
        logFlow(id, 'silent', `no event for ${Math.round((now - at) / 1000)} s, last: ${lastEventKind.get(id) ?? '?'}`);
        logError('turn', `no event for ${Math.round((now - at) / 60_000)} min`, `last: ${lastEventKind.get(id) ?? '?'}`, logCtx(id));
      }
    }
  }, 30_000);
}

/** An agent event as the reducer's events (see run-state.ts). Thinking and usage do not move the run. */
function recordAgentEvent(sessionId: string, event: AgentEvent) {
  switch (event.kind) {
    case 'delta':
      return advanceRun(sessionId, { type: 'output', text: event.text });
    case 'tool_result':
    case 'tool_output_delta':
      return advanceRun(sessionId, { type: 'output' });
    case 'permission_denied':
      for (let index = 0; index < event.requests.length; index++) advanceRun(sessionId, { type: 'waitStart', kind: 'approval' });
      return;
    case 'done':
      advanceRun(sessionId, { type: 'output', text: event.text });
      return advanceRun(sessionId, { type: 'turnResult', isError: false });
    case 'error':
      return advanceRun(sessionId, { type: 'turnResult', isError: true, message: event.message });
    default:
      return;
  }
}

/**
 * The CLI went on by itself (a sub-agent or a background job ended and it reports on it): that is a new reply in the
 * chat with no message of the user above it. A CLI that says so while a reply is already live (Copilot, mid-turn) just
 * goes on writing that reply.
 */
function beginAutoTurn(sessionId: string, turn: string, messageId?: string) {
  const session = workspace.sessions.find((item) => item.id === sessionId);
  if (!session || liveReplies[sessionId]?.live || hasSideTurn(sessionId)) return;
  const list = messagesOf(sessionId);
  const previous = list.at(-1);
  const runGroup = continuationGroups[sessionId] ?? (previous?.role === 'assistant' ? previous.runGroup : undefined) ?? nextId();
  if (previous?.role === 'assistant') previous.runGroup = runGroup;
  continuationGroups[sessionId] = runGroup;
  list.push({
    id: nextId(),
    role: 'assistant',
    time: clock(new Date().toISOString()),
    at: new Date().toISOString(),
    blocks: [],
    live: true,
    startedAt: Date.now(),
    agent: session.agent,
    model: session.model,
    autoTurn: true,
    triggeredByMessageId: messageId ?? (previous?.role === 'assistant' ? previous.replyTo ?? previous.triggeredByMessageId : undefined),
    runGroup,
  });
  const reply = list[list.length - 1];
  liveReplies[sessionId] = reply; // the stored (reactive) object
  replies[sessionId] = reply.id;
  turns[sessionId] = turn;
  session.status = 'working';
  const epoch = (runEpochs[sessionId] = (runEpochs[sessionId] ?? 0) + 1);
  advanceRun(sessionId, { type: 'spawn', epoch });
  advanceRun(sessionId, { type: 'turnStarted', epoch });
}

export function handleAgentEvent(
  sessionId: string,
  event: AgentEvent,
  turn?: string,
  messageId?: string,
) {
  const ownedRun = turn ? runReplies.get(turn) : undefined;
  const startsTailReply = event.kind === 'auto_turn' && turn?.startsWith('auto-');
  if (turn !== undefined && turns[sessionId] !== turn && !ownedRun && !startsTailReply) {
    // Late auto-turn markers are stale too: they must not start a reply after a newer user message.
    if (event.kind === 'done' || event.kind === 'error' || event.kind === 'auto_turn') {
      logFlow(sessionId, 'event dropped', `turn=${turn} is not the current turn (${turns[sessionId] ?? '-'}) kind=${event.kind}`);
    }
    return;
  }
  if (event.kind === 'auto_turn') {
    logFlow(sessionId, 'auto turn', `turn=${turn ?? '-'} ${event.reason}`);
    if (turn !== undefined) beginAutoTurn(sessionId, turn, messageId);
    return;
  }
  if (ownedRun) {
    if (event.kind === 'tool_call' && event.background) {
      ownedRun.backgroundTools.add(event.id);
      updateLane(sessionId, { id: turn!, role: ownedRun.listener ? 'listener' : 'primary', state: 'waiting', backgroundWork: true });
    } else if (event.kind === 'tool_result' && ownedRun.backgroundTools.delete(event.id)) {
      updateLane(sessionId, { id: turn!, role: ownedRun.listener ? 'listener' : 'primary', state: ownedRun.backgroundTools.size ? 'waiting' : 'running', backgroundWork: ownedRun.backgroundTools.size > 0 });
    } else if (event.kind === 'task') {
      if (event.status === 'started' || event.status === 'progress') {
        ownedRun.backgroundTools.add(event.id);
        if (event.tool_use_id) ownedRun.backgroundTools.add(event.tool_use_id);
      } else {
        ownedRun.backgroundTools.delete(event.id);
        if (event.tool_use_id) ownedRun.backgroundTools.delete(event.tool_use_id);
      }
      updateLane(sessionId, { id: turn!, role: ownedRun.listener ? 'listener' : 'primary', state: ownedRun.backgroundTools.size ? 'waiting' : 'running', backgroundWork: ownedRun.backgroundTools.size > 0 });
    }
  }
  const currentReply = ownedRun?.reply ?? liveReplies[sessionId];
  const expectedMessage = currentReply?.autoTurn ? currentReply.triggeredByMessageId : currentReply?.replyTo;
  if (messageId && expectedMessage && messageId !== expectedMessage) {
    logFlow(sessionId, 'event dropped', `turn=${turn ?? '-'} message=${messageId} does not belong to reply ${currentReply?.id ?? '-'}`);
    return;
  }
  if (event.kind === 'done' || event.kind === 'error') logFlow(sessionId, 'reply end', `turn=${turn ?? '-'} ${event.kind}${event.kind === 'error' ? ` ${event.message.slice(0, 120)}` : ''} live=${Boolean(liveReplies[sessionId]?.live)} queued=${queued[sessionId]?.length ?? 0}`);
  recordAgentEvent(sessionId, event);
  const session = workspace.sessions.find((item) => item.id === sessionId);
  // The live reply is kept by reference: the chat list can be swapped under it (another chat opened, a transcript
  // reloaded) and the turn must still finish, or the chat would spin for ever.
  const reply =
    ownedRun?.reply ?? liveReplies[sessionId] ??
    messagesOf(sessionId).find((item) => item.id === replies[sessionId]);
  if (!session) return;
  // Codex writes a thread's rollout, and Claude its session file, only once a run has started. A run stopped before
  // that leaves an id with nothing to resume. The next message then starts a new conversation, instead of resuming
  // that id again and failing the same way.
  if (
    !ownedRun?.listener &&
    event.kind === 'error' &&
    /no rollout found|no conversation found/i.test(event.message)
  ) {
    // The CLI no longer has the conversation this id named (a thread whose run was cut short, a cleaned-up session).
    // The next turn starts a new one, and is given what was said so far as text, so the work in progress is not lost.
    session.agentStarted = false;
    session.agentSession = undefined;
    session.forkContext = (session.forkContext ?? '') + handoffFor(sessionId, lastRequest(sessionId));
  }
  trackActivity(
    sessionId,
    session.title,
    workspace.projects.find((project) => project.id === session.projectId)
      ?.name ?? '',
    event,
  );
  noteSilence(sessionId, session, event);
  const documentTask = guide.tasks[sessionId];
  if (documentTask?.status === 'working') {
    if (event.kind === 'tool_call') documentTask.progress = `Agent is running ${event.name}`;
    if (event.kind === 'tool_result') documentTask.progress = event.is_error
      ? `Tool failed: ${event.output.slice(0, 300)}`
      : `Finished ${documentTask.progress.replace(/^Agent is running /, '')}`;
  }
  // A line of the CLI the chat does not show: it is only kept in the Audit log. A model fallback is the one it does show.
  if (event.kind === 'raw') {
    if (reply && event.source === 'claude' && /^model_(refusal_)?fallback$/.test(event.label)) noteModelFallback(session, reply, event.line);
    return;
  }
  if (!reply) {
    if (event.kind === 'done' || event.kind === 'error') {
      delete replies[sessionId];
      session.status = hasSideTurn(sessionId) ? 'working' : 'idle';
    }
    return;
  }
  if (event.kind === 'usage') {
    reply.model ??= session.model;
    addUsage(reply, event);
    return;
  }
  if (event.kind === 'done' && event.cost_usd != null)
    reply.cost = (reply.cost ?? 0) + event.cost_usd;
  const update = applyAgentEvent(reply.blocks, event);
  reply.blocks = update.blocks;
  if (ownedRun?.listener) {
    if (!update.finished) return;
    reply.live = false;
    if (reply.startedAt) reply.ms = Date.now() - reply.startedAt;
    updateLane(sessionId, { id: turn!, role: 'listener', state: event.kind === 'error' ? 'failed' : 'done', backgroundWork: false });
    runReplies.delete(turn!);
    void finishCommandImages(sessionId, reply);
    if (session.path) keepOtherTurn(session, reply);
    if (reply.tokens) recordUsage({
      at: new Date().toISOString(), sessionId, agent: reply.agent ?? session.agent,
      model: reply.model ?? session.model, projectId: session.projectId, ms: reply.ms,
      tokens: { ...reply.tokens }, cost: reply.cost ?? estimateCost(reply.tokens, reply.model), estimated: reply.cost == null,
    });
    persistLocal();
    session.status = replies[sessionId] || (sessionLanes[sessionId]?.length ?? 0) > 0 ? 'working' : 'idle';
    sendNextQueued(sessionId);
    return;
  }
  // An auto-compress turn shows one line: what `/compact` prints is not an answer.
  if (reply.compressed && event.kind !== 'error')
    reply.blocks = event.kind === 'done'
      ? [{ kind: 'response', text: '*Context compressed: the conversation goes on from a summary of what came before.*' }]
      : [{ kind: 'loading', label: 'Compressing context…' }];
  if (event.kind === 'tool_result' && event.image)
    persistCommandImage(sessionId, reply, event.id, event.image);
  // A delegate_task call just showed up: its card follows the task the call starts.
  if (event.kind === 'tool_call' && toolKindOf(event.name) === 'delegate_task')
    bindTasks(sessionId);
  // The CLI has the conversation once it answers; only then is `--resume` valid for the next turn.
  const first = session.source ?? session.agents?.[0] ?? session.agent;
  const agent = reply.agent ?? session.agent;
  if (agent === first) {
    if (event.kind !== 'error') session.agentStarted = true;
    if (update.agentSession) session.agentSession = update.agentSession;
  } else if (
    event.kind !== 'error' &&
    (update.agentSession || session.agentIds?.[agent])
  ) {
    const id = update.agentSession ?? session.agentIds?.[agent]?.id;
    // The rest of the entry (the account epoch) stays: only the id and its start change here.
    if (id) (session.agentIds ??= {})[agent] = { ...session.agentIds?.[agent], id, started: true };
  }
  // A failed turn that can be carried on (retry, another account, the next agent) does not end the work: the chat
  // gets a line saying what happens, and the steps below that close the work (failure, reports) are skipped.
  const recovery =
    update.finished && event.kind === 'error' ? recoveryFor(session, event.message) : null;
  const recovering = recovery !== null && recovery.plan.action !== 'stop';
  if (update.finished && event.kind !== 'error') delete recoveries[sessionId];
  // A limit with nowhere to move: wait for it to reset, then carry on (see `resumes`).
  if (update.finished && event.kind === 'error' && !recovering && classifyFailure(event.message) === 'limit') scheduleResume(session, event.message);
  if (recovery) {
    const plan = recovery.plan;
    const note = recoveryNote(recovery.kind, plan, {
      from: agentLabel(session.agent, session.model),
      to: plan.action === 'fallback' ? agentLabel(plan.agent as Agent) : undefined,
    });
    reply.blocks = [...reply.blocks, { kind: 'response', text: `> ↻ ${note}` }];
  }
  if (update.finished) {
    updateLane(sessionId, { id: turn!, role: 'primary', state: event.kind === 'error' ? 'failed' : 'done', backgroundWork: false });
    runReplies.delete(turn!);
    if (documentTask?.status === 'working' && !recovering) {
      documentTask.status = event.kind === 'error' ? 'error' : 'done';
      documentTask.progress = event.kind === 'error' ? 'Document task failed' : 'Document task finished';
      documentTask.result = event.kind === 'error'
        ? event.message
        : event.kind === 'done'
          ? event.text || reply.blocks.flatMap((block) => block.kind === 'response' ? [block.text] : []).join('\n').trim()
          : reply.blocks.flatMap((block) => block.kind === 'response' ? [block.text] : []).join('\n').trim();
      documentTask.outputPath = documentTask.result.match(/(?:[A-Za-z]:\\|\\\\|\/|docs[\\/])[^\n\r"'<>]*?\.(?:docx|pdf|pptx|webm|html|md)\b/i)?.[0];
    }
    // The chat is released in `finally`: a step here that throws must not leave it working for ever.
    try {
      if (event.kind === 'error' && !recovering) failSessionWork(sessionId);
      reply.live = false;
      void finishCommandImages(sessionId, reply);
      if (
        session.path &&
        session.source &&
        reply.agent &&
        reply.agent !== session.source
      ) {
        keepOtherTurn(session, reply);
        rememberMixed(session);
      }
      if (reply.startedAt) reply.ms = Date.now() - reply.startedAt;
      if (reply.tokens) {
        const estimate = estimateCost(reply.tokens, reply.model);
        recordUsage({
          at: new Date().toISOString(),
          sessionId,
          agent: session.agent,
          model: reply.model,
          projectId: session.projectId,
          ms: reply.ms,
          tokens: { ...reply.tokens },
          cost: reply.cost ?? estimate,
          estimated: reply.cost == null,
        });
      }
      openServedPage(sessionId, reply);
      // A worker or a hand-back that is being carried on is not over: its waiter gets the turn that finishes it.
      if (!recovering) {
        onDelegateDone(
          sessionId,
          reply,
          event.kind === 'error' ? event.message : null,
        );
        onHandbackDone(sessionId, reply, event.kind === 'error');
        endWaitingTasks(sessionId);
      }
      // Our own copy of the turn (agent, model, time), apart from what the CLIs store: the base for syncing later.
      archiveTurn(
        {
          id: sessionId,
          title: session.title,
          agent: session.agent,
          model: reply.model ?? session.model,
          projectPath:
            workspace.projects.find(
              (project) => project.id === session.projectId,
            )?.path ?? null,
        },
        [liveAsks[sessionId], reply].filter((message): message is Message =>
          Boolean(message),
        ),
      );
    } finally {
      delete replies[sessionId];
      delete liveReplies[sessionId];
      delete liveAsks[sessionId];
      delete liveHandouts[sessionId];
      session.status = hasSideTurn(sessionId) || (sessionLanes[sessionId]?.length ?? 0) > 0 ? 'working' : 'idle';
      persistLocal();
      // The CLI wrote the conversation to disk: pick it up so the row gets its path, in place (same id).
      // The cached transcript of this chat is now behind the file: a new time makes the next open read it again.
      session.updatedAt = new Date().toISOString();
      transcripts.delete(sessionId);
      if (!session.path) void importHistory({ refresh: true });
      // The final Markdown animation can outlast the CLI turn. Keep queued work behind the last rendered response.
      const responseBlocks = reply.blocks.flatMap((block, index) =>
        block.kind === 'response' && block.text ? [index] : [],
      );
      if (recovering && recovery && event.kind === 'error')
        void runRecovery(sessionId, recovery.plan, clipText(event.message, 200));
      else if (
        sessionId === workspace.activeId &&
        responseBlocks.length &&
        !needsUserInput(sessionId)
      ) {
        renderWaiters[sessionId] = {
          messageId: reply.id,
          blocks: new Set(responseBlocks),
        };
        // A block that is not on screen (scrolled out of the virtual list, a block that threw) never reports: the queue
        // behind this reply must not wait for it for ever.
        const waitingFor = renderWaiters[sessionId];
        logFlow(sessionId, 'render wait', `reply ${reply.id}: queue waits for ${responseBlocks.length} block(s) to draw`);
        setTimeout(() => {
          if (renderWaiters[sessionId] !== waitingFor) return;
          delete renderWaiters[sessionId];
          logFlow(sessionId, 'render wait timed out', `reply ${reply.id} was not drawn in ${RENDER_WAIT_LIMIT_MS} ms: the queue goes on`);
          sendNextQueued(sessionId);
        }, RENDER_WAIT_LIMIT_MS);
      } else sendNextQueued(sessionId);
    }
  }
}

if (inTauri()) {
  // A fast stream sends dozens of deltas a second; each one re-rendered the chat. Apply them once per frame.
  let queue: { session: string; turn: string; message_id?: string; event: AgentEvent }[] = [];
  const flush = () => {
    if (!queue.length) return; // already applied by the other of the two triggers
    const batch = queue;
    queue = [];
    for (const { session, turn, message_id, event } of batch) {
      // One event that throws must not drop the rest of the frame: a `done` behind it (of this chat or of another
      // running at the same time) would be lost, and that chat would show it is working for ever.
      try {
        if (sideTurns.has(session)) handleSideEvent(session, event, turn);
        else handleAgentEvent(session, event, turn, message_id);
      } catch (error) {
        console.error('agent event failed', session, event.kind, error);
      }
    }
  };
  void listenAgentEvents((envelope) => {
    if (queue.push(envelope) !== 1) return;
    // A hidden or minimised window gets no animation frames: the timer applies the events then, so a turn still
    // ends (and its queue, its notification and the agents waiting on it go on) while nobody looks.
    requestAnimationFrame(flush);
    setTimeout(flush, 250);
  });
}