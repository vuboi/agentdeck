import { credentialBusy } from '../../agents/credential-switch';
import { inTauri, clock } from '../../app/platform/history';
import { accountEpoch } from '../runs/account-epoch.svelte';
import { pictureData, prehandleAntigravity, prehandleAttachments, prehandleOpencode, prewarmAgent, saveAttachment, sendCustomTurn, sendAgentTurn, stopAgentTurn, answerCopilotPermission } from '../agent';
import { activeProviderKey, claudeCreditOn, claudeCreditState, connectors, currentCredentialId, providerQuotas, usagePct } from '../../agents/connectors.svelte';
import { type Message, messages as mockMessages } from '../mock';
import { guide } from '../../app/ui/guide.svelte';
import { logFlow } from '../../tasks/flow-log';
import { sessionMark } from '../content/chat-dividers';
import { within, PREPARE_LIMIT_MS } from '../../app/text/within';
import { expandSlash } from '../slash.svelte';
import { modelLabel, parseCustomModel } from '../../agents/models.svelte';
import { routesOf, taskRoleHint } from '../../agents/task-roles';
import { taskRoles } from '../../agents/task-roles.svelte';
import { agentDisplayName } from '../../agents/provider-registry';
import { customProviderOf } from '../../agents/provider-registry';
import { automation } from '../../app/platform/automation.svelte';
import { logError } from '../../app/platform/error-log.svelte';
import { DELEGATE_HINT, DELEGATE_REMINDER, agentdeckLead, isLongPrompt, looksNontrivial } from '../../tasks/delegate-task';
import { houseStyleHint } from '../content/house-style';
import { replyStyle } from '../content/reply-style.svelte';
import { memoryHint, readProjectMemory } from '../content/project-memory';
import { learnFromPrompt, loadProfile, profileHint } from '../content/user-profile';
import { logRequest } from '../runs/activity.svelte';
import { setPref } from '../../app/platform/prefs';
import { type RunLane, deliveryFor } from '../runs/run-lanes';
import { parseQuestions, type Question } from '../recovery/questions';
import { dequeue } from '../runs/queue';
import { MODELS, activeSession, advanceRun, localMessages, nextId, persistLocal, runEpochs, sessionCwd, workspace, type Agent, type Attachment, type Effort, type FileAttachment, type SessionItem } from './state.svelte';
import { AGENTDECK_HINT, KNOWS_PREVIEW, NATIVE_DELEGATION, NATIVE_DELEGATION_HINT, NO_DELEGATE_HINT, OPEN_WORDS, PERMISSION_MODE, agentLabel, approved, continuationGroups, delegateHinted, hasOutstandingSessionWork, hasTurnWork, hiddenPrompt, isRunning, lastPrompt, missedTurns, modelFor, nativeDelegationHinted, notifySessionWorkDone, orchestrateTold, priorDelegateHinted, queuePaused, queued, rememberDelegateHint, rememberLastChat, rememberNativeDelegationHint, turnOptions, turnOptionsOf, turns } from './turns.svelte';
import { handleAgentEvent, lastEventAt, lastEventKind, manuallyStopped, recoveryChain, recoveryModelFor } from './events.svelte';
import { answerWorkerApproval, backlog, flushReports, isWorkerChat } from './delegate-task.svelte';
import { LANGUAGE_RULE } from './orchestration.svelte';
import { COMPACT_COMMAND, MODEL_LEAD, compressIfNeeded, handoffDigest, liveModel, send, sendImmediate } from './sending.svelte';
import { chat, history, settleKeptPermission } from './history.svelte';

/** The chats that ran a turn since the app started: the first turn of any other chat that resumes a conversation resumes a session from before. */
const touchedThisRun = new Set<string>();
/** Chats whose next turn starts another CLI conversation (an edited message): that turn carries a "New session" line. */
export const pendingNew = new Set<string>();

/** When each chat's CLI was last asked to start ahead (by chat, agent and model), so typing does not ask on every key. */
/** Chats whose previous turn got DELEGATE_REMINDER (not repeated for a short follow-up). */
const reminded = new Map<string, boolean>();
const prewarmedAt = new Map<string, number>();

type PreHandler =
  | { via: 'http'; label: string; kind: string; baseUrl: string; apiKey: string | null; model: string }
  | { via: 'opencode' | 'antigravity'; label: string; model: string };
/** `route` is the `agent|model` the Task roles settings name this model by (see agents/task-roles.ts). */
type Candidate = { pick: PreHandler; route: string };
/** The wait for a CLI pre-handle: the CLI starts, then the model answers (the Rust side stops at 120 s). */
const CLI_PREPARE_LIMIT_MS = 120_000;
/** The Claude 5 models the Console credit runs (the `credit:` ids without the prefix). */
const CLAUDE_CREDIT_MODELS = ['claude-haiku-5-5', 'claude-sonnet-5-5', 'claude-opus-5-5'];
/**
 * The model that reads a turn's attachments: only the models picked for the image role in Task roles, in priority order.
 * Nothing is picked by default: with no pick, or the role switched off, the main agent reads the pictures itself. A pick
 * that fails at run time also goes back to the main agent, and is left out for a while (prehandleFailed).
 */
