import { type Message } from '../mock';
import { parseHandoff, agentdeckLead } from '../../tasks/delegate-task';
import { HANDBACK_HEAD } from '../runs/handback';
import { logError } from '../../app/platform/error-log.svelte';
import { inTauri, clock } from '../../app/platform/history';
import { sendAgentTurn, type AgentEvent } from '../agent';
import { addUsage, applyAgentEvent } from '../agent-reply';
import { persistCommandImage, finishCommandImages } from '../content/command-images';
import { enqueue, enqueueNext } from '../runs/queue';
import { getPref, setPref } from '../../app/platform/prefs';
import { presetOf } from '../../tasks/workflows.svelte';
import { automation, contextWindow, COMPRESS_AT } from '../../app/platform/automation.svelte';
import { needsContext } from '../recovery/context-need';
import { activeSession, localMessages, nextId, persistLocal, workspace, type Attachment, type Effort, type FileAttachment, type Mention, type PageElement, type SessionItem } from './state.svelte';
import { PERMISSION_MODE, afterSwitch, approved, beginSessionWork, continuationGroups, hiddenPrompt, isRunning, modelFor, queuePaused, queued, stopTurn, switchingAgent, turnOptions, withElements, type Queued } from './turns.svelte';
import { SKIPPED_PERMISSION, SKIPPED_QUESTIONS, SKIPPED_REQUIRED, canListenToSession, closePendingPermissions, continueCurrentRun, liveReplies, messagesOf, needsUserInput, openQuestions, pendingInput, pendingNew, replies, sendNextQueued, startTurn } from './run.svelte';
import { recoveries, resumeTries, resumes } from './events.svelte';
import { delegate, delegateRuns, mentionContext, statsOf, type DelegateBatch } from './delegation.svelte';
import { LANGUAGE_RULE, runWorkflow } from './workflow.svelte';
import { keepOtherTurn, otherTurns, transcripts } from './history.svelte';

/** The main agent finished the turn that got the answers: the run is over. */
export function onHandbackDone(sessionId: string, reply: Message, failed: boolean) {
  const run = delegateRuns.find(
    (item) => item.main === sessionId && item.handback === 'running',
  );
  if (!run) return;
  run.handback = failed ? 'failed' : 'done';
  run.endedAt = Date.now();
  run.handbackStats = statsOf(reply, run.handbackAt ?? run.startedAt);
}

/** Puts the answers into the chat that asked, and runs its agent on them as soon as it is free. */
/** A step that reviews or verifies: it looks at the result itself when it can, and ends with a verdict the owner reads. */
export const REVIEW_STEP = /\b(review|verify|check|test|qa)\b/i;
export const REVIEW_RULE =
  'Verify, do not only read: if the change shows on a page, open it with agentdeck browser_open (or browser_serve), use browser_screenshot and browser_console, and say what you saw. End with one line "VERDICT: PASS" or "VERDICT: FAIL - <blocking problems>". Risks and nice-to-haves go under "Suggestions", they are not a FAIL.';

/** Longest answer put whole into the hand-back prompt; the rest stays in the agent's own chat. */
const HANDBACK_ANSWER_MAX = 8000;
/** An answer that says the work failed, is blocked or needs a new plan: then the lead has to act, not only summarise. */
const TROUBLE = /VERDICT:\s*FAIL|STATUS:\s*(?:blocked|needs-plan-change)/i;

/** The prompt that hands the answers of the agents asked back to the chat's own agent. */
export function handbackPrompt(
  batch: Pick<DelegateBatch, 'results' | 'memo' | 'orchestrator' | 'task'>,
): string {
  const cut = (text: string) =>
    text.length > HANDBACK_ANSWER_MAX
      ? `${text.slice(0, HANDBACK_ANSWER_MAX)}\n… (cut here: the rest is in that agent's own chat)`
      : text;
  const answers = batch.results
    .map((result) => `### ${result.label}\n${cut(result.text)}`)
    .join('\n\n');
  // When nothing went wrong the lead only reads the answers and replies, like a manager reading a report: no tools, no
  // second round of the work. It re-checks and fixes (see `owner`) only when an answer reports trouble.
  const trouble = batch.results.some((result) => {
    const handoff = parseHandoff(result.text);
    return (
      handoff?.status === 'blocked' ||
      handoff?.status === 'needs-plan-change' ||
      TROUBLE.test(result.text)
    );
  });
  const memo = batch.memo ? `\n\n${batch.memo}` : '';
  const finish = batch.orchestrator
    ? `${batch.orchestrator}\n\nThe user's request: ${batch.task}`
    : `Using their answers, finish the user's request: ${batch.task}`;
  // A step that reviews can find the work wrong. The agent that gets the answers then owns the result: it fixes what is
  // wrong and cleans up after the steps, instead of only reporting it.
  const owner =
    "You own the result. Fix only what blocks the request: a step's VERDICT: FAIL, a failed check, or work that is wrong/unfinished. Then do not just report: see all changes (handoff FILES; git status, git diff), undo wrong ones, delete leftovers (debug output, stray files, notes), redo correctly, run the project's checks, end with what you fixed/removed. Suggestions and risks a step lists are not failures: do not edit for them, list them for the user in one short line each. If all is right, say so in one line.";
  const light =
    "The answers above are compact digests; each full answer is in that agent's own chat. Do not redo or re-check their work: run no commands, read no files, look at no git state. Write the reply to the user from the digests: combine them, say in a line where they disagree, what was done, and what the user should do next. Short.";
  return `${HANDBACK_HEAD}\n\n${answers}${memo}\n\n${finish}\n\n${trouble ? owner : light}\n\n${LANGUAGE_RULE}`;
}

