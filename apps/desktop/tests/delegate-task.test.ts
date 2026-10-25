// @ts-nocheck: run by node (type stripping), not by the Svelte type checker.
// Run with: node --experimental-strip-types --test tests/delegate-task.test.ts
import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  DELEGATE_HINT,
  DEFAULT_TIMEOUT_S,
  DIGEST_MAX,
  MAX_TIMEOUT_S,
  MIN_TIMEOUT_S,
  parseHandoff,
  agentdeckLead,
  argsOfCall,
  cardOf,
  findAgent,
  formatAgents,
  formatTaskResult,
  kindOfCommand,
  parseDelegateArgs,
  parseTaskResult,
  pickTask,
  reasonLine,
  reportDigest,
  resolveModel,
  startedText,
  toolKindOf,
} from '../src/lib/delegate-task.ts';
import { applyAgentEvent } from '../src/lib/agent-reply.ts';
import { HANDBACK_HEAD, isHandbackPrompt, stripLead } from '../src/lib/handback.ts';

// ---------- tool names ----------

test('every CLI names the delegation tools its own way', () => {
  for (const name of ['mcp__agentdeck__delegate_task', 'agentdeck.delegate_task', 'delegate_task']) {
    assert.equal(toolKindOf(name), 'delegate_task', name);
  }
  assert.equal(toolKindOf('mcp__agentdeck__list_agents'), 'list_agents');
  assert.equal(toolKindOf('agentdeck.task_status'), 'task_status');
  assert.equal(toolKindOf('mcp__agentdeck__task_stop'), 'task_stop');
  assert.equal(toolKindOf('mcp__agentdeck__browser_click'), null);
  assert.equal(toolKindOf('Task'), null);
  assert.equal(toolKindOf('delegate_task_later'), null);
  assert.equal(kindOfCommand('mcp__agentdeck__delegate_task  {"agent":"codex"}'), 'delegate_task');
  assert.equal(kindOfCommand('Bash  git status'), null);
});

// ---------- arguments ----------

test('delegate_task defaults to waiting, for thirty minutes', () => {
  const parsed = parseDelegateArgs({ agent: ' claude ', task: ' read the logs ' });
  assert.deepEqual(parsed, { ok: true, args: { agent: 'claude', model: undefined, task: 'read the logs', wait: true, timeoutS: DEFAULT_TIMEOUT_S } });
  assert.equal(DEFAULT_TIMEOUT_S, 1800);
});

test('agent and task are required', () => {
  for (const bad of [null, {}, { task: 'x' }, { agent: 'codex' }, { agent: 'codex', task: '  ' }, { agent: 3, task: 'x' }]) {
    assert.equal(parseDelegateArgs(bad).ok, false, JSON.stringify(bad));
  }
  assert.equal(parseDelegateArgs({ agent: 'a', task: 'x'.repeat(30_001) }).ok, false);
});

test('wait and timeout_s take what local models send', () => {
  const args = (extra) => parseDelegateArgs({ agent: 'codex', task: 'x', ...extra });
  assert.equal(args({ wait: 'false' }).args.wait, false);
  assert.equal(args({ wait: false }).args.wait, false);
  assert.equal(args({ wait: 0 }).args.wait, false);
  assert.equal(args({ wait: null }).args.wait, true);
  assert.equal(args({ wait: 'maybe' }).ok, false);
  assert.equal(args({ timeout_s: '90' }).args.timeoutS, 90);
  assert.equal(args({ timeout_s: 1 }).args.timeoutS, MIN_TIMEOUT_S);
  assert.equal(args({ timeout_s: 99999 }).args.timeoutS, MAX_TIMEOUT_S);
  assert.equal(args({ timeout_s: 'soon' }).ok, false);
  assert.equal(args({ model: ' haiku ' }).args.model, 'haiku');
});

// ---------- agents and models ----------

const claude = [
  { value: 'claude-opus-5-5', label: 'Opus 5.5' },
  { value: 'claude-sonnet-5-5', label: 'Sonnet 5.5' },
  { value: 'claude-haiku-5-5', label: 'Haiku 5.5' },
];

test('a model is found by id, label or a part of either', () => {
  assert.deepEqual(resolveModel(claude, 'claude-haiku-5-5'), { model: 'claude-haiku-5-5' });
  assert.deepEqual(resolveModel(claude, 'Haiku 5.5'), { model: 'claude-haiku-5-5' });
  assert.deepEqual(resolveModel(claude, 'haiku'), { model: 'claude-haiku-5-5' });
  assert.deepEqual(resolveModel(claude, 'OPUS'), { model: 'claude-opus-5-5' });
});

