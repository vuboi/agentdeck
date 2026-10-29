import { type Message } from '../mock';
import { parseHandoff, agentdeckLead } from '../../tasks/delegate-task';
import { HANDBACK_HEAD } from '../runs/handback';
import { logError } from '../../app/platform/error-log.svelte';
import { inTauri, clock } from '../../app/platform/history';
import { mimeOfPath } from '../agent';
import { enqueue, enqueueNext } from '../runs/queue';
import { getPref, setPref } from '../../app/platform/prefs';
import { presetById, presetOf, type WorkflowPreset } from '../../tasks/workflows.svelte';
import { automation, contextWindow, COMPRESS_AT } from '../../app/platform/automation.svelte';
import { needsContext } from '../recovery/context-need';
import { activeSession, localMessages, nextId, persistLocal, workspace, type Attachment, type Effort, type FileAttachment, type Mention, type PageElement, type SessionItem } from './state.svelte';
import { afterSwitch, beginSessionWork, continuationGroups, hiddenPrompt, isRunning, modelFor, queuePaused, queued, stopTurn, switchingAgent, turnOptions, withElements, type Queued } from './turns.svelte';
import { SKIPPED_PERMISSION, SKIPPED_QUESTIONS, SKIPPED_REQUIRED, canListenToSession, closePendingPermissions, continueCurrentRun, liveReplies, messagesOf, needsUserInput, openQuestions, pendingInput, pendingNew, replies, sendNextQueued, startTurn } from './run.svelte';
import { recoveries, resumeAutoChoice, resumeSince, resumes } from './events.svelte';
import { beginWorkflowIntegration, beginWorkflowRun, delegateRuns, mentionContext, statsOf, type DelegateReports } from './delegation.svelte';
import { LANGUAGE_RULE } from './orchestration.svelte';
import { otherTurns, transcripts } from './history.svelte';

/** The main agent finished the turn that got the answers: the run is over. */
/** Puts the answers into the chat that asked, and runs its agent on them as soon as it is free. */
/** A step that reviews or verifies: it looks at the result itself when it can, and ends with a verdict the owner reads. */
/** Sends an answer to a question asked by an agent in its chat. */
export function answerQuestions(sessionId: string, text: string): boolean {
  if (!text.trim() || replies[sessionId]) return false;
  send(sessionId, text, [], []);
  return true;
}

/** Longest worker report put whole into the integration prompt; the rest stays in the worker's own chat. */
const HANDBACK_ANSWER_MAX = 8000;
/** An answer that says the work failed, is blocked or needs a new plan: then the lead has to act, not only summarise. */
const TROUBLE = /VERDICT:\s*FAIL|STATUS:\s*(?:blocked|needs-plan-change)/i;

/** The prompt that returns background worker reports to the lead model. */
export function handbackPrompt(batch: DelegateReports): string {
  const cut = (text: string) => (text.length > HANDBACK_ANSWER_MAX ? `${text.slice(0, HANDBACK_ANSWER_MAX)}\n… (cut here: the rest is in that agent's own chat)` : text);
  const answers = batch.results.map((result) => `### ${result.label}\n${cut(result.text)}`).join('\n\n');
  // When nothing went wrong the lead only reads the answers and replies, like a manager reading a report: no tools, no
  // second round of the work. It re-checks and fixes (see `owner`) only when an answer reports trouble.
  const trouble = batch.results.some((result) => {
    const handoff = parseHandoff(result.text);
    return handoff?.status === 'blocked' || handoff?.status === 'needs-plan-change' || TROUBLE.test(result.text);
  });
  const memo = batch.memo ? `\n\n${batch.memo}` : '';
  const finish = batch.orchestrator ? `${batch.orchestrator}\n\nThe user's request: ${batch.task}` : `Using their answers, finish the user's request: ${batch.task}`;
  // A step that reviews can find the work wrong. The agent that gets the answers then owns the result: it fixes what is
  // wrong and cleans up after the steps, instead of only reporting it.
  const owner =
    "You own the result. Fix only what blocks the request: a step's VERDICT: FAIL, a failed check, or work that is wrong/unfinished. Then do not just report: see all changes (handoff FILES; git status, git diff), undo wrong ones, delete leftovers (debug output, stray files, notes), redo correctly, run the project's checks, end with what you fixed/removed. Suggestions and risks a step lists are not failures: do not edit for them, list them for the user in one short line each. If all is right, say so in one line.";
  const light = "The answers above are compact digests; each full answer is in that agent's own chat. Do not redo or re-check their work: run no commands, read no files, look at no git state. Write the reply to the user from the digests: combine them, say in a line where they disagree, what was done, and what the user should do next. Short.";
  return `${HANDBACK_HEAD}\n\n${answers}${memo}\n\n${finish}\n\n${trouble ? owner : light}\n\n${LANGUAGE_RULE}`;
}

