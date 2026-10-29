// Run: node --experimental-strip-types --test tests/inflight.test.mjs
import test from 'node:test';
import assert from 'node:assert/strict';
import { parseInflight, planRunRestore, terminalVerdict, withTerminal, withoutTerminal } from '../src/lib/chat/runs/inflight.ts';

const run = (over = {}) => ({ sessionId: 's', title: 't', project: 'p', input: 'pnpm dev\nC:\\app', startedAt: 1000, settled: false, ...over });
const server = (over = {}) => ({ key: 4200, command: 'pnpm dev', cwd: 'C:\\app', running: true, age: 30, ...over });
const none = { ports: new Set(), inputs: [] };

test('parse drops garbage and keeps valid records', () => {
  assert.deepEqual(parseInflight(null), { terminals: [], runs: [] });
  assert.deepEqual(parseInflight('{not json'), { terminals: [], runs: [] });
  const out = parseInflight(JSON.stringify({ terminals: [{ tabId: 'term-1', command: 'ng serve', startedAt: 5 }, { tabId: 3 }], runs: [{ input: 'a\nb', startedAt: 9 }, null] }));
  assert.equal(out.terminals.length, 1);
  assert.equal(out.runs.length, 1);
  assert.equal(out.runs[0].settled, false);
});

test('one record per tab', () => {
  let state = withTerminal({ terminals: [], runs: [] }, { tabId: 'term-1', command: 'a', startedAt: 1 });
  state = withTerminal(state, { tabId: 'term-1', command: 'b', startedAt: 2 });
  assert.equal(state.terminals.length, 1);
  assert.equal(state.terminals[0].command, 'b');
  assert.equal(withoutTerminal(state, 'term-1').terminals.length, 0);
});

test('terminal verdicts', () => {
  assert.equal(terminalVerdict(false, false), 'stopped');
  assert.equal(terminalVerdict(true, true), 'ok');
  assert.equal(terminalVerdict(true, false), 'running');
});

test('a running server with its record resumes with the original start', () => {
  const plan = planRunRestore([run({ settled: true })], [server()], none, 50_000);
  assert.equal(plan.resume.length, 1);
  assert.equal(plan.resume[0].startedAt, 1000);
  assert.equal(plan.stopped.length + plan.forget.length, 0);
});

test('a server without a record is rebuilt from its age', () => {
  const plan = planRunRestore([], [server()], none, 100_000);
  assert.equal(plan.resume[0].startedAt, 70_000);
  assert.equal(plan.resume[0].record, undefined);
});

test('records of gone servers: unsettled ones are stopped, settled ones forgotten', () => {
  const plan = planRunRestore([run(), run({ input: 'x\ny', settled: true })], [], none, 1);
  assert.equal(plan.stopped.length, 1);
  assert.equal(plan.forget.length, 1);
  const dead = planRunRestore([run()], [server({ running: false })], none, 1);
  assert.equal(dead.resume.length, 0);
  assert.equal(dead.stopped.length, 1);
});

test('a second restore makes no duplicates', () => {
  const plan = planRunRestore([run({ port: 4200 })], [server()], { ports: new Set([4200]), inputs: [] }, 1);
  assert.equal(plan.resume.length, 0);
  const byInput = planRunRestore([], [server()], { ports: new Set(), inputs: ['pnpm dev\nC:\\app'] }, 1);
  assert.equal(byInput.resume.length, 0);
});

test('two servers do not take the same record', () => {
  const plan = planRunRestore([run({ port: 4200 })], [server(), server({ key: 4300 })], none, 100_000);
  assert.equal(plan.resume.length, 2);
  assert.equal(plan.resume.filter((item) => item.record).length, 1);
});

import { stripLocationPrefix, terminalStep } from '../src/lib/chat/runs/inflight.ts';

test('the same command run again is a new run with its own entry', () => {
  assert.equal(terminalStep(undefined, { runId: 1 }), 'start');
  assert.equal(terminalStep(1, { runId: 1 }), 'none');
  assert.equal(terminalStep(1, null), 'finish');
  assert.equal(terminalStep(undefined, null), 'none');
  // Same command, same tab, after the first ended: nothing is open, so the next run starts.
  assert.equal(terminalStep(undefined, { runId: 2 }), 'start');
  // Run again before the end was seen: the old one finishes and the new one starts.
  assert.equal(terminalStep(1, { runId: 2 }), 'restart');
});

test('the folder prefix of the Run button is not part of the command', () => {
  assert.deepEqual(stripLocationPrefix("Set-Location -LiteralPath 'C:\\a b\\it''s'; nx serve app"), { command: 'nx serve app', cwd: "C:\\a b\\it's" });
  assert.deepEqual(stripLocationPrefix("cd 'it'\\''s' && ls"), { command: 'ls', cwd: "it's" });
  assert.deepEqual(stripLocationPrefix("cd '/home/x' && pnpm dev"), { command: 'pnpm dev', cwd: '/home/x' });
  assert.deepEqual(stripLocationPrefix('  ng serve '), { command: 'ng serve' });
});
