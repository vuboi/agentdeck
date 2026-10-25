// @ts-nocheck: run by node (type stripping), not by the Svelte type checker.
// Run with: node --experimental-strip-types --test tests/continuations.test.ts
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { joinContinuations } from '../src/lib/continuations.ts';

const text = (t) => ({ kind: 'response', text: t });

test('a finished reply joins the one before it from the same agent', () => {
  const first = { id: 'a', role: 'assistant', time: '1:00', blocks: [text('handing out')], agent: 'codex', ms: 1000, cost: 0.1, tokens: { input: 10, output: 5, cacheRead: 0, cacheWrite: 0 } };
  const second = { id: 'b', role: 'assistant', time: '1:01', blocks: [text('answer')], agent: 'codex', ms: 500, cost: 0.2, tokens: { input: 3, output: 2, cacheRead: 1, cacheWrite: 0 } };
  const out = joinContinuations([first, second]);
  assert.equal(out.length, 1);
  assert.equal(out[0].id, 'a');
  assert.equal(out[0].time, '1:00');
  assert.equal(out[0].blocks.length, 2);
  assert.equal(out[0].ms, 1500);
  assert.ok(Math.abs(out[0].cost - 0.3) < 1e-9);
  assert.deepEqual(out[0].tokens, { input: 13, output: 7, cacheRead: 1, cacheWrite: 0 });
});

test('users, other agents and live replies stay apart', () => {
  const user = { id: 'u', role: 'user', time: '1', blocks: [text('hi')] };
  const a = { id: 'a', role: 'assistant', time: '1', blocks: [], agent: 'codex' };
  const b = { id: 'b', role: 'assistant', time: '1', blocks: [], agent: 'claude' };
  const live = { id: 'l', role: 'assistant', time: '1', blocks: [], agent: 'claude', live: true };
  assert.equal(joinContinuations([user, a, user, a]).length, 4);
  assert.equal(joinContinuations([a, b]).length, 2);
  assert.equal(joinContinuations([b, live]).length, 2);
});
