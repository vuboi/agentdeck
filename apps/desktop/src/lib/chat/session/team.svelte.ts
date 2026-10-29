import { type TeamState, teamText, type TeamMember, type TeamMessage, inboxOf, teamFinished, planFor, inboxText, parseTeamReply, MAX_TEAM_TURNS } from '../../tasks/team';
import { getPref, setPref } from '../../app/platform/prefs';
import { inTauri } from '../../app/platform/history';
import { logError } from '../../app/platform/error-log.svelte';
import { type Message } from '../mock';
import { type WorkflowPreset, type WorkflowStep, allPresets } from '../../tasks/workflows.svelte';
import { step } from '../runs/run-state';
import { QUESTION_RULE } from '../recovery/questions';
import { handoffOf } from '../../tasks/delegate-task';
import { MODELS, nextId, workspace, type Attachment, type FileAttachment, type PageElement, type SessionItem } from './state.svelte';
import { failSessionWork } from './turns.svelte';
import { messagesOf, replies, sendNextQueued } from './run.svelte';
import { logCtx } from './events.svelte';
import { RUN_LIMIT, delegateRuns, endCancelled, type DelegateBatch, type DelegateCard, type DelegateRun } from './delegation.svelte';
import { LANGUAGE_RULE, runFile, runStep, stepWaiters } from './workflow.svelte';
import { handBack, send } from './sending.svelte';
import { keepHandout, otherTurns } from './history.svelte';

// ---------- a team run ----------
// The first step of a `team` preset plans; the others work at the same time, each in its own chat, and tell each other
// what they need or what is ready through a queue (team.ts). The queue, the members' state and a log are saved as
// they change (IndexedDB `team.state`, TEAM.md in the run folder): after a reload the run carries on from there.

export const teamStates = $state<Record<string, TeamState>>({});
let teamsLoaded = false;
void getPref<Record<string, TeamState>>('team.state').then((saved) => {
  // Finished runs are kept a day, for the Workflow tab and TEAM.md; the unfinished ones until they end.
  for (const [id, state] of Object.entries(saved ?? {}))
    if (!state.endedAt || Date.now() - state.endedAt < 86_400_000) teamStates[id] = state;
  teamsLoaded = true;
  if (inTauri()) setTimeout(() => void resumeTeams(), 4000);
});
/** The messages a team run sent, for the canvas: who to whom, and whether it still waits in a queue. Reactive. */
export function teamViewOf(runId: string | undefined) {
  const state = runId ? teamStates[runId] : undefined;
  return { sent: (state?.queue ?? []).map((message) => ({ from: message.from, to: message.to, waiting: !message.taken })) };
}
/** Wakes the members of a run that wait for a message (by run id). */
const teamWakers = new Map<string, Set<() => void>>();

function saveTeam(state: TeamState, line?: string) {
  if (line) state.log.push(`${new Date().toISOString().slice(11, 19)} ${line}`);
  if (state.log.length > 400) state.log.splice(0, state.log.length - 400);
  teamStates[state.id] = state;
  if (teamsLoaded) setPref('team.state', $state.snapshot(teamStates));
  void runFile(state.root, state.id, 'TEAM.md', teamText(state));
  for (const wake of [...(teamWakers.get(state.id) ?? [])]) wake();
}

/** One turn of a member's chat that already exists: `text` goes as its message, its answer comes back. */
function teamTurn(childId: string, text: string): Promise<{ text: string; ok: boolean }> {
  return new Promise((resolve) => {
    stepWaiters.set(childId, (answer, error) =>
      resolve({ text: answer, ok: !error && answer.length > 0 }),
    );
    // A chat still in a turn takes no message: it is waited for, then the message goes.
    const go = (tries: number) => {
      if (!replies[childId]) send(childId, text, [], []);
      else if (tries < 600) setTimeout(() => go(tries + 1), 1000);
      else {
        logError('team', 'a member chat stayed busy for 10 min: its message was never sent', text.slice(0, 120), logCtx(childId));
        stepWaiters.delete(childId);
        resolve({ text: '', ok: false });
      }
    };
    go(0);
  });
}

/** Waits until `member` has messages (they are taken and returned) or the run is over (`null`). */
function teamInbox(
  state: TeamState,
  member: TeamMember,
  run: DelegateRun,
): Promise<TeamMessage[] | null> {
  return new Promise((resolve) => {
    const wakers = teamWakers.get(state.id) ?? new Set<() => void>();
    teamWakers.set(state.id, wakers);
    const check = () => {
      const inbox = inboxOf(state, member.name);
      if (!inbox.length && !run.cancelled && !teamFinished(state)) return;
      wakers.delete(check);
      clearInterval(timer);
      if (!inbox.length) return resolve(null);
      const at = Date.now();
      for (const message of inbox) message.taken = at;
      resolve(inbox);
    };
    // Also checked each second: a Stop of the main chat cancels the run without any message.
    const timer = setInterval(check, 1000);
    wakers.add(check);
    check();
  });
}