/**
 * Send from the message box. While busy: Enter queues at the end, Alt+Enter queues next, Ctrl/Cmd+Enter stops and sends now.
 */
/** The picture of each picked element, sent with the message as an image next to its markup. */
const elementPictures = (elements: PageElement[]): Attachment[] =>
  elements.flatMap((item) =>
    item.picture || item.picturePath
      ? [
          {
            id: crypto.randomUUID(),
            name: `${item.tag}.png`,
            mime: 'image/png',
            src: item.picturePath ? '' : (item.picture ?? ''),
            path: item.picturePath,
          },
        ]
      : [],
  );

export function sendMessage(text: string, images: Attachment[] = [], elements: PageElement[] = [], mentions: Mention[] = [], whileRunning: 'queue' | 'next' | 'interrupt' | 'resume' = 'queue', files: FileAttachment[] = []) {
  const session = activeSession();
  // A mention only applies while its inline token is still present in the draft.
  mentions = mentions.filter((mention) => text.includes(mention.kind === 'workflow' ? `/w ${mention.label}` : `@${mention.label}`));
  const selectedFlow = mentions.find((mention) => mention.kind === 'workflow');
  if (selectedFlow && !text.replace(`/w ${selectedFlow.label}`, '').trim()) return;
  images = [...images, ...elementPictures(elements)];
  if (!session || (!text.trim() && images.length === 0 && elements.length === 0 && files.length === 0)) return;
  // A task for another agent needs words to say what the task is.
  if (mentions.some((mention) => mention.kind === 'ai') && !text.trim()) return;
  beginSessionWork(session.id);
  if (whileRunning === 'resume') {
    const runGroup = continueCurrentRun(session.id);
    enqueue(queued, session.id, {
      messageId: nextId(),
      text,
      images,
      elements,
      mentions,
      files,
      runGroup,
    });
    delete queuePaused[session.id];
    sendNextQueued(session.id);
    return;
  }
  // Explicit provider background work may outlive its current answer. Give a new prompt its own CLI conversation
  // and reply message while that waiter continues; never use this route for ambiguous waits or another active listener.
  if (whileRunning === 'queue' && canListenToSession(session.id) && !needsUserInput(session.id) && !mentions.some((mention) => mention.kind === 'ai' || mention.kind === 'workflow')) {
    const preface = handoffDigest(messagesOf(session.id), 'The user sent a new message while your previous task continues in the background. Handle this request independently and keep your reply under the new message. Avoid modifying files likely owned by the still-running task; if the requests conflict, explain the conflict before changing shared files. Do not claim the background task has finished.');
    send(session.id, text, images, elements, mentions, {
      files,
      listener: true,
      preface,
    });
    return;
  }
  // Questions are waiting and nothing runs: writing a message is the way to skip them. It goes out at once (not behind
  // the questions), says so to the agent, and the card closes because a newer message of the user follows it.
  // The same goes for a permission request: writing instead of answering it cancels it, and the agent is told the command
  // did not run.
  if (!replies[session.id] && !isRunning(session.id)) {
    const waiting = pendingInput(session.id);
    if (waiting === 'question' || waiting === 'permission') {
      if (waiting === 'permission') closePendingPermissions(session.id);
      const runGroup = continueCurrentRun(session.id);
      send(session.id, text, images, elements, mentions, {
        files,
        runGroup,
        preface: agentdeckLead(waiting === 'permission' ? SKIPPED_PERMISSION : openQuestions(session.id).some((question) => question.required) ? SKIPPED_REQUIRED : SKIPPED_QUESTIONS),
      });
      return;
    }
  }
  if (replies[session.id] || isRunning(session.id) || needsUserInput(session.id)) {
    const runGroup = continueCurrentRun(session.id);
    if (whileRunning !== 'interrupt') {
      // Every queued message gets its own turn; Alt+Enter puts it next without interrupting the current answer.
      const item = {
        messageId: nextId(),
        text,
        images,
        elements,
        mentions,
        files,
        runGroup,
      };
      if (whileRunning === 'next') enqueueNext(queued, session.id, item);
      else enqueue(queued, session.id, item);
      return;
    }
    if (needsUserInput(session.id)) {
      enqueue(queued, session.id, {
        messageId: nextId(),
        text,
        images,
        elements,
        mentions,
        files,
        runGroup,
      });
      return;
    }
    stopTurn(session.id, true);
    send(session.id, text, images, elements, mentions, {
      files,
      runGroup,
      messageId: nextId(),
    });
    return;
  }
  send(session.id, text, images, elements, mentions, {
    files,
  });
}