function prehandleProvider(): PreHandler | undefined {
  if (!routesOf(taskRoles, 'read').length) return undefined;
  const candidates: Candidate[] = [];
  for (const provider of connectors.custom.filter((item) => item.enabled && !item.restricted && item.baseUrl.trim())) {
    const key = activeProviderKey(provider);
    if (providerQuotas[`${provider.id}:${key?.id ?? 'default'}`]?.remaining === 0) continue;
    for (const model of new Set([...provider.models, provider.defaultModel].filter(Boolean))) {
      candidates.push({
        route: `${provider.id}|${model}`,
        pick: { via: 'http', label: `${provider.kind}:${provider.id}`, kind: provider.kind, baseUrl: provider.baseUrl, apiKey: key?.apiKey || null, model },
      });
    }
  }
  if (claudeCreditOn() && !((claudeCreditState.reading?.balance ?? 1) <= 0)) {
    for (const model of CLAUDE_CREDIT_MODELS) {
      candidates.push({
        route: `claude|credit:${model}`,
        pick: { via: 'http', label: 'claude-credit', kind: 'anthropic', baseUrl: 'https://api.anthropic.com', apiKey: connectors.claudeCredit.apiKey.trim(), model },
      });
    }
  }
  for (const item of MODELS.copilot) {
    candidates.push({ route: `copilot|${item.value}`, pick: { via: 'http', label: 'copilot', kind: 'copilot', baseUrl: 'https://api.githubcopilot.com', apiKey: null, model: item.value } });
  }
  for (const item of MODELS.opencode) {
    candidates.push({ route: `opencode|${item.value}`, pick: { via: 'opencode', label: 'opencode', model: item.value } });
  }
  // An Antigravity model at its usage limit is skipped; its CLI prints no usable memo once it is out.
  for (const item of MODELS.antigravity) {
    if ((usagePct('antigravity', item.value) ?? 0) >= 100) continue;
    candidates.push({ route: `antigravity|${item.value}`, pick: { via: 'antigravity', label: 'antigravity', model: item.value } });
  }
  // A model that failed lately (expired key, no quota) is left out for a while: the turn that failed went to the main
  // agent, and the next turns take the next model instead of failing on the same one again.
  const usable = candidates.filter((item) => Date.now() - (prehandleFailed.get(pickKey(item.pick)) ?? 0) > PREHANDLE_COOLDOWN_MS);
  return routesOf(taskRoles, 'read')
    .map((route) => usable.find((item) => item.route === route))
    .find((item) => item !== undefined)?.pick;
}

/** A route as the user reads it: "Codex GPT 5.6 Terra". */
const routeLabel = (route: string) => {
  const [agent, ...rest] = route.split('|');
  return `${agentDisplayName(agent)} ${modelLabel(rest.join('|'))}`;
};

/** The pick a failed pre-handle is remembered by. */
const pickKey = (pick: PreHandler) => `${pick.label}|${pick.model}`;
/** When each pick last failed to read pictures, and how long it is left out after that. */
const prehandleFailed = new Map<string, number>();
const PREHANDLE_COOLDOWN_MS = 10 * 60_000;


/** The conversation id a Claude chat takes after an account switch (by account epoch), so the CLI started ahead for it is the one used. */
const accountNext: Record<string, { epoch: number; id: string }> = {};
function accountConversationId(sessionId: string, epoch: number): string {
  const ready = accountNext[sessionId];
  if (ready?.epoch === epoch) return ready.id;
  const id = crypto.randomUUID();
  accountNext[sessionId] = { epoch, id };
  return id;
}

/**
 * Starts the CLI of a chat whose next message is being typed (Claude, Codex, Antigravity keep it open between turns,
 * see warm.rs), with what `startTurn` will send, so the first turn does not wait for it to load. Nothing is changed in
 * the chat, except that a new Claude chat gets the conversation id its first turn will use. A turn that will start a
 * new conversation (another model, another account) is left to start on its own.
 */
export function prewarmChat(sessionId: string) {
  const selected = workspace.sessions.find((item) => item.id === sessionId);
  if (selected && credentialBusy(selected.agent === 'antigravity' ? 'agy' : selected.agent)) return;
  const session = workspace.sessions.find((item) => item.id === sessionId);
  if (!session || !inTauri() || !['claude', 'codex', 'antigravity', 'copilot'].includes(session.agent) || isRunning(session.id)) return;
  const selectedModel = modelFor(session);
  const now = Date.now();
  const epoch = accountEpoch[session.agent] ?? 0;
  // The account is part of the throttle: a switch makes a new CLI worth starting at once.
  const throttle = `${session.id}|${session.agent}|${selectedModel}|${epoch}`;
  if (now - (prewarmedAt.get(throttle) ?? 0) < 60_000) return;
  prewarmedAt.set(throttle, now);
  const first = session.source ?? session.agents?.[0] ?? session.agent;
  let cliId: string | null;
  let resume: boolean;
  if (session.agent === first) {
    if (session.agentEpoch !== undefined && session.agentEpoch !== epoch) {
      // The account changed: the next turn starts a new conversation under it (see startTurn). Its CLI is started now,
      // with the id that turn will use, so the first message does not wait for the start.
      cliId = session.agent === 'claude' ? accountConversationId(session.id, epoch) : null;
      resume = false;
    } else if (session.conversationModel && session.conversationModel !== selectedModel && !liveModel(session.agent)) {
      // Another model starts a new conversation (see startTurn), which its CLI starts on its own when the message comes.
      cliId = null;
      resume = false;
    } else {
      const prefix = `${session.agent}:`;
      const fromId = session.agentStarted === false ? undefined : session.id.startsWith(prefix) ? session.id.slice(prefix.length) : undefined;
      if (!session.agentSession && !fromId && session.agent === 'claude') session.agentSession = crypto.randomUUID();
      cliId = session.agentSession ?? fromId ?? null;
      resume = cliId !== null && (session.agentStarted ?? !!session.path);
    }
  } else {
    const own = session.agentIds?.[session.agent];
    // A first turn of Claude here picks its id then; Codex and Antigravity name theirs, so they start without one.
    if (!own && session.agent === 'claude') return;
    if (own && own.epoch !== undefined && own.epoch !== epoch) return;
    if (session.agentModels?.[session.agent] && session.agentModels[session.agent] !== selectedModel && !liveModel(session.agent)) return;
    cliId = own?.id || null;
    resume = cliId !== null && Boolean(own?.started);
  }
  const cwd = sessionCwd(session);
  void prewarmAgent({
    session: session.id,
    agent: session.agent,
    agentSession: cliId,
    resume,
    model: selectedModel,
    permissionMode: PERMISSION_MODE[turnOptionsOf(session.id).permission],
    effort: turnOptionsOf(session.id).effort as Effort,
    allowedTools: [...(approved[session.id] ?? [])],
    cwd,
  }).catch(() => {});
}

/**
 * Starts the CLI of the agent a chat would fall back to (Settings: keep working when an agent fails) while the chat's own
 * agent works, so a failure at a limit does not also wait for a cold start. Only Codex and Antigravity name their own
 * conversation, so only they can be started ahead without an id; Claude as a fallback starts when it is needed.
 */
