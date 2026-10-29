import { type Message } from '../mock';
import { step } from '../runs/run-state';
import { type WorkflowStep, type WorkflowPreset, normalizeGraph, inputsOf, stepGroups } from '../../tasks/workflows.svelte';
import { parseQuestions, QUESTION_RULE } from '../recovery/questions';
import { logError } from '../../app/platform/error-log.svelte';
import { logTalk } from '../runs/activity.svelte';
import { parseHandoff, handoffOf } from '../../tasks/delegate-task';
import { MODELS, workspace, type Attachment, type FileAttachment, type PageElement, type SessionItem } from './state.svelte';
import { agentLabel, failSessionWork, queued } from './turns.svelte';
import { replies, sendNextQueued } from './run.svelte';
import { logCtx } from './events.svelte';
import { RUN_LIMIT, delegateRuns, endCancelled, firstLineOf, statsOf, type DelegateBatch, type DelegateCard, type DelegateRun } from './delegation.svelte';
import { runTeam } from './team.svelte';
import { REVIEW_RULE, REVIEW_STEP, handBack, send } from './sending.svelte';
import { keepHandout } from './history.svelte';

// ---------- workflow presets: the chat's agent as orchestrator ----------
// A chat with a flow does not answer a message itself. Its steps run as child chats (sub sessions under it), each
// stage after the one that feeds it; steps of one stage run side by side. To keep tokens down every step gets only
// its own job, the task and the short handoffs of the steps that feed it, never their whole transcripts. The run has
// a folder of shared notes (`.agentdeck/runs/<run>/`): BOARD.md, written by the app only (goal, plan, handoffs),
// `agents/<name>.md`, one per step and written by that step only (findings, questions to the others and answers),
// and `handoff/<n>-<name>.md`, the summary a step leaves for the next. One writer per file, so no locking is needed.
// When the last stage is done the chat's own agent is called once with the handoffs and the findings.

/** A step's question to an earlier step: its last line `ASK @<step>: <question>`. */
const MAX_ASKS = 2;
const askOf = (text: string) => {
  const found = /^\s*ASK @([^:\n]{1,60}):\s*(.+)$/m.exec(text);
  return found ? { to: found[1].trim(), question: found[2].trim() } : null;
};
export const shorten = (text: string, max: number) =>
  text.length > max ? `${text.slice(0, max)}…` : text;

/** The line a step ends with to send its input back to the step(s) before it. */
const REPLAN_MARK = 'REPLAN:';
const MAX_REPLANS = 1;
const replanOf = (text: string) => {
  const found = /^\s*REPLAN:\s*(.+)$/m.exec(text);
  if (found) return found[1].trim();
  // The handoff can say it too: STATUS: needs-plan-change, with what is missing in OPEN.
  if (/^\s*STATUS:\s*needs-plan-change/im.test(text))
    return (
      /^\s*OPEN:\s*(.+)$/im.exec(text)?.[1].trim() || 'the plan needs changes'
    );
  return null;
};

/** The section a step ends with: short labelled lines the next agent reads at a glance, never prose. */
const HANDOFF_RULE =
  'End with "## Handoff" (<=120 words, only this is passed on; "-" if empty): STATUS: done|blocked|needs-plan-change / RESULT: <1 line> / FILES: <paths> / FACTS: <<=5, "; "-separated, path:line> / OPEN: <<=3> / VERIFY: <cmd> -> pass|fail|not run / NEXT: <first thing the next step does>. Telegraphic: no filler, no restating the task.';

/** Told to every agent of a run: the user's language, not the language of these instructions. */
export const LANGUAGE_RULE =
  "Write all user-facing replies and progress updates in the language the user has been using in this conversation, even when attached files, tool output, or these notes are in another language. Keep code, paths, commands, and quoted source text unchanged.";

export const stepWaiters = new Map<
  string,
  (text: string, error: string | null, reply: Message) => void
>();
/** Flow steps waiting for the user's answers, by their chat. */
export const askingCards = new Map<string, DelegateCard>();

/** Sends the answers to questions an agent asked (in its own chat, or a flow step's from the Flow steps card). */
export function answerQuestions(sessionId: string, text: string): boolean {
  // A turn is running (a task's report started one, say): the answers cannot go out now, and the card must not say they did.
  if (!text.trim() || replies[sessionId]) return false;
  const card = askingCards.get(sessionId);
  if (card) {
    card.state = 'running';
    askingCards.delete(sessionId);
  }
  send(sessionId, text, [], []);
  return true;
}