test('no model asked: the preferred one, else the first', () => {
  assert.deepEqual(resolveModel(claude, undefined, 'sonnet'), { model: 'claude-sonnet-5-5' });
  assert.deepEqual(resolveModel(claude), { model: 'claude-opus-5-5' });
  assert.deepEqual(resolveModel(claude, '', 'nothing-like-it'), { model: 'claude-opus-5-5' });
});

test('an unknown model lists the real ones; an agent with none says so', () => {
  const unknown = resolveModel(claude, 'gpt-9');
  assert.ok('error' in unknown);
  assert.match(unknown.error, /Opus 5\.5, Sonnet 5\.5, Haiku 5\.5/);
  assert.ok('error' in resolveModel([], 'x'));
});

test('agents are found by id or name and listed with their models', () => {
  const rows = [
    { id: 'claude', label: 'Claude', models: claude },
    { id: 'custom-ab12', label: 'LM Studio', models: [{ value: 'qwen3', label: 'qwen3' }] },
  ];
  assert.equal(findAgent(rows, 'CLAUDE').id, 'claude');
  assert.equal(findAgent(rows, 'lm studio').id, 'custom-ab12');
  assert.equal(findAgent(rows, 'custom-ab12').label, 'LM Studio');
  assert.equal(findAgent(rows, 'codex'), undefined);
  const text = formatAgents(rows);
  assert.match(text, /- claude: Opus 5\.5 \[claude-opus-5-5\]/);
  assert.match(text, /- custom-ab12 \(LM Studio\): qwen3/);
  assert.match(formatAgents([]), /No agent/);
});

// ---------- the report ----------

test('the report is the Handoff section', () => {
  const answer = 'Long story about what I did.\n\n## Handoff\nSTATUS: done\nRESULT: fixed it\nFILES: a.ts';
  assert.equal(reportDigest(answer), 'STATUS: done\nRESULT: fixed it\nFILES: a.ts');
  assert.equal(reportDigest('### handoff\nSTATUS: blocked'), 'STATUS: blocked');
});

test('an answer without a Handoff goes back whole, and its end when it is very long', () => {
  assert.equal(reportDigest('  It works.  '), 'It works.');
  const long = `${'a'.repeat(DIGEST_MAX)}THE END`;
  const cut = reportDigest(long);
  assert.ok(cut.startsWith('…'));
  assert.ok(cut.endsWith('THE END'));
  assert.equal(cut.length, DIGEST_MAX + 1);
});

test('a long Handoff is cut at the limit', () => {
  const cut = reportDigest(`## Handoff\n${'x'.repeat(DIGEST_MAX + 500)}`);
  assert.equal(cut.length, DIGEST_MAX + 1);
  assert.ok(cut.endsWith('…'));
});

test('a structured Handoff is parsed into fields for the harness', () => {
  const report = parseHandoff(`work summary\n\n## Handoff\nSTATUS: needs-plan-change\nRESULT: input is missing\nFILES: src/a.ts; src/b.ts\nFACTS: src/a.ts:12; src/b.ts:8\nVERIFY:\n- pnpm test -> fail\nOPEN: confirm the API shape\n- decide whether to retry`);
  assert.deepEqual(report, {
    status: 'needs-plan-change',
    result: 'input is missing',
    files: ['src/a.ts', 'src/b.ts'],
    facts: ['src/a.ts:12', 'src/b.ts:8'],
    verify: ['pnpm test -> fail'],
    open: ['confirm the API shape', 'decide whether to retry'],
  });
});

test('a Handoff with prose but no machine fields stays unstructured', () => {
  assert.equal(parseHandoff('## Handoff\nEverything looks good.'), null);
});

test('a task that gave no report says why in one line', () => {
  assert.equal(reasonLine('stopped'), 'Stopped by the user before it finished.');
  assert.equal(reasonLine('stopped', 'you stopped it with task_stop'), 'Stopped: you stopped it with task_stop');
  assert.match(reasonLine('timed out'), /^Timed out/);
  assert.equal(reasonLine('failed', 'the model\nwas   not found'), 'Failed: the model was not found');
  assert.equal(reasonLine('failed'), 'Failed: the worker gave no answer');
  assert.ok(!reasonLine('failed', 'x'.repeat(2000)).includes('\n'));
  assert.ok(reasonLine('failed', 'x'.repeat(2000)).length < 330);
});