function prewarmFallback(session: SessionItem) {
  if (!connectors.recovery.enabled || !inTauri()) return;
  const target = recoveryChain().find((agent) => agent !== session.agent && ['codex', 'antigravity'].includes(agent));
  const model = target ? (recoveryModelFor(target) ?? MODELS[target as Agent]?.[0]?.value) : undefined;
  if (!target || !model) return;
  const throttle = `${session.id}|${target}|${model}|fallback`;
  const now = Date.now();
  if (now - (prewarmedAt.get(throttle) ?? 0) < 300_000) return;
  prewarmedAt.set(throttle, now);
  const cwd = sessionCwd(session);
  void prewarmAgent({
    session: session.id,
    agent: target,
    agentSession: null,
    resume: false,
    model,
    permissionMode: PERMISSION_MODE[turnOptionsOf(session.id).permission],
    effort: turnOptionsOf(session.id).effort as Effort,
    allowedTools: [...(approved[session.id] ?? [])],
    cwd,
  }).catch(() => {});
}

/** One CLI run (`claude -p`, `codex exec`) in `session`, with a fresh live reply. Images go to disk first so the CLI can open them. */
export function startTurn(
  session: SessionItem,
  text: string,
  images: Attachment[] = [],
  allow: string[] = [],
  preface = '',
  files: FileAttachment[] = [],
  silent?: Message,
  runGroup?: string,
  listener = false,
  report?: Pick<Message, 'triggeredByMessageId' | 'taskTitle' | 'taskIds' | 'workflowRunId'>,
) {
  // A new turn of the chat ends the "stopped by you" state: any start (a queued message, a recovery) clears it, or the
  // composer kept offering "Continue" for a request that had already moved on.
  if (!listener) delete manuallyStopped[session.id];
  const documentRequest = text.match(/^\/document(?:\s+([\s\S]*))?/i);
  if (documentRequest) {
    guide.tasks[session.id] = {
      request: documentRequest[1]?.trim() || 'Create the requested document',
      status: 'working',
      progress: 'Starting document task',
      result: '',
    };
    guide.open = true;
  }
  if (images.length || !allow.length) lastPrompt[session.id] = text;
  // The CLI's own id for the conversation. Claude takes one we choose; Codex names its thread on the first turn
  // and reports it, so a new Codex chat starts without one.
  const first = session.source ?? session.agents?.[0] ?? session.agent;
  const prefix = `${session.agent}:`;
  let cliId: string | null;
  let resume: boolean;
  /** The conversation so far was put in `forkContext` for a new session (model or account change). */
  let digested = false;
  /** The turn starts another CLI conversation than the chat had (a model or account change, an edited message). */
  let conversationChanged = !listener && pendingNew.delete(session.id);
  const selectedModel = modelFor(session);
  rememberLastChat(
    session.agent,
    selectedModel,
    turnOptionsOf(session.id).effort as Effort,
  );
  if (listener) {
    // Never resume the background conversation: this lane gets an independent provider conversation.
    cliId = session.agent === 'claude' ? crypto.randomUUID() : null;
    resume = false;
  } else if (session.agent === first) {
    // A conversation whose last run never started is not resumed from its id (see handleAgentEvent).
    const fromId =
      session.agentStarted === false
        ? undefined
        : session.id.startsWith(prefix)
          ? session.id.slice(prefix.length)
          : undefined;
    const known = session.agentSession ?? fromId;
    cliId = known ?? (session.agent === 'claude' ? crypto.randomUUID() : null);
    const modelChanged = Boolean(
      session.conversationModel && session.conversationModel !== selectedModel && !liveModel(session.agent),
    );
    if (modelChanged) {
      // Codex and Antigravity name their new conversation themselves.
      cliId = null;
      session.agentStarted = false;
      session.forkContext = (session.forkContext ?? '') + handoffDigest(messagesOf(session.id), MODEL_LEAD);
      digested = true;
      conversationChanged = true;
    }
    // A conversation begun under another account starts a new session: resuming the old one stalls. What was said
    // so far goes with the first message, so the agent does not lose the work in progress.
    const epoch = accountEpoch[session.agent] ?? 0;
    const accountChanged = session.agentEpoch !== undefined && session.agentEpoch !== epoch;
    if (accountChanged) {
      cliId = session.agent === 'claude' ? accountConversationId(session.id, epoch) : null;
      session.agentStarted = false;
      session.agentSession = undefined;
      if (!digested) session.forkContext = (session.forkContext ?? '') + handoffDigest(messagesOf(session.id));
      digested = true;
      conversationChanged = true;
    }
    session.agentEpoch = epoch;
    if (cliId) session.agentSession = cliId;
    resume =
      cliId !== null &&
      !modelChanged &&
      !accountChanged &&
      (session.agentStarted ?? !!session.path);
    session.conversationModel = selectedModel ?? undefined;
  } else {
    // Another agent in this chat: its own conversation, started the first time it is used here. The entry is made on
    // that first turn even before the agent has an id (Codex names its thread later), so the account it began under
    // is known from the start.
    const epoch = accountEpoch[session.agent] ?? 0;
    const known = session.agentIds?.[session.agent];
    const own = known ?? ((session.agentIds ??= {})[session.agent] = { id: '', started: false, epoch });
    cliId = own.id || (session.agent === 'claude' ? crypto.randomUUID() : null);
    if (cliId && !own.id) own.id = cliId;
    const modelChanged = Boolean(
      session.agentModels?.[session.agent] &&
      session.agentModels[session.agent] !== selectedModel &&
      !liveModel(session.agent),
    );
    if (modelChanged) {
      cliId = null;
      session.forkContext = (session.forkContext ?? '') + handoffDigest(messagesOf(session.id), MODEL_LEAD);
      digested = true;
      conversationChanged = true;
    }
    // The same account rule as the chat's first agent: a new session, with the conversation so far.
    const accountChanged = own.epoch !== undefined && own.epoch !== epoch;
    if (accountChanged) {
      cliId = session.agent === 'claude' ? crypto.randomUUID() : null;
      own.started = false;
      if (cliId) own.id = cliId;
      if (!digested) session.forkContext = (session.forkContext ?? '') + handoffDigest(messagesOf(session.id));
      digested = true;
      conversationChanged = true;
    }
    own.epoch = epoch;
    resume = cliId !== null && !modelChanged && !accountChanged && own.started;
    (session.agentModels ??= {})[session.agent] = selectedModel ?? '';
  }
  // A turn still marked live is replaced by this one: its events are ignored from now on (see `turns`), so it would
  // show "Thinking" for ever. It ends here, with what it wrote.
  const previous = listener ? undefined : liveReplies[session.id];
  if (previous?.live) {
    previous.live = false;
    if (previous.startedAt) previous.ms = Date.now() - previous.startedAt;
  }
  const turn = nextId();
  if (!listener) turns[session.id] = turn;
  logFlow(session.id, 'send', `turn=${turn} agent=${session.agent} model=${selectedModel ?? session.model ?? '-'} resume=${resume} silent=${Boolean(silent)} worker=${isWorkerChat(session.id)}`);
  if (!isWorkerChat(session.id)) prewarmFallback(session);
  const list = messagesOf(session.id);
  const asked = silent ? undefined : list.at(-1);
  // A silent turn (the hand-back) continues the finished reply that handed work out: it is one message on screen.
  const host = silent ? list.at(-1) : undefined;
  const continues =
    !report &&
    !!host &&
    host.role === 'assistant' &&
    !host.live &&
    host.agent === session.agent;
  // With pictures, the model that reads them is picked now, so the placeholder can name it while it waits.
  const pictureReader = images.length && automation.prehandle ? prehandleProvider() : undefined;
  const reply: Message =
    !listener && continues && host
      ? host
      : {
          id: nextId(),
          role: 'assistant',
          time: clock(new Date().toISOString()),
          at: new Date().toISOString(),
          blocks: [],
          live: true,
          // Shown from here, before the CLI is started: the attachments and the command are still being read.
          dispatching: true,
          ...(isWorkerChat(session.id) ? { handedOff: true } : {}),
          ...(pictureReader ? { dispatchingImages: { count: images.length, reader: modelLabel(pictureReader.model) || pictureReader.model } } : {}),
          startedAt: Date.now(),
          agent: session.agent,
          model: selectedModel ?? session.model,
          ...(asked?.role === 'user' ? { replyTo: asked.id } : {}),
          ...(runGroup ? { runGroup } : {}),
          ...(report ? { ...report, autoTurn: true } : {}),
        };
  if (continues && host) {
    if (runGroup) host.runGroup = runGroup;
    host.live = true;
    host.startedAt = Date.now() - (host.ms ?? 0);
    host.callOutput = 0;
  }
  reply.credentialId = currentCredentialId(session.agent, parseCustomModel(selectedModel ?? session.model)?.providerId);
  // A silent turn (the hand-back) has no message of its own: its reply simply follows the message that handed work out.
  const context = listener
    ? preface
    :
    // The whole conversation already goes with a new session (a model or account change): the turns of the other agents
    // are in it, so they are not added a second time.
    preface + (session.forkContext ?? '') + (digested ? '' : missedTurns(session, list, !silent));
  if (!listener) session.forkContext = undefined;
  // The line that says a session was resumed, compacted or replaced goes above the user's message that starts it (or
  // above the reply, for the compaction, which has no message of the user). A silent turn has none.
  const mark = silent
    ? undefined
    : sessionMark({
        resume,
        changed: conversationChanged,
        compacting: text === COMPACT_COMMAND,
        firstThisRun: !touchedThisRun.has(session.id),
        hasHistory: list.some((message) => message !== asked && message !== reply),
        now: new Date(),
      });
  touchedThisRun.add(session.id);
  if (!continues) list.push(reply);
  if (mark) (asked?.role === 'user' ? asked : list[list.length - 1]).mark = mark;
  if (!listener) liveReplies[session.id] = list[list.length - 1]; // the stored (reactive) object
  if (!listener && asked?.role === 'user') liveAsks[session.id] = asked;
  if (!listener && silent) {
    liveHandouts[session.id] = silent;
    delete liveAsks[session.id];
  } else if (!listener) delete liveHandouts[session.id];
  if (!listener) replies[session.id] = reply.id;
  runReplies.set(turn, { sessionId: session.id, reply: list[list.length - 1], listener, ask: asked?.role === 'user' ? asked : undefined, backgroundTools: new Set() });
  updateLane(session.id, { id: turn, role: listener ? 'listener' : 'primary', state: 'running', backgroundWork: false });
  session.status = 'working';
  // A turn that never says anything (a CLI that hangs while starting) is watched from its start.
  lastEventAt.set(session.id, Date.now());
  lastEventKind.set(session.id, 'start');
  const epoch = (runEpochs[session.id] = (runEpochs[session.id] ?? 0) + 1);
  advanceRun(session.id, { type: 'spawn', epoch });
  advanceRun(session.id, { type: 'turnStarted', epoch });
  const cwd = sessionCwd(session);
  void (async () => {
    // Nothing before the CLI starts may wait for ever: a call that never answers would leave the chat on "Thinking" with
    // no event to end it.
    const paths = await within(
      Promise.all(
        images.map((image) =>
          image.path ? image.path : saveAttachment(image.mime, image.src.slice(image.src.indexOf(',') + 1)),
        ),
      ),
      PREPARE_LIMIT_MS,
      'saving the attached pictures',
    );
    // A message that starts with `/name` is a command: its text goes to the agent, the chat keeps what was typed.
    const body = isWorkerChat(session.id)
      ? text
      : await within(expandSlash(text, session.agent, cwd), PREPARE_LIMIT_MS, 'reading the command', () => text);
    // Stop/Send now can happen while attachments or a slash command are being prepared. Never start that old run.
    if (!runReplies.has(turn)) return;
    let dispatchBody = body;
    let dispatchPaths = paths;
    if (automation.prehandle && (paths.length || files.length)) {
      const selected = prehandleProvider();
      if (selected) {
        logFlow(session.id, 'attachment pre-handle started', `provider=${selected.label} model=${selected.model} images=${paths.length} files=${files.length}`);
        try {
          const request = { model: selected.model, prompt: body, images: paths, files: files.map((file) => file.path) };
          // A CLI start (OpenCode, Antigravity) takes longer than an HTTP call, so it gets the CLI's own limit.
          const memo = await within(
            selected.via === 'http'
              ? prehandleAttachments({ ...request, kind: selected.kind, baseUrl: selected.baseUrl, apiKey: selected.apiKey })
              : selected.via === 'opencode' ? prehandleOpencode(request) : prehandleAntigravity(request),
            selected.via === 'http' ? PREPARE_LIMIT_MS : CLI_PREPARE_LIMIT_MS,
            'preparing attachment notes',
          );
          if (memo.trim()) {
            dispatchBody = `${body}\n\n[Attachment pre-handle memo]\n${memo.trim()}`;
            dispatchPaths = [];
            logFlow(session.id, 'attachment pre-handle completed', `model=${selected.model} memo=${memo.trim().length} chars`);
          }
        } catch (error) {
          logFlow(session.id, 'attachment pre-handle failed', String(error).slice(0, 300));
          prehandleFailed.set(pickKey(selected), Date.now());
          logError('turn', 'attachment pre-handle failed; sending original attachments', error);
        }
        // The reading is over (memo, or back to the main agent): the placeholder stops naming a reader.
        reply.dispatchingImages = undefined;
      } else {
        logFlow(session.id, 'attachment pre-handle skipped', 'no model picked for the image role: the main agent reads the pictures');
      }
    }
    if (session.agent === 'custom') {
      // A custom provider has no CLI to keep the conversation: the whole chat goes along with every turn.
      const target = parseCustomModel(selectedModel);
      const provider = connectors.custom.find(
        (item) => item.id === target?.providerId,
      );
      if (!target || !provider)
        throw new Error(
          'This provider was removed or turned off. Pick another model.',
        );
      const spoken = (message: Message) =>
        message.blocks
          .flatMap((block) => (block.kind === 'response' ? [block.text] : []))
          .join('\n')
          .trim();
      const history = list
        .slice(0, silent ? -1 : -2)
        .filter((message) => !hiddenPrompt(message))
        // A free-tier provider only gets what the user wrote and what restricted providers answered: the other agents'
        // replies are about the project, and carry its code.
        .filter((message) => !provider.restricted || message.role === 'user' || customProviderOf(message.model)?.restricted)
        .map((message) => ({
          role: message.role,
          content: spoken(message),
        }))
        .filter((message) => message.content);
      const pictureSources = dispatchPaths.length
        ? await within(Promise.all(images.map((image) => pictureData(image))), PREPARE_LIMIT_MS, 'reading the pictures', () => [] as string[])
        : [];
      if (!runReplies.has(turn)) return;
      // Like the CLI path: a start that never answers ends in an error the chat shows, not in "Thinking" for good.
      await within(sendCustomTurn({
        session: session.id,
        turn,
        messageId: asked?.role === 'user' ? asked.id : undefined,
        kind: provider.kind,
        baseUrl: provider.baseUrl,
        apiKey: activeProviderKey(provider)?.apiKey || null,
        model: target.model,
        messages: [
          ...history,
          {
            role: 'user',
            content: dispatchBody,
            images: pictureSources,
            files: files.map((file) => file.path),
          },
        ],
        permissionMode: PERMISSION_MODE[turnOptionsOf(session.id).permission],
        allowedTools: [...new Set([...(approved[session.id] ?? []), ...allow])],
        cwd: provider.restricted ? null : cwd,
        effort: turnOptionsOf(session.id).effort as Effort,
        restricted: Boolean(provider.restricted),
      }), PREPARE_LIMIT_MS, 'starting the provider');
      return;
    }
    // A flow's step is not told it may open pages: it reports back, the user did not ask to see anything.
    // Agents told about the Preview in their system prompt are not told again in every message that says "run" or "open".
    const openHint =
      !isWorkerChat(session.id) &&
      !KNOWS_PREVIEW.has(session.agent) &&
      OPEN_WORDS.test(dispatchBody)
        ? AGENTDECK_HINT
        : '';
    // How to delegate (DELEGATE_HINT): said when the conversation starts, and once for a chat this app never told (an
    // imported one), not with every turn: the agent keeps it in its history. Sub chats do not delegate.
    const hintKey = `${session.id}|${session.agent}`;
    // Auto-orchestrate (model popover): when it is off, the agent is told once not to hand work out by itself; when it
    // is switched on again, the delegation note is given again.
    const told = orchestrateTold.get(hintKey);
    const orchestrate = automation.orchestrate;
    const nativeDelegation = NATIVE_DELEGATION.has(session.agent);
    const previouslyToldToUseAgentDeck = delegateHinted.has(hintKey) || priorDelegateHinted.has(hintKey);
    const delegateHint = isWorkerChat(session.id)
      ? ''
      : nativeDelegation
        ? previouslyToldToUseAgentDeck && !nativeDelegationHinted.has(hintKey)
          ? NATIVE_DELEGATION_HINT
          : ''
        : orchestrate
        ? !resume || !delegateHinted.has(hintKey) || told === false
          ? DELEGATE_HINT
          : ''
        : told !== false
          ? NO_DELEGATE_HINT
          : '';
    // Later turns: a short reminder for nontrivial prompts only (not twice in a row for a short one, and not with inline
    // @ai mentions, which already carry delegation instructions).
    let reminder = '';
    if (orchestrate && !nativeDelegation && !isWorkerChat(session.id) && !delegateHint && !preface.includes('inline sub-agent requests')) {
      const kinds = /Project "[^"]*" is at /.test(preface) ? [{ kind: 'project' }] : [];
      if (looksNontrivial(text, kinds) && !(reminded.get(hintKey) && !isLongPrompt(text))) reminder = DELEGATE_REMINDER;
    }
    if (!isWorkerChat(session.id)) reminded.set(hintKey, Boolean(reminder));
    if (!isWorkerChat(session.id)) orchestrateTold.set(hintKey, orchestrate);
    if (delegateHint === DELEGATE_HINT) rememberDelegateHint(hintKey);
    else if (delegateHint === NATIVE_DELEGATION_HINT) rememberNativeDelegationHint(hintKey);
    // The reply conventions (house-style.ts): only when the conversation starts, the agent keeps them in its history.
    const styleHint = !resume && !isWorkerChat(session.id) ? houseStyleHint(replyStyle.houseStyle, replyStyle.houseStyleText) : '';
    // The project's memory (project-memory.ts): read from its file when a conversation starts, so every agent of the
    // project begins with what earlier ones learned.
    const memoryNote = !resume && !isWorkerChat(session.id) && cwd && replyStyle.projectMemory ? memoryHint(true, cwd, await within(readProjectMemory(cwd), PREPARE_LIMIT_MS, "reading the project's memory", () => '')) : '';
    if (!runReplies.has(turn)) return;
    // The user's way of talking (user-profile.ts): learnt quietly from each prompt of a main chat, and told to the agent
    // when a conversation starts. Workers keep their own language rule.
    const mainChat = !isWorkerChat(session.id) && asked?.role === 'user';
    if (mainChat && replyStyle.userProfile) learnFromPrompt(text);
    const profileNote = !resume && mainChat && replyStyle.userProfile ? profileHint(loadProfile()) : '';
    // Task routing (Settings > Task roles) hands work to other agents, so it follows auto-orchestrate: off, nothing is routed.
    const routing = !isWorkerChat(session.id) && orchestrate ? taskRoleHint(taskRoles, routeLabel) : '';
    const hint = agentdeckLead(openHint, delegateHint, reminder, styleHint, memoryNote, profileNote, routing, LANGUAGE_RULE);
    // Pictures and documents are on disk: the prompt names their paths, which the agent opens with its own tools.
    const attached =
      (dispatchPaths.length
        ? `Attached image${dispatchPaths.length > 1 ? 's' : ''} (open with the Read tool):\n${dispatchPaths.map((path) => `- ${path}`).join('\n')}`
        : '') +
      (files.length
        ? `${dispatchPaths.length ? '\n\n' : ''}Attached file${files.length > 1 ? 's' : ''} (open with the Read tool or a shell command):\n${files.map((file) => `- ${file.path}${file.note ? ` (${file.note})` : ''}`).join('\n')}`
        : '');
    // `/compact` (auto-compress) goes as it is: anything before it would make it a plain message.
    const prompt = text === COMPACT_COMMAND
      ? text
      : hint +
      context +
      (attached
        ? `${dispatchBody || (files.length && !dispatchPaths.length
          ? 'Read the attached file. If this conversation shows that the user is returning a task prompt you just drafted or explicitly asked them to run, carry it out within that prompt’s scope instead of only summarizing it. Otherwise, treat the file as context and ask what they want done if the intent is unclear.'
          : 'Look at the attached image.')}\n\n${attached}`
        : dispatchBody);
    logRequest(
      session.id,
      session.title,
      workspace.projects.find((project) => project.id === session.projectId)?.name ?? '',
      agentLabel(session.agent, selectedModel ?? undefined),
      selectedModel,
      [
        `Effort: ${turnOptionsOf(session.id).effort}`,
        `Permission: ${turnOptionsOf(session.id).permission}`,
        `Conversation: ${resume ? `continues ${cliId}` : cliId ? `new (${cliId})` : 'new'}`,
        `Folder: ${cwd ?? 'none'}`,
        `Message: ${prompt.length} characters${images.length ? `, ${images.length} picture(s)` : ''}${files.length ? `, ${files.length} file(s)` : ''}`,
      ].join('\n'),
      session.agent,
      turn,
    );
    await within(
      sendAgentTurn({
        session: session.id,
        turn,
        messageId: asked?.role === 'user' ? asked.id : undefined,
        agent: session.agent,
        prompt,
        keepOpen: !listener && !isWorkerChat(session.id),
        agentSession: cliId,
        resume,
        // A model of another agent (left over from a switch) is never sent: the agent's own first model goes instead.
        model: selectedModel,
        permissionMode: PERMISSION_MODE[turnOptionsOf(session.id).permission],
        effort: turnOptionsOf(session.id).effort as Effort,
        allowedTools: [...new Set([...(approved[session.id] ?? []), ...allow])],
        cwd,
      }),
      PREPARE_LIMIT_MS,
      'starting the agent',
    );
  })().catch((error: unknown) => {
    // A start that timed out may still come up late: stop it, or its CLI runs next to the retry on the same conversation.
    if (/^Error: Timed out after .* starting the /.test(String(error))) void stopAgentTurn(session.id, turn).catch(() => undefined);
    handleAgentEvent(
      session.id,
      { kind: 'error', message: String(error) },
      turn,
    );
  });
}

