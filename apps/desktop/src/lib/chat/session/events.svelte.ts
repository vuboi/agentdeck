import { connectors, agentHasRoom, autoSwitchConnectorAccount, autoSwitchProviderKey, limitResetAt, refreshProviderQuota, type ConnectorId } from '../../agents/connectors.svelte';
import { type FailureKind, type Recovery, classifyFailure, planRecovery, continuePrompt, recoveryNote } from '../recovery/turn-failure';
import { inTauri, isPreviewCopy, clock } from '../../app/platform/history';
import { type Message, type Block } from '../mock';
import { modelLabel } from '../../agents/models.svelte';
import { getPref, setPref } from '../../app/platform/prefs';
import { parseResetAt } from '../recovery/limit-reset';
import { classifyLimitGate, type LimitGateKind } from '../recovery/limit-gate';
import { type AgentEvent, listenAgentEvents } from '../agent';
import { logError } from '../../app/platform/error-log.svelte';
import { hangTraceOn, logFlow, markStep } from '../../tasks/flow-log';
import { activity, trackActivity } from '../runs/activity.svelte';
import { guide } from '../../app/ui/guide.svelte';
import { addUsage, applyAgentEvent, updateHistoricalTask } from '../agent-reply';
import { finishCommandImages, persistCommandImage } from '../content/command-images';
import { recordUsage, estimateCost } from '../runs/usage';
import { toolKindOf } from '../../tasks/delegate-task';
import { archiveTurn } from '../content/archive';
import { MODELS, advanceRun, nextId, persistLocal, runEpochs, workspace, type Agent, type SessionItem } from './state.svelte';
import { agentLabel, continuationGroups, failSessionWork, hiddenPrompt, isRunning, openServedPage, queued, stopTurn, turns } from './turns.svelte';
import { NO_HANDOUT, RENDER_WAIT_LIMIT_MS, askText, liveAsks, liveHandouts, liveReplies, messagesOf, needsUserInput, renderWaiters, replies, runReplies, sendNextQueued, sessionLanes, updateLane } from './run.svelte';
import { finishWorkflowTurn, onDelegateDone } from './delegation.svelte';
import { bindTasks, delegateTasks, endWaitingTasks, pauseDelegateTask, resumeDelegateTask, taskRuns } from './delegate-task.svelte';
import { handoffFor, send } from './sending.svelte';
import { clipText, taskWaiters } from './orchestration.svelte';
import { finishWorkflowIntegration } from './delegation.svelte';
import { importHistory, keepOtherTurn, rememberMixed, transcripts } from './history.svelte';
import { parseCustomModel } from '../../agents/models.svelte';
import type { CredentialAttempt } from '../../agents/credential-switch';
import { isBackgroundWait } from '../runs/run-lanes';

const credentialAttempts = new Map<string, CredentialAttempt & { controller: AbortController }>();
/** A user request or Stop invalidates every pending continuation for the previous task. */
export function cancelAutoSwitch(sessionId: string): void {
  credentialAttempts.get(sessionId)?.controller.abort();
  credentialAttempts.delete(sessionId);
}

/** A turn the user stopped can be continued from the composer without adding a visible user message. */
export const manuallyStopped = $state<Record<string, boolean>>({});

/** When the user last pressed Stop in a chat: a report the CLI sends right after that is a leftover of the stop, not news. */
export const userStoppedAt: Record<string, number> = {};
const STOP_REPORT_GRACE_MS = 30_000;

/** Stops the active turn and exposes the composer's one-click continuation action. */
export function stopTurnForResume(sessionId: string) {
  const task = Object.values(delegateTasks).find((item) => item.childId === sessionId && taskRuns.has(item.id) && ['queued', 'running'].includes(item.state));
  if (task) {
    pauseDelegateTask(task.id);
    return;
  }
  const waiter = taskWaiters.get(sessionId);
  taskWaiters.delete(sessionId);
  stopTurn(sessionId);
  if (waiter) taskWaiters.set(sessionId, waiter);
  manuallyStopped[sessionId] = true;
}

/** Continues a user-stopped turn with a hidden prompt that preserves the original request in context. */
export function resumeStoppedTurn(sessionId: string) {
  const task = Object.values(delegateTasks).find((item) => item.childId === sessionId && taskRuns.has(item.id) && item.pausedAt !== undefined);
  if (task) {
    void resumeDelegateTask(task.id);
    return;
  }
  if (!manuallyStopped[sessionId] || isRunning(sessionId)) return;
  delete manuallyStopped[sessionId];
  send(sessionId, '', [], [], [], {
    prompt: continuePrompt('the user stopped the reply', true, lastRequest(sessionId)),
    silent: NO_HANDOUT,
  });
}

