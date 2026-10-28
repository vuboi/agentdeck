import assert from 'node:assert/strict';
import test from 'node:test';
import { REWORK_ROUNDS, scorecard } from './scorecard.ts';

const reply = (sessionId: string, model: string, ms = 1000) => ({ sessionId, model, ms });
const task = (childId: string, state = 'done', took = 60_000) => ({ childId, state, startedAt: 0, endedAt: took });

test('rounds are the replies of the worker chat of a task', () => {
  const replies = [reply('w1', 'fast'), reply('w2', 'slow'), reply('w2', 'slow'), reply('w2', 'slow'), reply('w2', 'slow')];
  const rows = scorecard(replies, [task('w1'), task('w2')], () => 0.01);
  const slow = rows.find((row) => row.model === 'slow')!;
  const fast = rows.find((row) => row.model === 'fast')!;
  assert.equal(slow.avgRounds, 4);
  assert.equal(slow.rework, 1);
  assert.equal(fast.avgRounds, 1);
  assert.equal(fast.firstTry, 1);
});

test('the model that needed rework comes first', () => {
  const replies = [reply('a', 'good'), reply('b', 'bad'), reply('b', 'bad'), reply('b', 'bad')];
  const rows = scorecard(replies, [task('a'), task('b')], () => null);
  assert.deepEqual(rows.map((row) => row.model), ['bad', 'good']);
  assert.equal(rows[0].rework, 1);
  assert.equal(REWORK_ROUNDS, 3);
});

test('tasks split into first try, one fix and rework; failures are counted', () => {
  const replies = [reply('t1', 'm'), reply('t2', 'm'), reply('t2', 'm'), reply('t3', 'm'), reply('t3', 'm'), reply('t3', 'm'), reply('t4', 'm')];
  const rows = scorecard(replies, [task('t1'), task('t2'), task('t3'), task('t4', 'failed')], () => null);
  assert.equal(rows.length, 1);
  assert.deepEqual([rows[0].firstTry, rows[0].oneFix, rows[0].rework, rows[0].tasks, rows[0].done, rows[0].failed], [2, 1, 1, 4, 3, 1]);
});

test('time and cost are means over what has them', () => {
  const replies = [reply('a', 'm', 2000), reply('b', 'm', 4000), reply('c', 'm', 0)];
  const costs = new Map([[replies[0], 0.02], [replies[1], 0.04]]);
  const rows = scorecard(replies, [task('a', 'done', 30_000), task('b', 'done', 90_000)], (item) => costs.get(item) ?? null);
  assert.equal(rows[0].avgReplyMs, 3000);
  assert.equal(rows[0].avgTaskMs, 60_000);
  assert.equal(rows[0].cost, 0.06);
  assert.equal(rows[0].costPerTask, 0.03);
});

test('a task with no reply yet, and a model with no task, do not break it', () => {
  const rows = scorecard([reply('x', 'only-chat')], [task('queued-chat')], () => null);
  assert.equal(rows.length, 1);
  assert.equal(rows[0].tasks, 0);
  assert.equal(rows[0].avgRounds, 0);
  assert.equal(rows[0].costPerTask, null);
});