export function handBack(batch: DelegateBatch) {
  const prompt = handbackPrompt(batch);
  const go = (tries: number) => {
    if (!replies[batch.main]) {
      batch.run.handback = 'running';
      batch.run.handbackAt = Date.now();
      // Silent: the agent gets the prompt but the chat shows no message for it. The delegate card (on the message that
      // handed work out) is the wait state, and the agent's reply follows it as if it were the same turn.
      send(batch.main, '', [], [], [], { prompt, silent: batch.asked });
    }
    // The chat is busy with something else: the answers are not kept waiting for it, they are worked on at once, beside it.
    else if (handBackBeside(batch, prompt)) return;
    else if (tries < 900) setTimeout(() => go(tries + 1), 1000);
    else logError('handback', 'the answers never reached the chat (it stayed busy for 15 min)', undefined, { chat: batch.main, run: batch.run.id });
  };
  go(0);
}

// ---------- a hand-back that does not wait ----------
// The agents asked have answered while the chat is busy with another turn. That turn cannot be joined (one CLI
// conversation runs one turn at a time), so the answers go to a turn of their own, running beside it: it knows the chat so
// far, has no session and no tab, and its reply sits in the chat as a message (open its steps to see what it did).
// The same runs a message the user sends while the chat is busy (`sendBeside`): it is answered at once, beside the turn
// that is still working, instead of waiting in the queue.
export const sideTurns = new Map<
  string,
  {
    main: string;
    reply: Message;
    turn: string;
    /** `handback`: the answers of other agents; `beside`: a message of the user. */
    kind: 'handback' | 'beside';
    onDone?: (reply: Message, failed: boolean) => void;
  }
>();
let sideCount = 0;

/** A side hand-back still occupies the main chat until its result arrives. */
export function hasSideTurn(main: string): boolean {
  for (const side of sideTurns.values()) if (side.main === main) return true;
  return false;
}

const sideTurnsOf = (main: string) =>
  [...sideTurns.values()].filter((side) => side.main === main).length;

function handBackBeside(batch: DelegateBatch, prompt: string): boolean {
  if (!runBeside(batch.main, forkDigest(messagesOf(batch.main)), prompt, 'handback'))
    return false;
  batch.run.handback = 'running';
  batch.run.handbackAt = Date.now();
  return true;
}

/** Starts a turn of its own beside the chat `main`: `context` is what it knows of the chat, `prompt` what to do now. */
function runBeside(
  main: string,
  context: string,
  prompt: string,
  kind: 'handback' | 'beside',
  onDone?: (reply: Message, failed: boolean) => void,
): boolean {
  const session = workspace.sessions.find((item) => item.id === main);
  // A custom provider keeps its conversation in the app: it has no CLI turn to run beside another one.
  if (!session || session.agent === 'custom' || !inTauri()) return false;
  const id = `${main}#side${++sideCount}`;
  const turn = nextId();
  const list = messagesOf(main);
  const model = modelFor(session);
  const reply: Message = {
    id: nextId(),
    role: 'assistant',
    time: clock(new Date().toISOString()),
    at: new Date().toISOString(),
    blocks: [],
    live: true,
    startedAt: Date.now(),
    agent: session.agent,
    model: model ?? session.model,
  };
  list.push(reply);
  const stored = list[list.length - 1]; // the stored (reactive) object
  sideTurns.set(id, { main, reply: stored, turn, kind, onDone });
  session.status = 'working';
  const cwd =
    workspace.projects.find((project) => project.id === session.projectId)
      ?.path ?? null;
  void sendAgentTurn({
    session: id,
    turn,
    agent: session.agent,
    prompt: context + prompt,
    keepOpen: false,
    // A conversation of its own: Claude takes the id we choose, the others name theirs on the first turn.
    agentSession: session.agent === 'claude' ? crypto.randomUUID() : null,
    resume: false,
    model,
    permissionMode: PERMISSION_MODE[turnOptions.permission],
    effort: (session.stepEffort ?? turnOptions.effort) as Effort,
    allowedTools: [...(approved[main] ?? [])],
    cwd,
  }).catch((error: unknown) =>
    handleSideEvent(id, { kind: 'error', message: String(error) }, turn),
  );
  return true;
}