// ---------- keeping work going when a turn fails ----------
// A turn that ends in an error is not the end of the work: a passing fault is tried again, and otherwise (a usage
// limit, a sign-out, a crash) the next agent of the user's chain, another provider, takes over the chat (it is told
// what the others did, see `missedTurns`). Accounts are never switched here: that stays a manual step of the user. The decision is `planRecovery` (turn-failure.ts); the chat shows
// one line for each step. A new message of the user starts the count again.
type RecoveryState = { retries: number; tried: string[] };
export const recoveries: Record<string, RecoveryState> = {};
const CLI_OF: Record<string, string> = { claude: 'claude', codex: 'codex', antigravity: 'agy', opencode: 'opencode', copilot: 'gh' };
/** What a provider's CLI says when the conversation an id names does not exist (a run stopped before it wrote anything,
 *  or a session cleaned up): Codex rollout, Claude session, OpenCode session, Antigravity conversation, Copilot thread. */
const NO_CONVERSATION = /no (rollout|conversation|session|thread) (was )?found|(conversation|session|thread) (not found|does not exist)|unknown (conversation|session|thread)/i;

/** The chain's rows are `agent|model` (a model picked in Settings) or a bare agent (the agent's first model). */
const chainAgent = (row: string) => row.split('|')[0];

/** The agents a chat can move to now, in the user's order: installed, with models, not at their limit. */
export function recoveryChain(): string[] {
  const agents = [...new Set(connectors.recovery.chain.map(chainAgent))];
  return agents.filter((agent) => {
    const tool = CLI_OF[agent];
    if (!tool) return false;
    const installed = connectors.detected?.tools.find((item) => item.id === tool)?.path !== null;
    return installed && (MODELS[agent as Agent] ?? []).length > 0 && agentHasRoom(agent);
  });
}

/** The model the chain picked for an agent (its first row with a model), or undefined for the agent's first model. */
export function recoveryModelFor(agent: string): string | undefined {
  const row = connectors.recovery.chain.find((item) => chainAgent(item) === agent && item.includes('|'));
  const model = row?.slice(row.indexOf('|') + 1);
  return model || undefined;
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
export function moveChatTo(session: SessionItem, agent: Agent, picked?: string) {
  const model = picked ?? MODELS[agent]?.[0]?.value ?? session.model;
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
    // The user may have sent something meanwhile: their turn runs now, so the conversation is not reset under it.
    if (replies[sessionId] || recoveries[sessionId] !== state) return;
    forgetUnstartedConversation(session);
  } else {
    moveChatTo(session, plan.agent as Agent, recoveryModelFor(plan.agent));
    state.retries = 0;
    state.tried.push(plan.agent);
  }
  // The user may have sent something meanwhile, or stopped the chat: their message wins. A newer failure has its own
  // state, so this older plan is dropped too.
  if (replies[sessionId] || recoveries[sessionId] !== state) return;
  const same = plan.action !== 'fallback';
  send(sessionId, '', [], [], [], { prompt: continuePrompt(reason, same, lastRequest(sessionId)), silent: NO_HANDOUT });
}