const passes = (step: WorkflowStep, previous: string) => {
  const { mode, text } = step.condition;
  if (mode === 'always' || !text.trim()) return true;
  const found = previous.toLowerCase().includes(text.trim().toLowerCase());
  return mode === 'contains' ? found : !found;
};

/** Reads (no `content`) or writes one file of a run's notes; '' when it cannot. */
export async function runFile(
  root: string | null,
  run: string,
  name: string,
  content?: string,
): Promise<string> {
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    const args = {
      root,
      run,
      name,
      ...(content === undefined ? {} : { content }),
    };
    const out = await invoke(
      content === undefined ? 'run_file_read' : 'run_file_write',
      args,
    );
    return typeof out === 'string' ? out : '';
  } catch {
    return '';
  }
}

const slugOf = (text: string) =>
  text
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-|-$/g, '')
    .slice(0, 32) || 'step';

/** The findings (`[F]` lines) the steps wrote in their notes, without repeats, at most `limit` lines. */
function findingsOf(notes: string[], limit = 30): string[] {
  const seen = new Set<string>();
  for (const text of notes) {
    for (const line of text.split('\n')) {
      const item = line.trim().replace(/^[-*]\s*/, '');
      if (item.startsWith('[F]') && !seen.has(item)) seen.add(item);
      if (seen.size >= limit) return [...seen];
    }
  }
  return [...seen];
}

type FlowRunContext = {
  root: string | null;
  folder: string;
  board: string[];
  slugs: Map<string, string>;
  handoffs: Map<string, string>;
};

function boardText(
  task: string,
  preset: WorkflowPreset,
  ctx: FlowRunContext,
  groups: WorkflowStep[][],
): string {
  const plan = groups
    .map(
      (group, stage) =>
        `${stage + 1}. ${group.map((step) => `${step.name} (${agentLabel(step.agent, step.model)}) — notes: agents/${ctx.slugs.get(step.id)}.md`).join(' | ')}`,
    )
    .join('\n');
  return [
    `# Run board — ${preset.name}`,
    '',
    `Goal: ${firstLineOf(task, 300)}`,
    '',
    '## Plan (one line per stage; steps of a stage run at the same time)',
    plan,
    '',
    '## Handoffs',
    ...(ctx.board.length ? ctx.board : ['(none yet)']),
    '',
  ].join('\n');
}