/**
 * The user's answer to an approval card. "Allow" and "Always allow" let the CLI run the refused tools and
 * continue the work in a new turn; "Deny" just closes the card.
 */
export function answerPermission(
  messageId: string,
  blockIndex: number,
  answer: 'once' | 'always' | 'deny',
) {
  const session = activeSession();
  const block = chat.messages.find((message) => message.id === messageId)
    ?.blocks[blockIndex];
  // A Copilot turn is still running while its card waits: the answer goes to the SDK request, not to a new turn.
  const waiting = session?.agent === 'copilot';
  if (
    !session ||
    block?.kind !== 'permission' ||
    block.state !== 'pending' ||
    (!waiting && replies[session.id])
  )
    return;
  for (let index = 0; index < block.requests.length; index++) advanceRun(session.id, { type: 'waitEnd', kind: 'approval' });
  const entries = block.requests.map((request) => request.allow);
  if (answer === 'always') {
    approved[session.id] = [
      ...new Set([...(approved[session.id] ?? []), ...entries]),
    ];
    setPref('permissions.allowed', JSON.parse(JSON.stringify(approved)));
  }
  if (waiting) {
    // The card shows the answer at once; if the SDK no longer waits for it (the turn was gone), it says so instead.
    block.state = answer === 'deny' ? 'denied' : 'allowed';
    settleKeptPermission(session.id, messageId, blockIndex, block.state);
    persistLocal();
    void Promise.all(block.requests.map((request) => answerCopilotPermission(request.id, answer !== 'deny')))
      .catch(() => {
        block.state = 'expired';
        settleKeptPermission(session.id, messageId, blockIndex, 'expired');
        persistLocal();
      });
    if (answer === 'deny') sendNextQueued(session.id);
    return;
  }
  if (answer === 'deny') {
    block.state = 'denied';
    settleKeptPermission(session.id, messageId, blockIndex, 'denied');
    persistLocal();
    // A worker's task waits on this card: a "no" ends it instead of leaving it hanging until its time limit.
    answerWorkerApproval(session.id, false);
    sendNextQueued(session.id);
    return;
  }
  block.state = 'allowed';
  settleKeptPermission(session.id, messageId, blockIndex, 'allowed');
  persistLocal();
  answerWorkerApproval(session.id, true);
  startTurn(
    session,
    'Permission granted. Please continue with what you were doing.',
    [],
    entries,
  );
}