/** Rotates the credential for a limit/auth failure, then resumes the original request invisibly. */
async function runAutoSwitch(sessionId: string, reason: string, reply: Message, blockId: string) {
  const session = workspace.sessions.find((item) => item.id === sessionId);
  if (!session) return;
  const scope = session.agent === 'custom' ? parseCustomModel(session.model)?.providerId : session.agent === 'antigravity' ? 'agy' : session.agent;
  const expectedModel = session.model;
  const modelId = session.agent === 'custom' ? parseCustomModel(session.model)?.model ?? '' : session.model ?? '';
  let attempt = credentialAttempts.get(sessionId);
  if (attempt && (attempt.scope !== scope || attempt.model !== modelId)) { cancelAutoSwitch(sessionId); attempt = undefined; }
  if (!attempt) {
    const controller = new AbortController();
    attempt = { controller, signal: controller.signal, tried: new Set(), model: modelId, scope };
    credentialAttempts.set(sessionId, attempt);
  }
  const currentAttempt = attempt;
  currentAttempt.failedId = reply.credentialId;
  if (reply.credentialId) currentAttempt.tried.add(reply.credentialId);
  const isCurrent = () => !currentAttempt.signal.aborted && credentialAttempts.get(sessionId) === currentAttempt && workspace.sessions.some((item) => item.id === sessionId) && session.model === expectedModel && (session.agent === 'custom' ? parseCustomModel(session.model)?.providerId : session.agent === 'antigravity' ? 'agy' : session.agent) === scope;
  const block = () => reply.blocks.find((item): item is Extract<Block, { kind: 'command' }> => item.kind === 'command' && item.id === blockId);
  const started = Date.now();
  const timer = setInterval(() => {
    if (!isCurrent()) currentAttempt.controller.abort();
    const current = block();
    if (current?.status === 'running') current.output = `Waiting for an available account or API key…\nElapsed ${Math.floor((Date.now() - started) / 1000)}s`;
  }, 1000);
  let selected: string | null = null;
  try {
    if (!session) return;
    if (session.agent === 'custom') {
      const providerId = parseCustomModel(session.model)?.providerId;
      const provider = connectors.custom.find((item) => item.id === providerId);
      if (provider) {
        const switched = await autoSwitchProviderKey(provider, currentAttempt);
        if (switched) {
          selected = switched.label;
          const current = connectors.custom.find((item) => item.id === provider.id);
          if (current) void refreshProviderQuota(current);
        }
      }
    } else {
      const connector = (session.agent === 'antigravity' ? 'agy' : session.agent) as ConnectorId;
      const switched = await autoSwitchConnectorAccount(connector, currentAttempt);
      if (switched) selected = switched.email;
    }
    const current = block();
    if (!isCurrent()) { if (current) { current.status = 'fail'; current.output = 'Account switching cancelled'; } return; }
    if (selected && session && !replies[sessionId]) {
      if (current) {
        current.status = 'ok';
        current.output = `Switched to ${selected}. Continuing work…`;
      }
      send(sessionId, '', [], [], [], { prompt: continuePrompt('the account or API key reached its limit', true, lastRequest(sessionId)), silent: NO_HANDOUT });
      return;
    }
    if (selected && session && replies[sessionId]) {
      // The user's own message started a reply while the account was being switched: theirs wins, and the switch is done.
      if (current) {
        current.status = 'ok';
        current.output = `Switched to ${selected}.`;
      }
      cancelAutoSwitch(sessionId);
      return;
    }
    if (current) {
      current.status = 'fail';
      current.output = `No available account or API key. Elapsed ${Math.floor((Date.now() - started) / 1000)}s`;
    }
    if (session) scheduleResume(session, reason);
    cancelAutoSwitch(sessionId);
    // The failed turn skipped the queue while this switch ran: nothing else will start it, so it goes on now.
    if (!replies[sessionId]) sendNextQueued(sessionId);
  } catch (error) {
    const current = block();
    const cancelled = !isCurrent() || (error instanceof Error && error.name === 'AbortError');
    if (current) { current.status = 'fail'; current.output = cancelled ? 'Account switching cancelled' : 'Account verification failed. Resume manually after checking the account.'; }
    if (!cancelled) scheduleResume(session, reason);
    cancelAutoSwitch(sessionId);
    if (!cancelled && !replies[sessionId]) sendNextQueued(sessionId);
  } finally {
    clearInterval(timer);
    persistLocal();
  }
}