/** Sends a queued message now: the reply running in the chat is stopped (what it wrote stays), and this one goes out. */
export function steerQueued(sessionId: string, item: Queued) {
  beginSessionWork(sessionId);
  const rest = (queued[sessionId] ?? []).filter((entry) => entry !== item);
  if (rest.length) queued[sessionId] = rest;
  else delete queued[sessionId];
  if (replies[sessionId]) stopTurn(sessionId, true);
  send(sessionId, item.text, item.images, item.elements, item.mentions, {
    files: item.files,
    runGroup: item.runGroup,
    messageId: item.messageId,
  });
}

/** Messages waiting in a chat (sent while its reply runs, not yet started). */
export const queuedCount = (sessionId: string) => queued[sessionId]?.length ?? 0;

/** Sends a delegated worker its task again: the last message of its own chat that was written by the user. For a worker
 *  that failed or was interrupted; the new reply goes in the worker's chat, where the run can open it. */
export function retryWorker(childId: string) {
  const last = [...messagesOf(childId)].reverse().find((message) => message.role === 'user');
  const text =
    last?.blocks
      .map((block) => (block.kind === 'response' ? block.text : ''))
      .filter(Boolean)
      .join('\n\n') ?? '';
  if (!text.trim()) return;
  beginSessionWork(childId);
  send(childId, text, [], [], [], { files: [] });
}

/** A message as it is recognised again in the chat's file (where it has another id than the one it got here). */
export const signature = (message: Message) =>
  `${message.role}|${message.blocks
    .map((block) => (block.kind === 'response' ? block.text.trim() : ''))
    .join('')
    .slice(0, 300)}`;

/** Messages taken out of a saved chat by an edit: its file still has them, so they are hidden when it is read. */
export const hidden: Record<string, string[]> = {};
void getPref<Record<string, string[]>>('chat.hidden').then((saved) => {
  if (saved) Object.assign(hidden, saved);
});

/**
 * Edits one of your messages, as in ChatGPT: it and everything after it go, and the new text is sent in its
 * place (with the same pictures). A reply still running is stopped first.
 */