/** Adds the typed message to the open session. The first message turns a draft into a real session. */
/** The assistant message each live turn is writing into, per session. */
export const replies = $state<Record<string, string>>({});
/** The reply object of each running turn. */
export const liveReplies: Record<string, Message> = {};
/** Every live provider run owns its reply by turn id; primary aliases above remain for legacy queue/status UI. */
export const runReplies = new Map<string, { sessionId: string; reply: Message; listener: boolean; ask?: Message; backgroundTools: Set<string> }>();
export const sessionLanes = $state<Record<string, RunLane[]>>({});

export function updateLane(sessionId: string, lane: RunLane) {
  const lanes = sessionLanes[sessionId] ?? [];
  // Every streamed delta reports its lane again: an unchanged lane is left as it is, so the whole list is not rebuilt
  // (and every reader of it woken) a hundred times per answer.
  const same = lanes.find((item) => item.id === lane.id);
  if (same && same.role === lane.role && same.state === lane.state && same.backgroundWork === lane.backgroundWork && lane.state !== 'done' && lane.state !== 'failed' && lane.state !== 'stopped') return;
  const next = lanes.filter((item) => item.id !== lane.id);
  if (lane.state === 'done' || lane.state === 'failed' || lane.state === 'stopped') {
    sessionLanes[sessionId] = next;
  } else {
    sessionLanes[sessionId] = [...next, lane];
  }
}