// ---------- carrying on when a usage limit ends ----------
// A turn that ends at a usage limit with no other agent to move to waits for the limit to reset (its time is in the error,
// or in the account's usage windows) and then carries on by itself, as Claude Code does. The chat shows a card with the
// time, a switch to turn the automatic part off, and Try again / Cancel. A message of the user cancels it: they took over.
export type Resume = { sessionId: string; agent: string; model?: string; at: number; since: number; reason: string; label: string; gate?: LimitGateKind; detail?: string; canAutoResume?: boolean; auto: boolean };
export const resumes = $state<Record<string, Resume>>({});
const RESUMES_KEY = 'chat.resumes';
/** A little after the reset, so the first request is not refused for being early. */
const RESUME_BUFFER_MS = 60_000;
/** Keep a paused request long enough for monthly provider windows to reset. */
const RESUME_MAX_WAIT_MS = 31 * 86_400_000;
const RESUME_MAX_AGE_MS = 32 * 86_400_000;
const resumeStore = { ready: false };
/** Keeps the user's switch choice across repeated provider limit responses for this paused task. */
export const resumeAutoChoice: Record<string, boolean> = {};
export const resumeSince: Record<string, number> = {};
void getPref<Resume[]>(RESUMES_KEY)
  .then((saved) => {
    for (const item of Array.isArray(saved) ? saved : []) if (!resumes[item.sessionId] && (item.since ?? item.at) + RESUME_MAX_AGE_MS > Date.now()) {
      item.label ??= 'Usage limit reached';
      item.gate ??= 'generic';
      item.detail ??= 'Try again after the provider limit resets';
      item.canAutoResume ??= item.auto;
      item.since ??= item.at;
      resumes[item.sessionId] = item;
      resumeAutoChoice[item.sessionId] = item.auto;
      resumeSince[item.sessionId] = item.since;
    }
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
  const resetAt = parseResetAt(message, now) ?? limitResetAt(session.agent, now);
  const gate = classifyLimitGate(message, resetAt, session.model);
  const end = gate.canAutoResume ? resetAt : null;
  if (end !== null && end - now > RESUME_MAX_WAIT_MS) return;
  const since = resumeSince[session.id] ?? resumes[session.id]?.since ?? now;
  if (now - since > RESUME_MAX_AGE_MS) return;
  resumes[session.id] = {
    sessionId: session.id,
    agent: session.agent,
    model: session.model,
    at: end === null ? now : Math.max(end, now) + RESUME_BUFFER_MS,
    since,
    reason: 'the usage limit was reached',
    label: gate.label,
    gate: gate.kind,
    detail: gate.detail,
    canAutoResume: gate.canAutoResume,
    auto: gate.canAutoResume && (resumeAutoChoice[session.id] ?? connectors.recovery.autoResume),
  };
}

/** Carries on now (the "Try again" button, or the time has come). */
export function resumeNow(sessionId: string) {
  const resume = resumes[sessionId];
  const session = workspace.sessions.find((item) => item.id === sessionId);
  // A reply is running: "Try again" must not throw the card away and send nothing; it stays until the chat is idle.
  if (resume && session && replies[sessionId]) return;
  delete resumes[sessionId];
  if (resume) {
    resumeAutoChoice[sessionId] = resume.auto;
    resumeSince[sessionId] = resume.since;
  }
  if (!resume || !session) return;
  send(sessionId, '', [], [], [], { prompt: continuePrompt(resume.reason, true, lastRequest(sessionId)), silent: NO_HANDOUT });
}
export const cancelResume = (sessionId: string) => {
  delete resumes[sessionId];
  delete resumeAutoChoice[sessionId];
  delete resumeSince[sessionId];
};
export function setResumeAuto(sessionId: string, auto: boolean) {
  const resume = resumes[sessionId];
  if (resume) {
    resume.auto = auto;
    resumeAutoChoice[sessionId] = auto;
  }
}

function checkResumes() {
  const now = Date.now();
  for (const resume of Object.values(resumes)) {
    if (resume.since + RESUME_MAX_AGE_MS < now) {
      delete resumes[resume.sessionId];
      delete resumeAutoChoice[resume.sessionId];
      delete resumeSince[resume.sessionId];
      continue;
    }
    if (!resume.auto || resume.at > now) continue;
    const session = workspace.sessions.find((item) => item.id === resume.sessionId);
    // Wait for the chat to be loaded and idle: a turn that is running will have its own end.
    if (!session || isRunning(resume.sessionId) || replies[resume.sessionId]) continue;
    resumeNow(resume.sessionId);
  }
}
if (typeof window !== 'undefined') setInterval(checkResumes, 30_000);

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
  if (!session || runReplies.has(turn)) return;
  const besideForeground = Boolean(liveReplies[sessionId]?.live);
  const list = messagesOf(sessionId);
  // Work the user stopped is not reported on: its "stopped" row is not a finished job.
  const candidates = activity.filter((item) => item.sessionId === sessionId && item.background && item.status !== 'stopped' && item.sourceMessageId && (!item.endedAt || Date.now() - item.endedAt < 10_000));
  // Right after the user's Stop, a report with no live background work behind it is the stopped work's leftover.
  if (candidates.length === 0 && Date.now() - (userStoppedAt[sessionId] ?? 0) < STOP_REPORT_GRACE_MS) return;
  const sources = new Set(candidates.map((item) => item.sourceMessageId));
  const source = messageId ?? (sources.size === 1 ? candidates[0]?.sourceMessageId : undefined);
  const runGroup = list.find((item) => item.id === source)?.runGroup ?? nextId();
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
    triggeredByMessageId: source,
    taskTitle: candidates.find((item) => item.sourceMessageId === source)?.sourceTitle ?? 'Background work report',
    runGroup,
  });
  const reply = list[list.length - 1];
  // A provider's unsolicited report owns a separate lane when the user is already talking to another run.
  runReplies.set(turn, { sessionId, reply, listener: besideForeground, backgroundTools: new Set() });
  updateLane(sessionId, { id: turn, role: besideForeground ? 'listener' : 'primary', state: 'running', backgroundWork: false });
  if (besideForeground) {
    session.status = 'working';
    return;
  }
  liveReplies[sessionId] = reply; // the stored (reactive) object
  replies[sessionId] = reply.id;
  turns[sessionId] = turn;
  session.status = 'working';
  const epoch = (runEpochs[sessionId] = (runEpochs[sessionId] ?? 0) + 1);
  advanceRun(sessionId, { type: 'spawn', epoch });
  advanceRun(sessionId, { type: 'turnStarted', epoch });
}