const TEAM_RULE = (self: string, others: string[]) =>
  [
    `You work in a team with ${others.map((name) => `@${name}`).join(', ')}, at the same time, each in your own chat. Their messages reach you as "FROM @<name>: ...".`,
    'To tell a member something (a part is ready, a shape changed, you need something), add a line "SEND @<name>: <message>" at the end of your answer: short and concrete (paths, endpoints, shapes). Send as soon as something is ready, not only at the end.',
    'End every answer with one line: "STATUS: done" (your part is finished and you need nothing more), "STATUS: waiting @<name>" (nothing more to do until that member answers) or "STATUS: working" (you go on in your next turn).',
    `Do not edit the files another member owns: say what you need instead. You are ${self}.`,
  ].join('\n');

type TeamCards = Map<string, { uuid: string; card: DelegateCard }>;
type TeamContext = { main: SessionItem; asked: Message; run: DelegateRun; cards: TeamCards };

export async function runTeam(
  main: SessionItem,
  asked: Message,
  preset: WorkflowPreset,
  task: string,
  preface: string,
  images: Attachment[],
  elements: PageElement[],
  files: FileAttachment[],
) {
  const runId = crypto.randomUUID();
  const [planner, ...crew] = preset.steps;
  const root =
    workspace.projects.find((project) => project.id === main.projectId)?.path ?? null;
  const { run, batch, cards } = teamRun(runId, main, asked, preset, task);
  const boardPath = await runFile(root, runId, 'BOARD.md', `# ${preset.name}\n\nTask: ${task}\n`);
  const folder = boardPath ? boardPath.replace(/[\\/]BOARD\.md$/, '') : '';
  run.folder = folder || undefined;
  const state: TeamState = {
    id: runId,
    main: main.id,
    askedId: asked.id,
    task,
    root,
    folder,
    plan: '',
    members: crew.map((step) => ({
      stepId: step.id,
      name: step.name,
      childId: `claude:${cards.get(step.id)?.uuid}`,
      status: 'new',
      turns: 0,
    })),
    queue: [],
    log: [],
    startedAt: Date.now(),
  };
  saveTeam(state, `start: ${preset.name}`);
  const names = crew.map((step) => step.name);
  const brief = [
    preface,
    `Planner of an AgentDeck team run. Members: ${names.join(', ')}.`,
    planner.prompt.trim() || 'Plan the work for the members.',
    // Any roles (QA, DevOps, a designer…): each member gets the section with its own name, with the shared one.
    `Sections of the plan, exactly these headings: "## Shared" (the contract every member works against), then ${names.map((name) => `"## ${name}"`).join(', ')}: each with that member's ordered tasks, what it can do before the others are ready, and what it must tell whom and when.`,
    `Run folder: ${folder || `.agentdeck/runs/${runId}`}. Write the plan as your answer: the app saves it as PLAN.md and gives each member its part.`,
    LANGUAGE_RULE,
    QUESTION_RULE,
  ]
    .filter(Boolean)
    .join('\n');
  const plan = await runStep(main, asked, run, planner, brief, task, '', images, elements, files, cards.get(planner.id));
  if (!plan.ok || run.cancelled) {
    if (!plan.ok) logError('team', 'the planner failed', plan.text.slice(0, 300) || 'no answer', { run: runId });
    saveTeam(state, plan.ok ? 'stopped' : 'the planner failed');
    return endTeam(state, run, batch, cards);
  }
  state.plan = plan.text;
  void runFile(root, runId, 'PLAN.md', plan.text);
  saveTeam(state, 'plan written');
  const ctx: TeamContext = { main, asked, run, cards };
  await Promise.all(
    state.members.map((member) =>
      teamMember(state, member, crew.find((step) => step.id === member.stepId) as WorkflowStep, names, preface, ctx),
    ),
  );
  endTeam(state, run, batch, cards);
}