export function canListenToSession(sessionId: string): boolean {
  return deliveryFor(sessionLanes[sessionId] ?? []) === 'listener';
}
/** Groups a live reply with follow-ups sent before the whole queue goes idle. */
export function continueCurrentRun(sessionId: string): string {
  const group = continuationGroups[sessionId] ?? liveReplies[sessionId]?.runGroup ?? nextId();
  continuationGroups[sessionId] = group;
  const current = liveReplies[sessionId] ?? messagesOf(sessionId).at(-1);
  if (current?.role === 'assistant') current.runGroup = group;
  return group;
}
/** The message that started each running turn. Reactive, so the Tasks panel can show the prompt while the reply runs. */
export const liveAsks = $state<Record<string, Message>>({});
/** For a running hand-back turn (it has no message of its own): the message whose delegate cards it answers. */
export const liveHandouts: Record<string, Message> = {};
/** For a hand-back that answers no message (a background task's report): its reply just follows the chat's last message. */
export const NO_HANDOUT: Message = {
  id: 'no-handout',
  role: 'user',
  time: '',
  blocks: [],
};

export const askText = (message: Message) =>
  message.blocks
    .filter((block) => block.kind === 'response')
    .map((block) => block.text)
    .join('\n')
    .trim();

/**
 * A chat opened again while its reply is still being written: what was loaded (a saved copy, or the file as the CLI
 * has written it so far) does not hold the running turn, or holds only part of it. The live question and reply go
 * on the end, in place of any copy of that question, so the text keeps growing where the reader left it.
 */