// Streamed text and tool output come as many small events, and each one used to redraw the whole reply on its own: a
// turn of a few thousand events froze the page. They are applied together every COALESCE_MS, in their order; any other
// event first applies what came before it, so a stop or a finished turn never overtakes the text it follows.
const COALESCE_MS = 100;
const STREAMED = new Set<AgentEvent['kind']>(['delta', 'thinking', 'tool_output_delta']);
const streamQueue = new Map<string, { event: AgentEvent; turn?: string; messageId?: string }[]>();
let flushTimer: ReturnType<typeof setTimeout> | undefined;

/** Applies the queued events of one chat, or of every chat. */
function flushQueued(onlySession?: string) {
  for (const [sessionId, items] of [...streamQueue]) {
    if (onlySession !== undefined && sessionId !== onlySession) continue;
    streamQueue.delete(sessionId);
    const started = performance.now();
    markStep(`apply ${items.length} streamed event(s) of ${sessionId}`);
    // One event that throws must not drop the rest of its chat's queue (a `done` could follow), nor another chat's.
    for (const item of items) applyEvent(sessionId, item.event, item.turn, item.messageId);
    const ms = Math.round(performance.now() - started);
    if (ms >= 50 || hangTraceOn()) logFlow(sessionId, 'applied', `${items.length} event(s) in ${ms} ms, last=${items.at(-1)?.event.kind}`);
  }
}
/** `handleEventNow` that never throws: a failing handler is logged and the next event still runs. */
function applyEvent(sessionId: string, event: AgentEvent, turn?: string, messageId?: string) {
  try {
    handleEventNow(sessionId, event, turn, messageId);
  } catch (error) {
    logError('turn', `event handler failed (${event.kind})`, error, logCtx(sessionId));
  }
}
if (typeof window !== 'undefined') window.addEventListener('pagehide', () => flushQueued());

export function handleAgentEvent(
  sessionId: string,
  event: AgentEvent,
  turn?: string,
  messageId?: string,
) {
  if (STREAMED.has(event.kind)) {
    const items = streamQueue.get(sessionId) ?? [];
    items.push({ event, turn, messageId });
    streamQueue.set(sessionId, items);
    flushTimer ??= setTimeout(() => {
      flushTimer = undefined;
      try {
        flushQueued();
      } finally {
        // Whatever stayed queued (a failure above) is applied on the next tick, not left until the next delta.
        if (streamQueue.size > 0) flushTimer = setTimeout(() => ((flushTimer = undefined), flushQueued()), COALESCE_MS);
      }
    }, COALESCE_MS);
    return;
  }
  flushQueued(sessionId);
  applyEvent(sessionId, event, turn, messageId);
}

