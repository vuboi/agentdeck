import assert from 'node:assert/strict';
import test from 'node:test';
import { IDLE, canQueue, canSend, step, type RunState } from './run-state.ts';

const running = (epoch = 1): RunState => ({ kind: 'running', epoch, substantive: false, waiting: { approval: 0, auth: 0, question: 0 } });
const kinds = (transitions: { to: RunState }[]) => transitions.map((item) => item.to.kind);

test('a turn goes idle -> starting -> running -> idle, with a transition at each change of phase', () => {
  let state: RunState = IDLE;
  const spawned = step(state, { type: 'spawn', epoch: 1 });
  assert.deepEqual(kinds(spawned.transitions), ['starting']);
  state = spawned.state;
  const started = step(state, { type: 'turnStarted', epoch: 1 });
  assert.equal(started.state.kind, 'running');
  state = started.state;
  const wrote = step(state, { type: 'output', text: 'hello' });
  assert.equal(wrote.transitions.length, 0, 'output inside running is not a phase change');
  const done = step(wrote.state, { type: 'turnResult', isError: false });
  assert.equal(done.state.kind, 'idle');
  assert.equal(canSend(done.state), true);
});

test('cancel goes to idle, never to error, and the kill that follows is absorbed', () => {
  const stopped = step(running(), { type: 'cancel' });
  assert.equal(stopped.state.kind, 'idle');
  assert.equal(step(stopped.state, { type: 'detached' }).state.kind, 'idle');
});

test('a process that ends mid-turn is a crash; one that ends after its result is not', () => {
  const crashed = step({ kind: 'running', epoch: 1, substantive: true, waiting: { approval: 0, auth: 0, question: 0 } }, { type: 'detached' });
  assert.equal(crashed.state.kind, 'error');
  assert.equal(crashed.state.kind === 'error' && crashed.state.reason, 'crashed');
  const after = step(step(running(), { type: 'output', text: 'x' }).state, { type: 'turnResult', isError: false });
  assert.equal(step(after.state, { type: 'detached' }).state.kind, 'idle');
});

test('a process that dies while starting is an error (the bad-flag or bad-login case)', () => {
  const starting: RunState = { kind: 'starting', epoch: 1 };
  const dead = step(starting, { type: 'detached' });
  assert.equal(dead.state.kind, 'error');
});

test('a bad resume fails on the first frame: its message reaches the error', () => {
  const failed = step({ kind: 'starting', epoch: 1 }, { type: 'turnResult', isError: true, message: 'No conversation found' });
  assert.deepEqual(failed.state, { kind: 'error', reason: 'backend', message: 'No conversation found' });
});

test('a turn that finishes with nothing written is an error, not a quiet success', () => {
  const empty = step(running(), { type: 'turnResult', isError: false });
  assert.equal(empty.state.kind === 'error' && empty.state.reason, 'empty');
});

test('an error or idle run ignores the events of a finished turn', () => {
  const failed: RunState = { kind: 'error', reason: 'crashed', message: '' };
  assert.equal(step(failed, { type: 'output', text: 'late' }).state, failed);
  assert.equal(step(IDLE, { type: 'output', text: 'late' }).state, IDLE);
});

test('a stale turn start (an epoch that does not advance) is dropped', () => {
  const current = running(5);
  assert.equal(step(current, { type: 'turnStarted', epoch: 5 }).state, current);
  assert.equal(step(current, { type: 'turnStarted', epoch: 4 }).state, current);
  assert.equal(step(current, { type: 'turnStarted', epoch: 6 }).state.kind, 'running');
});

test('waiting for the user is a count beside running: the three kinds are counted apart and return to running at zero', () => {
  let state = running();
  state = step(state, { type: 'waitStart', kind: 'approval' }).state;
  state = step(state, { type: 'waitStart', kind: 'question' }).state;
  assert.equal(canQueue(state, true), false, 'a run waiting for an answer cannot take a queued message');
  const oneBack = step(state, { type: 'waitEnd', kind: 'approval' });
  assert.equal(oneBack.transitions.length, 0, 'one of two waits resolved: still waiting');
  const allBack = step(oneBack.state, { type: 'waitEnd', kind: 'question' });
  assert.equal(allBack.state.kind, 'running');
  assert.equal(canQueue(allBack.state, true), true);
});

test('a wait that ends more often than it started never goes below zero', () => {
  const state = step(running(), { type: 'waitEnd', kind: 'auth' }).state;
  assert.equal(state.kind === 'running' && state.waiting.auth, 0);
});

test('can queue only while running with no wait, and only where the backend takes input mid-turn', () => {
  assert.equal(canQueue(running(), true), true);
  assert.equal(canQueue(running(), false), false);
  assert.equal(canQueue(IDLE, true), false);
  assert.equal(canSend(running()), false);
});