function withPrimaryLive(sessionId: string, list: Message[]): Message[] {
  const reply = liveReplies[sessionId];
  const handout = liveHandouts[sessionId];
  if (reply && handout === NO_HANDOUT)
    return list.includes(reply) ? list : [...list, reply];
  if (reply && handout && !list.includes(reply)) {
    // The file may already hold part of the reply, behind the message that handed work out: that is replaced by the live one.
    const at = list.findLastIndex((message) => message.id === handout.id);
    return at >= 0
      ? [...list.slice(0, at), handout, reply]
      : [...list, handout, reply];
  }
  const asked = liveAsks[sessionId];
  if (!reply || !asked || list.includes(reply)) return list;
  const want = askText(asked);
  let cut = list.length;
  // Only the last few can be this turn: an older message with the same words is another turn.
  for (
    let index = list.length - 1;
    index >= Math.max(0, list.length - 3);
    index--
  ) {
    if (list[index].role === 'user' && askText(list[index]) === want) {
      cut = index;
      break;
    }
  }
  return [...list.slice(0, cut), asked, reply];
}

/** Reattaches every live lane after transcript loading, including independent replies beside background work. */
export function withLive(sessionId: string, list: Message[]): Message[] {
  const visible = withPrimaryLive(sessionId, list);
  const next = [...visible];
  for (const run of runReplies.values()) {
    if (run.sessionId !== sessionId || !run.listener) continue;
    if (run.ask && !next.some((message) => message.id === run.ask!.id)) next.push(run.ask);
    if (!next.some((message) => message.id === run.reply.id)) next.push(run.reply);
  }
  return next;
}

/** Messages of a session: the live list for the active one, the remembered list for the others. */
export function messagesOf(sessionId: string): Message[] {
  if (sessionId === workspace.activeId) {
    if (chat.messages === mockMessages) chat.messages = [...mockMessages];
    return chat.messages;
  }
  return (localMessages[sessionId] ??= []);
}

/** What the latest user turn still waits for: a permission request, or questions the agent asked. */
export function pendingInput(sessionId: string): 'permission' | 'question' | null {
  const list = messagesOf(sessionId);
  const messages =
    liveReplies[sessionId] && !list.includes(liveReplies[sessionId])
      ? [...list, liveReplies[sessionId]]
      : list;
  for (let index = messages.length - 1; index >= 0; index--) {
    const message = messages[index];
    if (message.role === 'user') return null;
    // The user cancelled what this reply ended with: nothing waits on it.
    if (message.cancelled) return null;
    if (
      message.blocks.some(
        (block) => block.kind === 'permission' && block.state === 'pending',
      )
    )
      return 'permission';
    if (
      message.blocks.some(
        (block) =>
          block.kind === 'response' &&
          parseQuestions(block.text).questions.length > 0,
      )
    )
      return 'question';
  }
  return null;
}

/** A queued task must wait while the latest user turn still has a question or permission request to resolve. */
export const needsUserInput = (sessionId: string): boolean => pendingInput(sessionId) !== null;

/** Told to the agent with a message that was written instead of answering its questions. What a skipped question means
 *  depends on its kind: an ordinary one is a preference the agent may settle itself; one marked `required` is not. */
export const SKIPPED_QUESTIONS =
  'The user did not answer your questions and wrote this instead. Treat it as their answer; for anything it leaves open, use your best judgement and say what you assumed. Do not ask the same questions again unless you still cannot go on without them.';
export const SKIPPED_REQUIRED =
  'The user did not answer the questions you marked required and wrote this instead. Do not guess those answers and do not do the work that depends on them: answer what they wrote, then ask again only what is still missing.';