/** The run's cards and hand-back batch, for a new run or one resumed after a reload (`uuids`: the members' chats). */
function teamRun(
  runId: string,
  main: SessionItem,
  asked: Message,
  preset: WorkflowPreset,
  task: string,
  uuids?: Map<string, string>,
) {
  delegateRuns.push({
    id: runId,
    main: main.id,
    task,
    preset: preset.name,
    plan: JSON.parse(JSON.stringify(preset.steps)),
    skipped: [],
    startedAt: Date.now(),
    nodes: [],
    handback: 'waiting',
  });
  if (delegateRuns.length > RUN_LIMIT) delegateRuns.splice(0, delegateRuns.length - RUN_LIMIT);
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
  const cards: TeamCards = new Map();
  for (const step of preset.steps) {
    const uuid = uuids?.get(step.id) ?? crypto.randomUUID();
    const found = asked.blocks.find(
      (block): block is DelegateCard => block.kind === 'delegate' && block.childId === `claude:${uuid}`,
    );
    if (!found)
      asked.blocks.push({
        kind: 'delegate',
        childId: `claude:${uuid}`,
        agent: step.agent,
        model: step.model || MODELS[step.agent]?.[0]?.value || '',
        state: 'queued',
        step: step.name,
      });
    cards.set(step.id, { uuid, card: (found ?? asked.blocks[asked.blocks.length - 1]) as DelegateCard });
  }
  keepHandout(main.id, asked);
  return { run, batch, cards };
}

/** One member: its first turn (or the one a reload cut off), then one turn for each batch of messages it gets. */
async function teamMember(
  state: TeamState,
  member: TeamMember,
  step: WorkflowStep,
  names: string[],
  preface: string,
  ctx: TeamContext,
) {
  const { main, asked, run, cards } = ctx;
  const others = names.filter((name) => name !== member.name);
  const entry = cards.get(step.id);
  const setCard = (patch: Partial<DelegateCard>) => {
    if (!entry) return;
    Object.assign(entry.card, patch);
    keepHandout(main.id, asked);
  };
  let reply: { text: string; ok: boolean } | null = null;
  if (member.status === 'new') {
    member.busy = true;
    member.status = 'working';
    saveTeam(state, `${member.name}: started`);
    const brief = [
      preface,
      `Member "${member.name}" of an AgentDeck team run. Your job:`,
      step.prompt.trim() || 'Do your part of the plan.',
      `Your part of the plan, with the shared part (the whole plan: ${state.folder ? `${state.folder}/PLAN.md` : 'ask the planner'}):\n${planFor(state.plan, member.name)}`,
      TEAM_RULE(member.name, others),
      LANGUAGE_RULE,
    ]
      .filter(Boolean)
      .join('\n');
    const first = await runStep(main, asked, run, step, brief, state.task, '', [], [], [], entry);
    reply = { text: first.text, ok: first.ok };
  } else if (member.busy) {
    // A reload cut this member's turn off: it goes on, with what waits for it.
    const waiting = inboxOf(state, member.name);
    for (const message of waiting) message.taken = Date.now();
    setCard({ state: 'running', note: 'Resumed after a restart' });
    saveTeam(state, `${member.name}: resumed`);
    reply = await teamTurn(
      member.childId,
      ['AgentDeck was restarted while you worked: check what you had already done, then go on with your part.', inboxText(waiting)]
        .filter(Boolean)
        .join('\n'),
    );
  }
  for (;;) {
    if (reply) {
      member.busy = false;
      member.turns++;
      member.last = reply.text;
      if (!reply.ok) {
        member.status = 'failed';
        logError('team', `${member.name}: turn failed`, reply.text.slice(0, 300) || 'no answer', { run: state.id, member: member.name, turns: member.turns });
        setCard({ state: 'failed', note: 'Turn failed' });
        saveTeam(state, `${member.name}: turn failed`);
        break;
      }
      const parsed = parseTeamReply(reply.text, member.name, names);
      for (const message of parsed.sends)
        state.queue.push({ id: nextId(), from: member.name, to: message.to, text: message.text, at: Date.now() });
      const lost = (reply.text.match(/^\s*[-*>]?\s*\**SEND\s+@/gim)?.length ?? 0) - parsed.sends.length;
      if (lost > 0) logError('team', `${member.name}: ${lost} message(s) to an unknown member or to itself`, undefined, { run: state.id, member: member.name, known: names.join(',') });
      if (!/^\s*[-*>]?\s*\**STATUS:/im.test(reply.text)) logError('team', `${member.name}: answer without a STATUS line`, 'treated as working', { run: state.id, member: member.name });
      const spent = member.turns >= MAX_TEAM_TURNS;
      if (spent) logError('team', `${member.name}: turn limit reached`, `${MAX_TEAM_TURNS} turns`, { run: state.id, member: member.name });
      member.status = spent ? 'failed' : parsed.status;
      member.waitingOn = parsed.waitingOn;
      setCard({
        state: member.status === 'failed' ? 'failed' : member.status === 'done' ? 'done' : 'running',
        note: spent ? 'Turn limit reached' : member.status === 'waiting' ? `Waiting for ${parsed.waitingOn ?? 'a message'}` : undefined,
      });
      saveTeam(
        state,
        `${member.name}: ${member.status}${parsed.sends.length ? `, sent to ${parsed.sends.map((item) => `@${item.to}`).join(' ')}` : ''}`,
      );
      if (member.status === 'failed' || run.cancelled) break;
    }
    // `working`: it goes on at once, with whatever came in meanwhile; otherwise it waits for a message.
    let inbox: TeamMessage[] | null;
    if (member.status === 'working') {
      inbox = inboxOf(state, member.name);
      for (const message of inbox) message.taken = Date.now();
    } else inbox = await teamInbox(state, member, run);
    if (inbox === null || run.cancelled) break;
    member.busy = true;
    member.status = 'working';
    setCard({
      state: 'running',
      note: inbox.length ? `Message from ${[...new Set(inbox.map((item) => item.from))].join(', ')}` : undefined,
    });
    saveTeam(state, `${member.name}: turn ${member.turns + 1}${inbox.length ? ` with ${inbox.length} message(s)` : ''}`);
    reply = await teamTurn(member.childId, inbox.length ? inboxText(inbox) : 'Go on with your part.');
  }
  member.busy = false;
  // A member that stops early wakes the others, so none waits on it for ever.
  saveTeam(state);
}