export function runStep(
  main: SessionItem,
  asked: Message,
  run: DelegateRun,
  step: WorkflowStep,
  brief: string,
  task: string,
  previous: string,
  images: Attachment[],
  elements: PageElement[],
  files: FileAttachment[],
  queued?: { uuid: string; card: DelegateCard },
) {
  const agent = step.agent;
  const model = step.model || MODELS[agent]?.[0]?.value || '';
  // Replan attempts stay in the same provider conversation. This preserves context and avoids paying a new
  // system/tool-definition prefix for a correction that belongs to the same step.
  const prior = [...run.nodes]
    .reverse()
    .find((item) => item.stepId === step.id && item.agentSession);
  const uuid = prior?.agentSession ?? queued?.uuid ?? crypto.randomUUID();
  const id = `claude:${uuid}`;
  if (!workspace.sessions.some((item) => item.id === id)) {
    workspace.sessions.push({
      id,
      projectId: main.projectId,
      title: `${step.name}: ${firstLineOf(task, 50)}`,
      agent,
      agentSession: uuid,
      model,
      gitBranch: main.gitBranch,
      status: 'idle',
      parentId: main.id,
      stepEffort: step.effort || undefined,
      draft: false,
    });
  }
  // Its card was made when the run began, as "queued": now it is working.
  let card: DelegateCard;
  if (queued) {
    card = queued.card;
    card.state = 'running';
    // A step run again (its plan was sent back) resumes its existing chat: the card follows that conversation.
    card.childId = id;
    card.note = undefined;
  } else {
    asked.blocks.push({
      kind: 'delegate',
      childId: id,
      agent,
      model,
      state: 'running',
      step: step.name,
    });
    card = asked.blocks[asked.blocks.length - 1] as DelegateCard;
  }
  const label = `${step.name} · ${agentLabel(agent, model)}`;
  run.nodes.push({
    childId: id,
    stepId: step.id,
    agent,
    model,
    label,
    startedAt: Date.now(),
    state: 'running',
    input: previous,
    attempt: (prior?.attempt ?? 0) + 1,
    agentSession: uuid,
  });
  const node = run.nodes[run.nodes.length - 1];
  const asks = new Map<string, number>();
  return new Promise<{
    step: WorkflowStep;
    label: string;
    text: string;
    ok: boolean;
  }>((resolve) => {
    const settle = (text: string, error: string | null, reply: Message) => {
      // The step asked the user first: the run waits here until the answers come back as the step's next turn.
      const questions = error ? [] : parseQuestions(text).questions;
      if (questions.length) {
        card.state = 'asking';
        card.questions = questions;
        askingCards.set(id, card);
        stepWaiters.set(id, settle);
        keepHandout(main.id, asked);
        return;
      }
      // The step needs a fact only an earlier step knows: that step's own chat (it has the context) is asked, its short
      // answer goes back to this step as its next turn. Capped, so two steps cannot talk for ever.
      const ask = error ? null : askOf(text);
      const target =
        ask && (asks.get(id) ?? 0) < MAX_ASKS
          ? run.nodes.findLast(
              (item) =>
                item.state === 'done' &&
                item.childId !== id &&
                item.label.split(' · ')[0].toLowerCase() ===
                  ask.to.toLowerCase(),
            )
          : undefined;
      if (ask && !target)
        logError('flow', `step "${step.name}" asked @${ask.to}`, (asks.get(id) ?? 0) >= MAX_ASKS ? 'ask limit reached' : 'no finished step with that name', { run: run.id, ...logCtx(id) });
      if (ask && target) {
        asks.set(id, (asks.get(id) ?? 0) + 1);
        (node.talks ??= []).push({
          to: ask.to,
          question: ask.question,
          at: Date.now(),
        });
        const talk = node.talks[node.talks.length - 1];
        card.talking = ask.to;
        card.talkText = ask.question;
        keepHandout(main.id, asked);
        // Registered at once: a Stop while it waits ends this step (it has no reply of its own to stop).
        stepWaiters.set(id, settle);
        stepWaiters.set(target.childId, (answer, failed) => {
          talk.answer = failed ? '(no answer)' : shorten(answer, 700);
          card.talking = undefined;
          card.talkText = undefined;
          logTalk(
            main.id,
            main.title,
            workspace.projects.find((project) => project.id === main.projectId)
              ?.name ?? '',
            step.name,
            ask.to,
            ask.question,
            talk.answer,
          );
          keepHandout(main.id, asked);
          send(id, `A from ${ask.to}: ${talk.answer}`, [], []);
        });
        send(
          target.childId,
          `Q from ${step.name}: ${ask.question}\nAnswer in at most 60 words, only the answer.`,
          [],
          [],
        );
        return;
      }
      card.questions = undefined;
      const ok = !error && text.length > 0;
      card.state = ok ? 'done' : 'failed';
      card.note = ok ? undefined : (error ?? 'no answer');
      if (!ok) logError('flow', `step "${step.name}" failed`, error ?? 'no answer', { run: run.id, ...logCtx(id) });
      node.state = ok ? 'done' : 'failed';
      node.note = card.note;
      node.endedAt = Date.now();
      node.stats = statsOf(reply, node.startedAt);
      node.output = text;
      node.handoff = parseHandoff(text);
      keepHandout(main.id, asked);
      resolve({
        step,
        label,
        text: ok ? text : `(it did not answer: ${error ?? 'no text'})`,
        ok,
      });
    };
    stepWaiters.set(id, settle);
    send(id, task, images, elements, [], { preface: brief, files });
  });
}