export function editMessage(messageId: string, text: string, inlinedFilePaths: string[] = []) {
  const session = activeSession();
  if (!session || !text.trim()) return;
  beginSessionWork(session.id);
  const list = messagesOf(session.id);
  const index = list.findIndex((message) => message.id === messageId && message.role === 'user');
  if (index === -1) return;
  if (replies[session.id]) stopTurn(session.id, true);
  delete queued[session.id];
  delete queuePaused[session.id];
  delete continuationGroups[session.id];
  // The pictures, page elements and files sent with it go again too: an edit changes the words, not what was attached.
  const { images, elements, files: attachedFiles } = attachmentsOf(list[index]);
  const files = attachedFiles.filter((file) => !inlinedFilePaths.includes(file.path));
  const removed = list.splice(index);
  if (session.path) {
    hidden[session.id] = [...new Set([...(hidden[session.id] ?? []), ...removed.flatMap((message) => [message.id, signature(message)])])];
    setPref('chat.hidden', hidden);
    const others = otherTurns[session.id];
    if (others) otherTurns[session.id] = others.filter((message) => !removed.includes(message));
    transcripts.delete(session.id);
  } else {
    localMessages[session.id] = list;
    persistLocal();
  }
  // The CLI's conversation still holds the turns just removed: resumed, the agent would see them (and answer them again).
  // Every agent of the chat starts a new one, told what is left of the chat.
  session.agentStarted = false;
  session.agentSession = undefined;
  for (const own of Object.values(session.agentIds ?? {})) {
    own.id = '';
    own.started = false;
  }
  session.forkContext = (session.forkContext ?? '') + handoffDigest(list, EDIT_LEAD);
  pendingNew.add(session.id);
  send(session.id, text, images, elements, [], { files });
}

/** What a message of yours carried besides its words, as composer attachments again (edit, copy all). */
export function attachmentsOf(message: Message): {
  images: Attachment[];
  elements: PageElement[];
  files: FileAttachment[];
} {
  const images: Attachment[] = message.blocks.flatMap((block) =>
    block.kind === 'image'
      ? [
          {
            id: nextId(),
            name: block.name,
            mime: block.path ? mimeOfPath(block.path) : block.src.slice(5, block.src.indexOf(';')) || 'image/png',
            src: block.src,
            path: block.path,
          },
        ]
      : [],
  );
  const elements: PageElement[] = message.blocks.flatMap((block) =>
    block.kind === 'element'
      ? [
          {
            id: nextId(),
            tag: block.tag,
            text: block.text,
            selector: block.selector,
            html: block.html ?? '',
            around: block.around,
            box: {
              x: block.box?.x ?? 0,
              y: block.box?.y ?? 0,
              w: block.box?.w ?? 0,
              h: block.box?.h ?? 0,
            },
            style: block.style ?? {},
            url: block.url ?? '',
            title: block.title ?? '',
            note: block.note,
          },
        ]
      : [],
  );
  const files: FileAttachment[] = message.blocks.flatMap((block) =>
    block.kind === 'file'
      ? [
          {
            id: nextId(),
            name: block.name,
            path: block.path,
            size: block.size ?? 0,
          },
        ]
      : [],
  );
  return { images, elements, files };
}

/**
 * `extra.prompt` is what the agent is sent when it differs from the text shown in the chat, and `extra.preface` goes in
 * front of the prompt without being shown (context of a delegated task). `extra.silent` (the message that handed work
 * out) sends `extra.prompt` without any message in the chat: the agent's reply just continues after that message.
 */
/** A message of the user as the chat shows it: pictures, files and page elements first, then the words. */
function userMessage(body: string, images: Attachment[], elements: PageElement[], files: FileAttachment[], runGroup?: string, id = nextId()): Message {
  return {
    id,
    role: 'user',
    time: clock(new Date().toISOString()),
    blocks: [
      // A picture kept in its file is stored by its path only: no base64 in the chat's history.
      ...images.map((image) => ({
        kind: 'image' as const,
        src: image.path ? '' : image.src,
        name: image.name,
        path: image.path,
      })),
      ...files.map((file) => ({
        kind: 'file' as const,
        name: file.name,
        path: file.path,
        size: file.size,
      })),
      ...elements.map((item) => ({
        kind: 'element' as const,
        tag: item.tag,
        text: item.text,
        selector: item.selector,
        url: item.url,
        title: item.title,
        note: item.note,
        html: item.html,
        around: item.around,
        box: { ...item.box },
        style: { ...item.style },
        picture: item.picture,
        picturePath: item.picturePath,
      })),
      ...(body ? [{ kind: 'response' as const, text: body }] : []),
    ],
    at: new Date().toISOString(),
    ...(runGroup ? { runGroup } : {}),
  };
}