function handleEventNow(
  sessionId: string,
  event: AgentEvent,
  turn?: string,
  messageId?: string,
) {
  const ownedRun = turn ? runReplies.get(turn) : undefined;
  // The agent answered: the reply stops saying it is being handed over.
  if (ownedRun?.reply.dispatching) ownedRun.reply.dispatching = false;
  // Task ids correlate to their original cards even after the user has started a newer turn.
  if (event.kind === 'task') {
    updateHistoricalTask(messagesOf(sessionId), event);
  }
  const startsTailReply = event.kind === 'auto_turn' && turn?.startsWith('auto-');
  if (turn !== undefined && turns[sessionId] !== turn && !ownedRun && !startsTailReply) {
    if (event.kind === 'task') trackActivity(sessionId, '', '', event, turn);
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
  const currentReply = ownedRun?.reply ?? liveReplies[sessionId];
  const expectedMessage = currentReply?.autoTurn ? currentReply.triggeredByMessageId : currentReply?.replyTo;
  if (messageId && expectedMessage && messageId !== expectedMessage) {
    logFlow(sessionId, 'event dropped', `turn=${turn ?? '-'} message=${messageId} does not belong to reply ${currentReply?.id ?? '-'}`);
    return;
  }
  if (ownedRun) {
    if (event.kind === 'tool_call' && event.background) {
      ownedRun.backgroundTools.add(event.id);
      updateLane(sessionId, { id: turn!, role: ownedRun.listener ? 'listener' : 'primary', state: 'running', backgroundWork: true });
    } else if (event.kind === 'tool_result' && ownedRun.backgroundTools.delete(event.id)) {
      updateLane(sessionId, { id: turn!, role: ownedRun.listener ? 'listener' : 'primary', state: 'running', backgroundWork: ownedRun.backgroundTools.size > 0 });
    } else if (event.kind === 'task') {
      if (event.status === 'started' || event.status === 'progress') {
        ownedRun.backgroundTools.add(event.id);
        if (event.tool_use_id) ownedRun.backgroundTools.add(event.tool_use_id);
      } else {
        ownedRun.backgroundTools.delete(event.id);
        if (event.tool_use_id) ownedRun.backgroundTools.delete(event.tool_use_id);
      }
      const wasWaiting = sessionLanes[sessionId]?.find((lane) => lane.id === turn)?.state === 'waiting';
      updateLane(sessionId, { id: turn!, role: ownedRun.listener ? 'listener' : 'primary', state: wasWaiting && ownedRun.backgroundTools.size ? 'waiting' : 'running', backgroundWork: ownedRun.backgroundTools.size > 0 });
    }
    if (event.kind === 'tool_call' && isBackgroundWait(event.name, event.background, event.input) && ownedRun.backgroundTools.size) {
      updateLane(sessionId, { id: turn!, role: ownedRun.listener ? 'listener' : 'primary', state: 'waiting', backgroundWork: true });
      sendNextQueued(sessionId);
    }
    if (event.kind === 'delta' || event.kind === 'thinking') {
      updateLane(sessionId, { id: turn!, role: ownedRun.listener ? 'listener' : 'primary', state: 'running', backgroundWork: ownedRun.backgroundTools.size > 0 });
    }
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
    NO_CONVERSATION.test(event.message)
  ) {
    // The CLI no longer has the conversation this id named (a thread whose run was cut short, a cleaned-up session).
    // The next turn starts a new one, and is given what was said so far as text, so the work in progress is not lost.
    // Every provider's own id is reset: the chat's first agent and any agent it moved to later.
    session.agentStarted = false;
    session.agentSession = undefined;
    const own = session.agentIds?.[session.agent];
    if (own) own.started = false;
    session.forkContext = (session.forkContext ?? '') + handoffFor(sessionId, lastRequest(sessionId));
  }
  trackActivity(
    sessionId,
    session.title,
    workspace.projects.find((project) => project.id === session.projectId)
      ?.name ?? '',
    event,
    turn,
    { agent: reply?.agent ?? session.agent, model: reply?.model ?? session.model, messageId: ownedRun?.ask?.id ?? reply?.replyTo ?? reply?.triggeredByMessageId, request: ownedRun?.ask ? askText(ownedRun.ask) : liveAsks[sessionId] ? askText(liveAsks[sessionId]) : undefined },
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
      session.status = 'idle';
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
    archiveTurn({ id: sessionId, title: session.title, agent: reply.agent ?? session.agent, model: reply.model ?? session.model, projectPath: workspace.projects.find((project) => project.id === session.projectId)?.path ?? null }, [ownedRun.ask, reply].filter((message): message is Message => Boolean(message)));
    void finishCommandImages(sessionId, reply);
    if (session.path) {
      if (ownedRun.ask) keepOtherTurn(session, ownedRun.ask);
      keepOtherTurn(session, reply);
    }
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
  if (update.agentSession && agent === first) {
    session.agentSession = update.agentSession;
    session.agentStarted = true;
  } else if (agent === first) {
    if (event.kind !== 'error') session.agentStarted = true;
  } else if (
    update.agentSession ||
    (event.kind !== 'error' &&
      session.agentIds?.[agent])
  ) {
    const id = update.agentSession ?? session.agentIds?.[agent]?.id;
    if (id) (session.agentIds ??= {})[agent] = { ...session.agentIds?.[agent], id, started: true };
  }
  // A failed turn that can be carried on (retry, another account, the next agent) does not end the work: the chat
  // gets a line saying what happens, and the steps below that close the work (failure, reports) are skipped.
  const failureKind = event.kind === 'error' ? classifyFailure(event.message) : null;
  const customProviderId = session.agent === 'custom' ? parseCustomModel(session.model)?.providerId : undefined;
  const autoSwitchConnector = session.agent === 'antigravity' ? 'agy' : session.agent;
  const autoSwitchRequested = Boolean(update.finished && event.kind === 'error' && (failureKind === 'limit' || failureKind === 'auth') && (
    session.agent === 'custom'
      ? connectors.custom.some((provider) => provider.id === customProviderId && provider.autoSwitch)
      : ['claude', 'codex', 'copilot', 'agy'].includes(autoSwitchConnector) && connectors.autoSwitch[autoSwitchConnector as ConnectorId]
  ));
  const recovery = update.finished && event.kind === 'error' && !autoSwitchRequested ? recoveryFor(session, event.message) : null;
  const recovering = autoSwitchRequested || (recovery !== null && recovery.plan.action !== 'stop');
  let autoSwitchBlockId: string | null = null;
  if (update.finished && event.kind !== 'error') {
    cancelAutoSwitch(sessionId);
    delete recoveries[sessionId];
    delete resumeAutoChoice[sessionId];
    delete resumeSince[sessionId];
  }
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
  if (autoSwitchRequested) {
    autoSwitchBlockId = nextId();
    reply.blocks = [...reply.blocks, { kind: 'command', id: autoSwitchBlockId, command: 'AgentDeck · waiting for account switch', output: 'Elapsed 0s', status: 'running' }];
  }
  if (update.finished) {
    for (const id of reply.taskIds ?? []) {
      if (delegateTasks[id]) delegateTasks[id].reportState = event.kind === 'error' ? 'failed' : 'done';
    }
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
        finishWorkflowTurn(sessionId, event.kind === 'error' && !recovering, reply);
        finishWorkflowIntegration(sessionId, reply, event.kind === 'error');
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
      session.status = (sessionLanes[sessionId]?.length ?? 0) > 0 ? 'working' : 'idle';
      persistLocal();
      // The CLI wrote the conversation to disk: pick it up so the row gets its path, in place (same id).
      // The cached transcript of this chat is now behind the file: a new time makes the next open read it again.
      session.updatedAt = new Date().toISOString();
      transcripts.delete(sessionId);
      if (!session.path) void importHistory({ refresh: true });
      // Track the final Markdown animation independently of provider execution and queue delivery.
      const responseBlocks = reply.blocks.flatMap((block, index) =>
        block.kind === 'response' && block.text ? [index] : [],
      );
      if (autoSwitchRequested && autoSwitchBlockId && event.kind === 'error')
        void runAutoSwitch(sessionId, clipText(event.message, 200), reply, autoSwitchBlockId);
      else if (recovering && recovery && event.kind === 'error')
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
        // An offscreen virtual block never reports: expire its presentation state without blocking delivery.
        const waitingFor = renderWaiters[sessionId];
        logFlow(sessionId, 'render wait', `reply ${reply.id}: tracking ${responseBlocks.length} block(s) drawing`);
        setTimeout(() => {
          if (renderWaiters[sessionId] !== waitingFor) return;
          delete renderWaiters[sessionId];
          logFlow(sessionId, 'render wait timed out', `reply ${reply.id} was not drawn in ${RENDER_WAIT_LIMIT_MS} ms: clearing presentation state`);
          sendNextQueued(sessionId);
        }, RENDER_WAIT_LIMIT_MS);
        // Rendering is presentation only: an offscreen virtual block must not add 15 seconds between queued turns.
        sendNextQueued(sessionId);
      } else sendNextQueued(sessionId);
    }
  }
}

if (inTauri() && !isPreviewCopy()) {
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
        handleAgentEvent(session, event, turn, message_id);
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
