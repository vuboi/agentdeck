import assert from 'node:assert/strict';
import test from 'node:test';
import { runUsage } from './run-usage.ts';

const tokens = (input: number, output: number) => ({ input, output, cacheRead: 0, cacheWrite: 0 });
const estimate = (t: { input: number; output: number }) => (t.input + t.output) / 1000;

test('a run adds up the replies a newer message cut off, up to the next message of the user', () => {
  const list = [
    { role: 'user' },
    { role: 'assistant', agent: 'codex', model: 'm', tokens: tokens(100, 10), cost: 0.5 },
    { role: 'user' },
    { role: 'assistant', agent: 'codex', model: 'm', tokens: tokens(200, 20) },
    { role: 'assistant', agent: 'codex', model: 'm', tokens: tokens(300, 30) },
  ];
  const usage = runUsage(list, 3, estimate)!;
  assert.deepEqual(usage.tokens, { input: 500, output: 50, cacheRead: 0, cacheWrite: 0 });
  assert.equal(usage.replies, 2);
  assert.equal(usage.exact, false);
  assert.ok(Math.abs(usage.cost! - 0.55) < 1e-9);
  assert.deepEqual(runUsage(list, 3, estimate), runUsage(list, 4, estimate));
});

test('the run stops at another agent or model, and a reply with no numbers adds nothing', () => {
  const list = [
    { role: 'assistant', agent: 'claude', model: 'a', tokens: tokens(1, 1), cost: 1 },
    { role: 'assistant', agent: 'codex', model: 'b', tokens: tokens(2, 2), cost: 2 },
    { role: 'assistant', agent: 'codex', model: 'b' },
  ];
  assert.equal(runUsage(list, 1, estimate)!.replies, 1);
  assert.equal(runUsage(list, 1, estimate)!.exact, true);
  assert.equal(runUsage(list, 0, estimate)!.cost, 1);
  assert.equal(runUsage([{ role: 'user' }], 0, estimate), undefined);
});