export function send(
  sessionId: string,
  text: string,
  images: Attachment[],
  elements: PageElement[],
  mentions: Mention[] = [],
  extra: {
    prompt?: string;
    preface?: string;
    files?: FileAttachment[];
    silent?: Message;
    runGroup?: string;
    messageId?: string;
    listener?: boolean;
  } = {},
) {
  const session = workspace.sessions.find((item) => item.id === sessionId);
  const flowMention = mentions.find((mention) => mention.kind === 'workflow');
  const requestedFlow = flowMention ? presetById(flowMention.id) : undefined;
  const rawBody = text.trim();
  const body = flowMention && requestedFlow ? rawBody.replace(`/w ${flowMention.label}`, '').trim() : rawBody;
  const files = extra.files ?? [];
  if (!session || (replies[sessionId] && !extra.listener)) return;
  const messageId = extra.messageId ?? nextId();
  // Waits behind the account switch of its agent, whole and in order (see switchingAgent).
  if (switchingAgent(session.agent)) {
    const waiting = afterSwitch.get(session.agent) ?? [];
    waiting.push(() => send(sessionId, text, images, elements, mentions, extra));
    afterSwitch.set(session.agent, waiting);
    return;
  }
  // The user took over: a wait for a limit to reset is over. (The recovery's own turns are silent.)
  if (!extra.silent) {
    delete resumes[sessionId];
    delete resumeAutoChoice[sessionId];
    delete resumeSince[sessionId];
  }
  const messages = messagesOf(sessionId);
  if (extra.silent && extra.prompt && inTauri() && (session.agent === 'claude' || session.agent === 'codex' || session.agent === 'antigravity' || session.agent === 'opencode' || session.agent === 'custom')) {
    beginWorkflowIntegration(sessionId);
    startTurn(session, extra.prompt, [], [], extra.preface ?? '', [], extra.silent, extra.runGroup);
    if (!session.path) localMessages[session.id] = messages;
    persistLocal();
    return;
  }
  // Auto-compress: a Claude context near the model's window is compacted first; the message waits at the head of the
  // queue and goes once the compaction is done.
  if (!extra.prompt && !extra.preface && needsCompress(session, messages)) {
    queued[sessionId] = [
      {
        messageId,
        text,
        images,
        elements,
        mentions,
        files,
        runGroup: extra.runGroup,
      },
      ...(queued[sessionId] ?? []),
    ];
    compressContext(session);
    return;
  }
  messages.push(userMessage(body, images, elements, files, extra.runGroup, messageId));
  // A new request from the user: retries and agents tried are counted again from here.
  delete recoveries[sessionId];
  const preset = flowMention ? requestedFlow : presetOf(sessionId);
  const workflow = inTauri() && preset && preset.agents.length && !extra.listener && !extra.prompt ? preset : undefined;
  const preface = (extra.preface ?? '') + mentionContext(mentions) + (workflow ? workflowInstructions(workflow) : '');
  if (workflow) beginWorkflowRun(session.id, body, workflow);
  if (inTauri() && (session.agent === 'claude' || session.agent === 'codex' || session.agent === 'antigravity' || session.agent === 'opencode' || session.agent === 'copilot' || session.agent === 'custom')) startTurn(session, withElements(extra.prompt ?? body, elements), images, [], preface, files, undefined, extra.runGroup, Boolean(extra.listener));
  else if (inTauri()) {
    messages.push({
      id: nextId(),
      role: 'assistant',
      time: clock(new Date().toISOString()),
      blocks: [
        {
          kind: 'response',
          text: `⚠ ${session.agent} chats are not connected yet. Claude and Codex run from here for now.`,
        },
      ],
    });
  }
  // Imported sessions reload from disk; sessions that only exist here keep their messages in memory.
  if (!session.path) localMessages[session.id] = messages;
  persistLocal();
  if (session.draft) {
    session.draft = false;
    const firstLine = body.split('\n')[0] || files[0]?.name || images[0]?.name || elements[0]?.title || (elements.length ? 'Selected web content' : 'New chat');
    session.title = firstLine.length > 60 ? `${firstLine.slice(0, 57)}…` : firstLine;
  }
}