test('the result header is read back from the tool result', () => {
  const head = { taskId: 't-ab12cd', childId: 'claude:3f2a-9b', label: 'Claude · Haiku 5.5', verdict: 'done', took: '1m 20s' };
  const text = formatTaskResult(head, 'STATUS: done');
  assert.equal(text, '[task t-ab12cd · chat claude:3f2a-9b · Claude · Haiku 5.5 · done · 1m 20s]\n\nSTATUS: done');
  assert.deepEqual(parseTaskResult(text), { taskId: 't-ab12cd', childId: 'claude:3f2a-9b', verdict: 'done' });
  assert.equal(parseTaskResult(formatTaskResult({ ...head, verdict: 'timed out' }, 'x')).verdict, 'timed out');
  assert.equal(parseTaskResult('some other output'), null);
  const started = startedText({ taskId: 't-1', childId: 'c-1', label: 'Codex' });
  assert.equal(parseTaskResult(started).verdict, 'running');
  assert.match(started, /background/);
});

// ---------- from the tool call to the card ----------

test('the call line is read whole, or cut off', () => {
  assert.deepEqual(argsOfCall('{"agent":"codex","model":"gpt-6-luna","task":"review the diff","wait":false}'), { agent: 'codex', model: 'gpt-6-luna', task: 'review the diff', wait: false });
  assert.deepEqual(argsOfCall('{"agent":"claude","task":"say \\"hi\\"\\nthen stop"}'), { agent: 'claude', model: undefined, task: 'say "hi"\nthen stop', wait: undefined });
  // A CLI cuts a long input and may end it with an ellipsis.
  const cut = argsOfCall('{"agent":"claude","wait":false,"task":"read every file under src/lib and list the exports tha…');
  assert.equal(cut.agent, 'claude');
  assert.equal(cut.wait, false);
  assert.equal(cut.task, 'read every file under src/lib and list the exports tha');
  assert.deepEqual(argsOfCall(''), { agent: undefined, model: undefined, task: undefined, wait: undefined });
});

test('parallel calls are matched to their tasks by what they asked, not by arrival order', () => {
  const free = [
    { id: 't-b', agent: 'codex', task: 'review the diff for bugs' },
    { id: 't-a', agent: 'claude', task: 'search the repository for usages of foo and list them' },
    { id: 't-c', agent: 'claude', task: 'summarise the README' },
  ];
  assert.equal(pickTask({ agent: 'claude', task: 'summarise the' }, free), 't-c');
  assert.equal(pickTask({ agent: 'claude', task: 'search the repository' }, free), 't-a');
  assert.equal(pickTask({ agent: 'codex', task: 'review the diff for bugs' }, free), 't-b');
  // Nothing recognisable: the first free task of that agent, then the first one.
  assert.equal(pickTask({ agent: 'claude' }, free), 't-a');
  assert.equal(pickTask({}, free), 't-b');
  assert.equal(pickTask({ agent: 'claude' }, []), undefined);
});

const record = (extra = {}) => ({ id: 't-1', caller: 'claude:x', childId: 'claude:w', state: 'running', wait: true, label: 'Claude · Haiku 5.5', agent: 'claude', agentId: 'claude', model: 'claude-haiku-5-5', task: 'do it', startedAt: 1000, ...extra });
const call = (extra = {}) => ({ command: 'mcp__agentdeck__delegate_task  {"agent":"claude","task":"do it"}', output: '', status: 'running', ...extra });

test('a running task is a working card, in the background when it does not wait', () => {
  assert.equal(cardOf(call(), record()).state, 'running');
  assert.equal(cardOf(call(), record({ wait: false })).state, 'background');
  assert.equal(cardOf(call(), record({ state: 'approval' })).state, 'approval');
  const card = cardOf(call(), record());
  assert.equal(card.taskId, 't-1');
  assert.equal(card.childId, 'claude:w');
  assert.equal(card.task, 'do it');
});

test('a finished task shows its report, a failed one its reason', () => {
  const done = cardOf(call({ status: 'ok' }), record({ state: 'done', digest: 'STATUS: done', endedAt: 5000 }));
  assert.equal(done.state, 'done');
  assert.equal(done.report, 'STATUS: done');
  assert.equal(done.endedAt, 5000);
  const stopped = cardOf(call({ status: 'ok' }), record({ state: 'stopped', note: 'Stopped by the user before it finished.' }));
  assert.equal(stopped.state, 'stopped');
  assert.match(stopped.report, /Stopped/);
  assert.equal(cardOf(call({ status: 'fail' }), record({ state: 'failed', note: 'Failed: x' })).state, 'failed');
});