/** The questions the latest user turn left open (the ones of the newest reply that has any). */
export function openQuestions(sessionId: string): Question[] {
  const list = messagesOf(sessionId);
  for (let index = list.length - 1; index >= 0; index--) {
    const message = list[index];
    if (message.role === 'user') return [];
    for (const block of message.blocks) {
      if (block.kind !== 'response') continue;
      const questions = parseQuestions(block.text).questions;
      if (questions.length) return questions;
    }
  }
  return [];
}
/** Told to the agent with a message that was written instead of answering its permission request. */
export const SKIPPED_PERMISSION =
  'The user did not answer your permission request and wrote this instead. The request was cancelled: the command or tool call was NOT run and nothing it would have changed was changed. Do not assume it ran; ask again only if it is still needed.';

/** The line the chat shows once a cancelled question or request has nothing left behind it (in the style of the note for a
 *  reply the app closed on). */
const CANCEL_NOTE = '> ✕ Task cancelled. Send a message to go on.';
/** Chats where the user cancelled while messages were queued behind: those run first, the note comes after them. */
const cancelNotes = new Set<string>();

function addCancelNote(sessionId: string) {
  cancelNotes.delete(sessionId);
  const session = workspace.sessions.find((item) => item.id === sessionId);
  const list = messagesOf(sessionId);
  const last = list.at(-1);
  const lastBlock = last?.blocks.at(-1);
  if (last?.role === 'assistant' && lastBlock?.kind === 'response' && lastBlock.text === CANCEL_NOTE) return;
  list.push({
    id: crypto.randomUUID(),
    role: 'assistant',
    time: clock(new Date().toISOString()),
    at: new Date().toISOString(),
    agent: session?.agent,
    blocks: [{ kind: 'response', text: CANCEL_NOTE }],
  });
  persistLocal();
}

/** The user cancels what the agent waits for (its questions or a permission request): nothing is sent to the agent. A
 *  message queued behind it goes out next and the chat works through the queue; when none is left, the chat says the
 *  task was cancelled. */
export function cancelPending(sessionId: string) {
  if (replies[sessionId] || !pendingInput(sessionId)) return;
  const list = messagesOf(sessionId);
  for (let index = list.length - 1; index >= 0; index--) {
    const message = list[index];
    if (message.role === 'user') break;
    if (message.blocks.some((block) => block.kind === 'permission' && block.state === 'pending')) closePendingPermissions(sessionId);
    message.cancelled = true;
  }
  persistLocal();
  if ((queued[sessionId]?.length ?? 0) > 0) {
    cancelNotes.add(sessionId);
    sendNextQueued(sessionId);
  } else addCancelNote(sessionId);
}

/** Closes the permission requests the latest user turn left unanswered, as "not answered" (what the cards say), and for a
 *  Copilot turn, which holds the request open, refuses it so that turn can end. */
export function closePendingPermissions(sessionId: string) {
  const list = messagesOf(sessionId);
  for (let index = list.length - 1; index >= 0; index--) {
    const message = list[index];
    if (message.role === 'user') break;
    for (const block of message.blocks) {
      if (block.kind !== 'permission' || block.state !== 'pending') continue;
      if (message.agent === 'copilot') for (const request of block.requests) void answerCopilotPermission(request.id, false).catch(() => {});
      block.state = 'expired';
    }
  }
  persistLocal();
}

/** Dispatches one queued message only after the active turn and all user decisions are complete. */
export function sendNextQueued(sessionId: string) {
  const listenerAvailable = () => canListenToSession(sessionId) && !needsUserInput(sessionId) && Boolean(queued[sessionId]?.length) && !queued[sessionId][0].mentions?.some((mention) => mention.kind === 'ai' || mention.kind === 'workflow');
  if (queuePaused[sessionId] || (hasTurnWork(sessionId) && !listenerAvailable())) return;
  // Let the current event stack finish before taking an item. Ctrl/Cmd+Enter stops the current turn and
  // starts its message synchronously; taking an older queue item before that message starts would
  // remove it from the queue, then `send` would reject it because the priority turn is now running.
  setTimeout(() => {
    if (queuePaused[sessionId] || (hasTurnWork(sessionId) && !listenerAvailable())) return;
    const listener = listenerAvailable();
    // A context near the window is compacted before the next message; the compaction turn calls back here when done.
    if (!listener && compressIfNeeded(sessionId)) return;
    // Run only one queued user message per provider turn; the rest stay in line until it finishes.
    const next = dequeue(queued, sessionId);
    if (next) logFlow(sessionId, 'queue', `sending one waiting message (${queued[sessionId]?.length ?? 0} remain)`);
    if (next) {
      if (listener) {
        sendImmediate(sessionId, next);
        return;
      }
      send(sessionId, next.text, next.images, next.elements, next.mentions, {
        files: next.files,
        runGroup: next.runGroup,
        messageId: next.messageId,
      });
      return;
    }
    if (backlog.get(sessionId)?.length) {
      flushReports(sessionId, 0);
      return;
    }
    if (hasOutstandingSessionWork(sessionId)) return;
    // The queue behind a cancelled question has run out: say the task was cancelled.
    if (cancelNotes.has(sessionId)) addCancelNote(sessionId);
    delete continuationGroups[sessionId];
    notifySessionWorkDone(sessionId);
  }, 0);
}

/** Expiry for a finished reply's presentation state when its virtual blocks are not mounted. */
export const RENDER_WAIT_LIMIT_MS = 15_000;

/** A finished reply can still be animating its final Markdown; presentation tracking does not block the queue. */
export const renderWaiters: Record<
  string,
  { messageId: string; blocks: Set<number> }
> = $state({});

/** Whether the reply is still being written out on screen (its text is not all rendered yet, see `replyBlockRendered`). */
export function replyRendering(sessionId: string, messageId: string): boolean {
  return renderWaiters[sessionId]?.messageId === messageId;
}

/** Called after a response block's final text has reached the DOM. */
export function replyBlockRendered(
  sessionId: string,
  messageId: string,
  blockIndex: number,
) {
  const waiting = renderWaiters[sessionId];
  if (!waiting || waiting.messageId !== messageId) return;
  waiting.blocks.delete(blockIndex);
  if (waiting.blocks.size) return;
  delete renderWaiters[sessionId];
  sendNextQueued(sessionId);
}

/** If the user leaves the chat before its final animation ends, let its queued work continue in the background. */
export function releaseReplyRender(sessionId: string) {
  if (!renderWaiters[sessionId]) return;
  delete renderWaiters[sessionId];
  sendNextQueued(sessionId);
}