/** Turns a saved flow into instructions for the main model; it is a roster and coordination policy, not a step runner. */
function workflowInstructions(preset: WorkflowPreset): string {
  const roster = preset.agents.map((worker) => {
    const custom = worker.agent === 'custom' && worker.model.includes('::');
    const provider = custom ? worker.model.split('::', 1)[0] : worker.agent;
    const model = custom ? worker.model.slice(worker.model.indexOf('::') + 2) : worker.model;
    return `- ${worker.name}: provider=${provider}; model=${model || 'choose an available default'}; role=${worker.prompt.trim() || 'take an appropriate part of the task'}`;
  }).join('\n');
  const strategy = preset.strategy === 'parallel'
    ? 'Prefer delegating independent roles in parallel.'
    : preset.strategy === 'sequential'
      ? 'Prefer delegating in sequence when one role depends on another role’s result.'
      : 'Choose parallel or sequential delegation based on real task dependencies.';
  return [
    `[AgentDeck workflow: ${preset.name}]`,
    'You are the lead model for this request. Analyze the user request, choose the work breakdown, delegate the actual work to AgentDeck sub-agents, inspect their results, resolve conflicts, and write the final integrated answer yourself.',
    preset.leadPrompt.trim(),
    `Suggested sub-agent roles and exact provider/model preferences:\n${roster}`,
    strategy,
    `Use AgentDeck's delegate_task for these explicitly requested sub-agents, including when a worker uses your own provider. Start independent work together with wait=false, then check and collect each result before finalizing. Use wait=true for a task that depends on a previous result. Keep the number of concurrent workers at or below ${Math.max(1, Math.min(6, Math.round(preset.coordination?.maxParallel ?? 3)))}. If a requested provider/model is unavailable or fails from quota/server trouble, choose an available fallback and report the substitution. If no worker can take a task, do that task yourself.`,
    preset.orchestrator.trim() ? `Final integration guidance: ${preset.orchestrator.trim()}` : '',
  ].filter(Boolean).join('\n\n');
}

export const COMPACT_COMMAND = '/compact';

/** The context of `session` is near its model's window: the last Claude call read more than COMPRESS_AT of it. */
function needsCompress(session: SessionItem, messages: Message[]): boolean {
  if (!automation.compress || session.agent !== 'claude' || !session.agentStarted || !session.agentSession) return false;
  const last = [...messages].reverse().find((message) => message.role === 'assistant' && message.agent === 'claude');
  if (!last?.context || last.compressed) return false;
  return last.context >= contextWindow(last.model) * COMPRESS_AT;
}

/** Runs `/compact` in the chat's Claude conversation, shown as one "Compressing context" line. */
function compressContext(session: SessionItem) {
  startTurn(session, COMPACT_COMMAND);
  const reply = liveReplies[session.id];
  if (!reply) return;
  reply.compressed = true;
  reply.blocks = [{ kind: 'loading', label: 'Compressing context…' }];
}

/** A fork keeps the project and branch of its parent and nests under it in the tree. */
// Leaving or reloading the page keeps the open session and the local chats.
if (typeof window !== 'undefined') {
  window.addEventListener('pagehide', () => persistLocal());
}

/** The conversation so far for a new session, but only when the message it is about to get cannot stand without it:
 *  one that names its subject goes as it is (see context-need.ts), so a session is not loaded with history for nothing. */
export function handoffFor(sessionId: string, prompt: string): string {
  return needsContext(prompt) ? handoffDigest(messagesOf(sessionId)) : '';
}

/** What an agent is told after an account switch: the conversation so far, without the message it is about to get.
 *  The new session has no history of its own, so this is how the work in progress carries over. */