test('a chat read back from its file still gets its cards, from the tool call and its result', () => {
  const output = formatTaskResult({ taskId: 't-9', childId: 'claude:kid', label: 'Codex · GPT', verdict: 'done', took: '9s' }, 'STATUS: done\nRESULT: ok');
  const card = cardOf({ command: 'agentdeck.delegate_task  {"agent":"codex","model":"gpt","task":"check"}', output, status: 'ok' });
  assert.equal(card.state, 'done');
  assert.equal(card.taskId, 't-9');
  assert.equal(card.childId, 'claude:kid');
  assert.equal(card.agent, 'codex');
  assert.equal(card.task, 'check');
  assert.equal(card.report, 'STATUS: done\nRESULT: ok');
  const failed = cardOf({ command: 'x', output: formatTaskResult({ taskId: 't-9', childId: 'c', label: 'L', verdict: 'timed out' }, 'Timed out'), status: 'fail' });
  assert.equal(failed.state, 'failed');
  // A task started in the background whose app run is over: nothing runs behind it any more.
  const lost = cardOf({ command: 'x  {"agent":"claude","task":"t","wait":false}', output: startedText({ taskId: 't-3', childId: 'c', label: 'L' }), status: 'ok' });
  assert.equal(lost.state, 'stopped');
  assert.equal(lost.wait, false);
  // Unknown output of a call that is still running.
  assert.equal(cardOf({ command: 'x  {"agent":"claude","task":"t"}', output: '', status: 'running' }).state, 'running');
  assert.equal(cardOf({ command: 'x  {"agent":"claude","task":"t","wait":false}', output: '', status: 'running' }).state, 'background');
});

test('a delegate_task tool event becomes a command block the card reads', () => {
  const input = '{"agent":"antigravity","task":"profile the build"}';
  let update = applyAgentEvent([], { kind: 'tool_call', id: 'toolu_1', name: 'mcp__agentdeck__delegate_task', input, change: null, background: false });
  const block = update.blocks[0];
  assert.equal(block.kind, 'command');
  assert.equal(kindOfCommand(block.command), 'delegate_task');
  assert.equal(cardOf(block).state, 'running');
  const output = formatTaskResult({ taskId: 't-5', childId: 'claude:c5', label: 'Antigravity · Gemini', verdict: 'done' }, 'STATUS: done');
  update = applyAgentEvent(update.blocks, { kind: 'tool_result', id: 'toolu_1', output, is_error: false });
  const settled = update.blocks[0];
  assert.equal(settled.status, 'ok');
  assert.equal(cardOf(settled).state, 'done');
  assert.equal(cardOf(settled).task, 'profile the build');
  // Codex names the same tool differently.
  const codex = applyAgentEvent([], { kind: 'tool_call', id: 'i1', name: 'agentdeck.delegate_task', input, change: null, background: false }).blocks[0];
  assert.equal(kindOfCommand(codex.command), 'delegate_task');
});

// ---------- what the agents are told ----------

test('the delegation hint stays short, because it goes with the first turn of every chat', () => {
  assert.ok(DELEGATE_HINT.length < 900, `${DELEGATE_HINT.length} characters`);
  for (const phrase of ['BIG or clearly separable', 'self-contained', 'wait:false', 'in parallel', 'yourself', "relay the user's message", 'final answer', '1-3 workers']) {
    assert.ok(DELEGATE_HINT.includes(phrase), `the hint lacks: ${phrase}`);
  }
  const lead = agentdeckLead('[AgentDeck] You run inside AgentDeck. Use the tools.', DELEGATE_HINT);
  assert.ok(lead.length < 900 + 200);
  assert.ok(lead.startsWith('[AgentDeck] You run inside AgentDeck. Use the tools. Delegation'));
  assert.equal(lead.match(/\[AgentDeck\]/g).length, 1);
  assert.ok(lead.endsWith('\n\n'));
  assert.equal(agentdeckLead('', ''), '');
});

test('the lead is one paragraph: a hand-back behind it is still recognised, and it is stripped from a saved prompt', () => {
  const lead = agentdeckLead('[AgentDeck] You run inside AgentDeck.', DELEGATE_HINT);
  const handback = `${HANDBACK_HEAD}\n\n### Claude\nDone.`;
  assert.equal(isHandbackPrompt(lead + handback), true);
  const digest = 'Earlier in this chat, other agents answered these turns (you were not part of them):\n\nUser: hi\n\n---\nNow the user says:\n';
  assert.equal(isHandbackPrompt(lead + digest + handback), true);
  assert.equal(isHandbackPrompt(lead + 'fix the login bug'), false);
  assert.equal(stripLead(lead + 'fix the login bug'), 'fix the login bug');
  assert.equal(stripLead('fix the login bug'), 'fix the login bug');
  assert.equal(stripLead('[AgentDeck] only a lead'), '');
});