/** Every member has stopped: the main chat gets what each one did, and the run is closed. */
function endTeam(state: TeamState, run: DelegateRun, batch: DelegateBatch, cards: TeamCards) {
  state.endedAt = Date.now();
  const stuck = state.members.filter((member) => member.status === 'waiting' && !member.waitingOn);
  if (stuck.length || state.members.some((member) => member.status === 'failed'))
    logError('team', 'run over with problems', state.members.map((member) => `${member.name}=${member.status}${member.waitingOn ? `@${member.waitingOn}` : ''}`).join(' '), { run: state.id, queued: state.queue.filter((message) => !message.taken).length });
  saveTeam(state, 'run over');
  for (const { card } of cards.values())
    if (card.state === 'queued') Object.assign(card, { state: 'skipped', note: 'Not run' });
  keepHandout(batch.main, batch.asked);
  if (run.cancelled) return endCancelled(batch.main, run);
  batch.results = state.members
    .filter((member) => member.last)
    .map((member) => ({ label: `${member.name} (${member.status})`, text: handoffOf(member.last ?? '') }));
  if (!batch.results.length) {
    const main = workspace.sessions.find((item) => item.id === batch.main);
    if (main) main.status = 'idle';
    run.handback = 'failed';
    run.endedAt = Date.now();
    failSessionWork(batch.main);
    sendNextQueued(batch.main);
    return;
  }
  const sent = state.queue.map((message) => `- ${message.from} -> ${message.to}: ${message.text}`);
  batch.memo = [
    sent.length ? `What the members told each other:\n${sent.slice(-40).join('\n')}` : '',
    state.folder ? `The run's files are in ${state.folder} (PLAN.md, TEAM.md with the queue and the log).` : '',
  ]
    .filter(Boolean)
    .join('\n\n');
  handBack(batch);
}

/** Runs cut off by a reload go on: each member takes up its queue, or the turn it was in, where it stopped. */
async function resumeTeams() {
  for (const state of Object.values(teamStates)) {
    if (state.endedAt) continue;
    const main = workspace.sessions.find((item) => item.id === state.main);
    const preset = allPresets().find(
      (item) =>
        item.mode === 'team' &&
        item.steps.length === state.members.length + 1 &&
        item.steps.slice(1).every((step, index) => step.name === state.members[index].name),
    );
    const asked = main
      ? (messagesOf(main.id).find((message) => message.id === state.askedId) ??
        otherTurns[main.id]?.find((message) => message.id === state.askedId))
      : undefined;
    if (!main || !preset || !state.plan || !asked) {
      state.endedAt = Date.now();
      saveTeam(state, 'could not be resumed after a restart');
      continue;
    }
    const uuids = new Map(state.members.map((member) => [member.stepId, member.childId.replace(/^claude:/, '')]));
    const { run, batch, cards } = teamRun(state.id, main, asked, preset, state.task, uuids);
    run.folder = state.folder || undefined;
    saveTeam(state, 'resumed after a restart');
    const names = state.members.map((member) => member.name);
    const ctx: TeamContext = { main, asked, run, cards };
    void Promise.all(
      state.members.map((member) => {
        if (member.status === 'done' || member.status === 'failed' || member.status === 'new') return Promise.resolve();
        // Not in a turn when the app closed: it waits for its messages like before.
        if (!member.busy) member.status = 'waiting';
        const step = preset.steps.find((item) => item.id === member.stepId) as WorkflowStep;
        return teamMember(state, member, step, names, '', ctx);
      }),
    ).then(() => endTeam(state, run, batch, cards));
  }
}