export async function runWorkflow(
  main: SessionItem,
  asked: Message,
  preset: WorkflowPreset,
  task: string,
  preface: string,
  images: Attachment[],
  elements: PageElement[],
  files: FileAttachment[] = [],
) {
  if (preset.mode === 'team' && preset.steps.length >= 2)
    return runTeam(main, asked, preset, task, preface, images, elements, files);
  const runId = crypto.randomUUID();
  const graph = normalizeGraph(preset.steps);
  const links = inputsOf(graph);
  const groups = stepGroups(graph);
  delegateRuns.push({
    id: runId,
    main: main.id,
    task,
    preset: preset.name,
    plan: JSON.parse(JSON.stringify(graph)),
    skipped: [],
    startedAt: Date.now(),
    nodes: [],
    handback: 'waiting',
  });
  if (delegateRuns.length > RUN_LIMIT)
    delegateRuns.splice(0, delegateRuns.length - RUN_LIMIT);
  const run = delegateRuns[delegateRuns.length - 1];
  const batch: DelegateBatch = {
    main: main.id,
    task,
    asked,
    pending: new Set(),
    results: [],
    run,
    orchestrator: preset.orchestrator.trim() || undefined,
  };
  main.status = 'working';

  const root =
    workspace.projects.find((project) => project.id === main.projectId)?.path ??
    null;
  const slugs = new Map<string, string>();
  graph.forEach((step, index) =>
    slugs.set(step.id, `${index + 1}-${slugOf(step.name)}`),
  );
  const ctx: FlowRunContext = {
    root,
    folder: '',
    board: [],
    slugs,
    handoffs: new Map(),
  };
  const boardPath = await runFile(
    root,
    runId,
    'BOARD.md',
    boardText(task, preset, ctx, groups),
  );
  ctx.folder = boardPath ? boardPath.replace(/[\\/]BOARD\.md$/, '') : '';
  run.folder = ctx.folder || undefined;

  // Every step has its card in the message from the start: the ones that wait say "Queued", the ones of a stage that
  // runs together all say "Working" once it starts.
  const cards = new Map<string, { uuid: string; card: DelegateCard }>();
  for (const step of graph) {
    const uuid = crypto.randomUUID();
    asked.blocks.push({
      kind: 'delegate',
      childId: `claude:${uuid}`,
      agent: step.agent,
      model: step.model || MODELS[step.agent]?.[0]?.value || '',
      state: 'queued',
      step: step.name,
    });
    cards.set(step.id, {
      uuid,
      card: asked.blocks[asked.blocks.length - 1] as DelegateCard,
    });
  }
  keepHandout(main.id, asked);

  let first = true;
  // A step can send its input back (REPLAN): the steps that fed it run again with the reason, at most MAX_REPLANS times.
  const replanNotes = new Map<string, string>();
  let replans = 0;
  // Keep parallel fan-out useful without turning a normal flow into a provider swarm. A preset may lower or raise
  // this deliberately, but the app keeps the value bounded so an imported preference cannot exhaust the machine.
  const maxParallel = Math.max(
    1,
    Math.min(6, Math.round(preset.coordination?.maxParallel ?? 3)),
  );
  const maxReplans = Math.max(
    0,
    Math.min(3, Math.round(preset.coordination?.maxReplans ?? MAX_REPLANS)),
  );
  for (let stage = 0; stage < groups.length; stage++) {
    const group = groups[stage];
    const inputText = (step: WorkflowStep) =>
      (links[step.id] ?? [])
        .filter((from) => from !== 'start' && ctx.handoffs.has(from))
        .map(
          (from) =>
            `### ${graph.find((item) => item.id === from)?.name ?? from}\n${ctx.handoffs.get(from)}`,
        )
        .join('\n\n');
    const active = group.filter((step) => passes(step, inputText(step)));
    for (const step of group) {
      if (active.includes(step)) continue;
      run.skipped?.push(step.id);
      const skipped = cards.get(step.id)?.card;
      if (skipped) {
        skipped.state = 'skipped';
        skipped.note = 'Condition not met';
      }
    }
    if (active.length === 0) continue;
    const board = boardText(task, preset, ctx, groups);
    const done: {
      step: WorkflowStep;
      label: string;
      text: string;
      ok: boolean;
    }[] = [];
    for (let offset = 0; offset < active.length; offset += maxParallel) {
      const batch = await Promise.all(
        active.slice(offset, offset + maxParallel).map((step) => {
          const slug = slugs.get(step.id) ?? slugOf(step.name);
          const previous = inputText(step);
          const others = active
            .filter((item) => item !== step)
            .map(
              (item) =>
                `"${item.name}" (notes: agents/${slugs.get(item.id)}.md)`,
            );
          const where = ctx.folder || '.agentdeck/runs/' + runId;
          const fed = (links[step.id] ?? []).some((from) => from !== 'start');
          // Written for the agent, not the user: short labelled lines. What the user reads is the chat itself.
          const brief = [
            preface,
            `Step "${step.name}" of an AgentDeck run (orchestrator: ${agentLabel(main.agent)}). Do only this job:`,
            step.prompt.trim() || 'Do your part of the task.',
            `Run folder: ${where}. BOARD.md = goal, plan, handoffs (app-written, read-only${ctx.folder ? '; read it first' : '; below'}). Notes: agents/${slug}.md, yours only, append one line each: [F] fact (path:line) | [Q @name] question | [A @name] answer. Never edit BOARD.md or others' notes.`,
            ctx.folder ? '' : board,
            previous
              ? `IN (from the step before; read its notes only if needed):\n${previous}`
              : '',
            others.length
              ? `Parallel with you: ${others.join(', ')}. Read their notes at start and after each big step, answer [Q @${step.name}] with [A], do not edit files they claim.`
              : '',
            LANGUAGE_RULE,
            QUESTION_RULE,
            fed
              ? `Need one fact only an earlier step knows: end with "ASK @<step name>: <question>" and stop (max ${MAX_ASKS}). Input too unclear or wrong to work from: do not guess, end with "${REPLAN_MARK} <what to fix>" and stop.`
              : '',
            replanNotes.get(step.id)
              ? `Sent back by the next step: "${replanNotes.get(step.id)}". Fix that.`
              : '',
            REVIEW_STEP.test(`${step.name} ${step.prompt}`) ? REVIEW_RULE : '',
            HANDOFF_RULE,
          ]
            .filter((line, index, all) => line !== '' || all[index - 1] !== '')
            .join('\n');
          return runStep(
            main,
            asked,
            run,
            step,
            brief,
            task,
            previous,
            first ? images : [],
            first ? elements : [],
            first ? files : [],
            cards.get(step.id),
          );
        }),
      );
      done.push(...batch);
    }
    first = false;
    for (const item of done) {
      const handoff = item.ok ? handoffOf(item.text) : item.text;
      ctx.handoffs.set(item.step.id, handoff);
      const name = `handoff/${slugs.get(item.step.id)}.md`;
      void runFile(root, runId, name, `# ${item.step.name}\n\n${handoff}\n`);
      ctx.board.push(
        `- ${item.step.name}: ${name}${item.ok ? '' : ' (failed)'}`,
      );
      batch.results.push({ label: item.label, text: handoff });
    }
    void runFile(root, runId, 'BOARD.md', boardText(task, preset, ctx, groups));
    // A step that failed ends the chain: the agent is told what happened so far.
    if (done.some((item) => !item.ok)) {
      logError('flow', 'a stage failed: the run stops', done.filter((item) => !item.ok).map((item) => item.label).join(', '), { run: run.id, stage });
      failSessionWork(main.id);
    }
    if (run.cancelled || done.some((item) => !item.ok)) break;
    for (const step of active) replanNotes.delete(step.id);
    // A step sent its input back: the steps that fed it run again (and every stage after them), with its reason.
    const back = done.find((item) => item.ok && replanOf(item.text) !== null);
    if (back && replans < maxReplans) {
      const feeders = (links[back.step.id] ?? []).filter(
        (from) => from !== 'start',
      );
      const from = Math.min(
        ...feeders
          .map((id) =>
            groups.findIndex((list) => list.some((item) => item.id === id)),
          )
          .filter((index) => index >= 0),
      );
      if (Number.isFinite(from)) {
        replans++;
        const reason = replanOf(back.text) ?? '';
        for (const id of feeders) replanNotes.set(id, reason);
        const again = groups.slice(from, stage + 1).flat();
        // Their cards wait again; runStep resumes each step's existing conversation when it starts.
        for (const item of again) {
          const entry = cards.get(item.id);
          if (!entry) continue;
          entry.card.state = 'queued';
          entry.card.note = feeders.includes(item.id) ? 'Sent back' : undefined;
          cards.set(item.id, { uuid: entry.uuid, card: entry.card });
          ctx.handoffs.delete(item.id);
        }
        batch.results = batch.results.filter(
          (result) =>
            !again.some((item) => result.label.startsWith(`${item.name} ·`)),
        );
        ctx.board.push(`- ${back.step.name} sent its input back: ${reason}`);
        keepHandout(main.id, asked);
        stage = from - 1;
      }
    }
  }
  // A step that failed ends the chain: those that never started say so, instead of waiting for ever.
  for (const { card } of cards.values()) {
    if (card.state === 'queued') {
      card.state = 'skipped';
      card.note = 'Not run';
    }
  }
  keepHandout(main.id, asked);
  if (run.cancelled) {
    endCancelled(main.id, run);
    return;
  }
  if (batch.results.length === 0) {
    main.status = 'idle';
    run.handback = 'failed';
    run.endedAt = Date.now();
    failSessionWork(main.id);
    sendNextQueued(main.id);
    return;
  }
  const notes = await Promise.all(
    graph.map((step) =>
      runFile(root, runId, `agents/${slugs.get(step.id)}.md`),
    ),
  );
  const facts = findingsOf(notes);
  batch.memo = [
    facts.length
      ? `Findings the steps shared:\n${facts.map((fact) => `- ${fact}`).join('\n')}`
      : '',
    ctx.folder
      ? `The run's notes are in ${ctx.folder} (BOARD.md, agents/, handoff/): open them only if you need more detail.`
      : '',
  ]
    .filter(Boolean)
    .join('\n\n');
  handBack(batch);
}