export function handoffDigest(list: Message[], lead = 'The account was switched, so this is a new session of the same conversation. Continue the work in progress; do not redo what is already done.'): string {
  // Not the message about to be answered (the last one of the user), and not a reply that only holds an error line.
  const last = list.at(-1);
  const onlyError = last?.role === 'assistant' && last.blocks.every((block) => block.kind === 'response' && (!block.text.trim() || block.text.trim().startsWith('⚠')));
  const before = last?.role === 'user' || onlyError ? list.slice(0, -1) : list;
  return forkDigest(before, lead);
}

/** Whether a chat's CLI switches its model in the same conversation (Claude's `set_model`, see warm.rs): a model change
 *  then keeps the conversation. The other agents start a new one (and carry the turns so far, see MODEL_LEAD). */
export const liveModel = (agent: string) => agent === 'claude';

/** What an edited message carries over: the chat before it (the edited message and what came after are gone). */
const EDIT_LEAD = 'The user edited an earlier message, so this is a new session of the same conversation, without what came after it. Continue from here; do not repeat work that the turns below do not show.';

/** What a model change carries over: a new CLI conversation cannot resume the old one, so the turns so far go with the next message. */
export const MODEL_LEAD = 'The model was changed, so this is a new session of the same conversation. Continue the work in progress; do not redo what is already done.';

/** How many of the last messages a digest keeps whole when "Compress context" is on, and how much of each older one. */
const DIGEST_WHOLE = 6;
const DIGEST_OLD_CHARS = 400;

/** What a fork is told of the conversation it came from: the turns so far, newest kept when it is long. */
function forkDigest(list: Message[], lead = 'This chat is a fork of an earlier conversation.'): string {
  const shown = list.filter((message) => !hiddenPrompt(message));
  // With "Compress context" on, only the last turns go whole: older ones keep their start, so a long chat moved to a new
  // session reads like a summary instead of filling the window with the full history.
  const whole = automation.compress ? Math.max(0, shown.length - DIGEST_WHOLE) : 0;
  const lines = shown
    .map((message, index) => {
      const limit = index < whole ? DIGEST_OLD_CHARS : 2500;
      const text = message.blocks
        .flatMap((block) => (block.kind === 'response' ? [block.text] : block.kind === 'command' ? [`(ran: ${block.command.slice(0, 160)})`] : []))
        .join('\n')
        .trim();
      return text ? `${message.role === 'user' ? 'User' : 'Assistant'}: ${text.length > limit ? `${text.slice(0, limit)}…` : text}` : '';
    })
    .filter(Boolean);
  if (!lines.length) return '';
  let digest = lines.join('\n\n');
  if (digest.length > 16000) digest = `…${digest.slice(-16000)}`;
  return `${lead} What was said so far:\n\n${digest}\n\n---\nNow the user says:\n`;
}

/**
 * A fork is a chat of its own: it gets a copy of the messages on screen, kept with the app's own chats, and no tie to the
 * parent's transcript file or CLI conversation. Sharing those made the fork show the parent's replies again each time it
 * was opened, run under the parent's conversation, and disturb the parent's status.
 */
export function forkSession(sourceId: string) {
  const source = workspace.sessions.find((item) => item.id === sourceId);
  if (!source) return;
  const id = nextId();
  const copy = (JSON.parse(JSON.stringify(messagesOf(sourceId))) as Message[]).map((message) => ({ ...message, live: false }));
  localMessages[id] = copy;
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  const { path, source: _origin, updatedAt, messageCount, agentSession, agentStarted, agentIds, conversationModel, agentModels, ...rest } = source;
  workspace.sessions.push({
    ...rest,
    id,
    title: `${source.title} (fork)`,
    status: 'idle',
    parentId: source.id,
    draft: false,
    messageCount: copy.length,
    updatedAt: new Date().toISOString(),
    forkContext: forkDigest(copy) || undefined,
  });
  workspace.activeId = id;
  persistLocal();
}