export const clipText = (text: string, max: number) => {
  const flat = text.replace(/\s+/g, ' ').trim();
  return flat.length > max ? `${flat.slice(0, max - 1)}…` : flat;
};

/** One event of a turn running beside its chat: it fills that reply, and ends it. */
export function handleSideEvent(id: string, event: AgentEvent, turn: string) {
  const side = sideTurns.get(id);
  if (!side || side.turn !== turn || event.kind === 'raw') return;
  const { reply, main } = side;
  if (event.kind === 'usage') {
    addUsage(reply, event);
    return;
  }
  if (event.kind === 'done' && event.cost_usd != null)
    reply.cost = (reply.cost ?? 0) + event.cost_usd;
  const update = applyAgentEvent(reply.blocks, event);
  reply.blocks = update.blocks;
  if (event.kind === 'tool_result' && event.image)
    persistCommandImage(main, reply, event.id, event.image);
  if (!update.finished) return;
  reply.live = false;
  if (reply.startedAt) reply.ms = Date.now() - reply.startedAt;
  void finishCommandImages(main, reply);
  sideTurns.delete(id);
  const session = workspace.sessions.find((item) => item.id === main);
  if (session)
    session.status = replies[main] || hasSideTurn(main) ? 'working' : 'idle';
  if (session?.path) keepOtherTurn(session, reply);
  if (side.kind === 'handback') onHandbackDone(main, reply, event.kind === 'error');
  side.onDone?.(reply, event.kind === 'error');
  persistLocal();
  if (!replies[main] && !hasSideTurn(main)) sendNextQueued(main);
}

/**
 * Send from the message box. While busy: Enter queues at the end, Alt+Enter queues next, Ctrl/Cmd+Enter stops and sends now.
 */
/** The picture of each picked element, sent with the message as an image next to its markup. */
const elementPictures = (elements: PageElement[]): Attachment[] =>
  elements.flatMap((item) =>
    item.picture
      ? [
          {
            id: crypto.randomUUID(),
            name: `${item.tag}.png`,
            mime: 'image/png',
            src: item.picture,
          },
        ]
      : [],
  );

export function sendMessage(
  text: string,
  images: Attachment[] = [],
  elements: PageElement[] = [],
  mentions: Mention[] = [],
  whileRunning: 'queue' | 'next' | 'interrupt' | 'resume' = 'queue',
  files: FileAttachment[] = [],
) {
  const session = activeSession();
  images = [...images, ...elementPictures(elements)];
  if (
    !session ||
    (!text.trim() &&
      images.length === 0 &&
      elements.length === 0 &&
      files.length === 0)
  )
    return;
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
  if (
    whileRunning === 'queue' &&
    canListenToSession(session.id) &&
    !needsUserInput(session.id) &&
    !mentions.some((mention) => mention.kind === 'ai')
  ) {
    const preface = handoffDigest(messagesOf(session.id), 'The user sent a new message while your previous task continues in the background. Handle this request independently and keep your reply under the new message. Avoid modifying files likely owned by the still-running task; if the requests conflict, explain the conflict before changing shared files. Do not claim the background task has finished.');
    send(session.id, text, images, elements, mentions, { files, listener: true, preface });
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
  if (
    replies[session.id] ||
    isRunning(session.id) ||
    needsUserInput(session.id)
  ) {
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
    send(session.id, text, images, elements, mentions, { files, runGroup, messageId: nextId() });
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
  const text = last?.blocks.map((block) => (block.kind === 'response' ? block.text : '')).filter(Boolean).join('\n\n') ?? '';
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
  const index = list.findIndex(
    (message) => message.id === messageId && message.role === 'user',
  );
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
    hidden[session.id] = [
      ...new Set([
        ...(hidden[session.id] ?? []),
        ...removed.flatMap((message) => [message.id, signature(message)]),
      ]),
    ];
    setPref('chat.hidden', hidden);
    const others = otherTurns[session.id];
    if (others)
      otherTurns[session.id] = others.filter(
        (message) => !removed.includes(message),
      );
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
            mime: block.src.slice(5, block.src.indexOf(';')) || 'image/png',
            src: block.src,
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
function userMessage(
  body: string,
  images: Attachment[],
  elements: PageElement[],
  files: FileAttachment[],
  runGroup?: string,
  id = nextId(),
): Message {
  return {
    id,
    role: 'user',
    time: clock(new Date().toISOString()),
    blocks: [
      ...images.map((image) => ({
        kind: 'image' as const,
        src: image.src,
        name: image.name,
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
  const body = text.trim();
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
    delete resumeTries[sessionId];
  }
  const messages = messagesOf(sessionId);
  if (
    extra.silent &&
    extra.prompt &&
    inTauri() &&
    (session.agent === 'claude' ||
      session.agent === 'codex' ||
      session.agent === 'antigravity' ||
      session.agent === 'opencode' ||
      session.agent === 'custom')
  ) {
    startTurn(
      session,
      extra.prompt,
      [],
      [],
      (extra.preface ?? ''),
      [],
      extra.silent,
      extra.runGroup,
    );
    if (!session.path) localMessages[session.id] = messages;
    persistLocal();
    return;
  }
  // Auto-compress: a Claude context near the model's window is compacted first; the message waits at the head of the
  // queue and goes once the compaction is done.
  if (!extra.prompt && !extra.preface && needsCompress(session, messages)) {
    queued[sessionId] = [{ messageId, text, images, elements, mentions, files, runGroup: extra.runGroup }, ...(queued[sessionId] ?? [])];
    compressContext(session);
    return;
  }
  messages.push(userMessage(body, images, elements, files, extra.runGroup, messageId));
  // A new request from the user: retries and agents tried are counted again from here.
  delete recoveries[sessionId];
  const preface = (extra.preface ?? '') + mentionContext(mentions);
  const targets = mentions.filter((mention) => mention.kind === 'ai');
  const preset = presetOf(sessionId);
  if (!extra.listener && inTauri() && targets.length > 0 && body)
    delegate(
      session,
      messages[messages.length - 1],
      body,
      targets,
      preface,
      images,
      elements,
      files,
    );
  else if (
    !extra.listener && inTauri() &&
    preset &&
    preset.steps.length > 0 &&
    body &&
    !extra.prompt &&
    !extra.preface
  )
    void runWorkflow(
      session,
      messages[messages.length - 1],
      preset,
      body,
      preface,
      images,
      elements,
      files,
    );
  else if (
    inTauri() &&
    (session.agent === 'claude' ||
      session.agent === 'codex' ||
      session.agent === 'antigravity' ||
      session.agent === 'opencode' ||
      session.agent === 'copilot' ||
      session.agent === 'custom')
  )
    startTurn(
      session,
      withElements(extra.prompt ?? body, elements),
      images,
      [],
      preface,
      files,
      undefined,
      extra.runGroup,
      Boolean(extra.listener),
    );
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
    const firstLine = body.split('\n')[0]
      || files[0]?.name
      || images[0]?.name
      || elements[0]?.title
      || (elements.length ? 'Selected web content' : 'New chat');
    session.title =
      firstLine.length > 60 ? `${firstLine.slice(0, 57)}…` : firstLine;
  }
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
export function handoffDigest(
  list: Message[],
  lead = 'The account was switched, so this is a new session of the same conversation. Continue the work in progress; do not redo what is already done.',
): string {
  // Not the message about to be answered (the last one of the user), and not a reply that only holds an error line.
  const last = list.at(-1);
  const onlyError =
    last?.role === 'assistant' &&
    last.blocks.every((block) => block.kind === 'response' && (!block.text.trim() || block.text.trim().startsWith('⚠')));
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
        .flatMap((block) =>
          block.kind === 'response'
            ? [block.text]
            : block.kind === 'command'
              ? [`(ran: ${block.command.slice(0, 160)})`]
              : [],
        )
        .join('\n')
        .trim();
      return text
        ? `${message.role === 'user' ? 'User' : 'Assistant'}: ${text.length > limit ? `${text.slice(0, limit)}…` : text}`
        : '';
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
  const copy = (
    JSON.parse(JSON.stringify(messagesOf(sourceId))) as Message[]
  ).map((message) => ({ ...message, live: false }));
  localMessages[id] = copy;
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  const {
    path,
    source: _origin,
    updatedAt,
    messageCount,
    agentSession,
    agentStarted,
    agentIds,
    conversationModel,
    agentModels,
    ...rest
  } = source;
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