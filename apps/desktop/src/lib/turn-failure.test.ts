import { test } from 'node:test';
import assert from 'node:assert/strict';
import { classifyFailure, nextAgent, planRecovery, type RecoveryInput } from './turn-failure.ts';

test('classifies the errors agents end with', () => {
  assert.equal(classifyFailure("Claude AI usage limit reached|1761500000"), 'limit');
  assert.equal(classifyFailure('You have hit your usage limit. Try again in 2h 13m.'), 'limit');
  assert.equal(classifyFailure('stream error: 429 Too Many Requests'), 'limit');
  assert.equal(classifyFailure('API Error: 529 {"type":"overloaded_error"}'), 'transient');
  assert.equal(classifyFailure('request failed: ECONNRESET'), 'transient');
  assert.equal(classifyFailure('Invalid API key · Please run /login'), 'auth');
  assert.equal(classifyFailure('401 Unauthorized: token expired'), 'auth');
  assert.equal(classifyFailure('could not start codex: Check that its CLI is installed'), 'missing');
  assert.equal(classifyFailure('stopped'), 'stopped');
  assert.equal(classifyFailure('agent ended without a result'), 'other');
  assert.equal(classifyFailure("You've hit your usage limit. Upgrade to Pro or try again at 3:42 PM."), 'limit');
  assert.equal(classifyFailure('You exceeded your current quota, please check your plan'), 'limit');
  // Too much context is not a usage limit: another try (or agent) is the answer, not waiting hours.
  assert.notEqual(classifyFailure('prompt is too long: exceeded the maximum context length'), 'limit');
});

const base: RecoveryInput = { kind: 'transient', retries: 0, chain: ['claude', 'codex', 'opencode'], current: 'claude', tried: ['claude'] };

test('a passing fault is retried, then the next agent carries on', () => {
  assert.deepEqual(planRecovery(base), { action: 'retry', delayMs: 3000 });
  assert.deepEqual(planRecovery({ ...base, retries: 1 }), { action: 'retry', delayMs: 12000 });
  assert.deepEqual(planRecovery({ ...base, retries: 2 }), { action: 'fallback', agent: 'codex' });
});

test('a limit never switches account: the next provider carries on', () => {
  const limit = { ...base, kind: 'limit' as const };
  assert.deepEqual(planRecovery(limit), { action: 'fallback', agent: 'codex' });
  assert.deepEqual(planRecovery({ ...limit, current: 'codex', tried: ['claude', 'codex'] }), { action: 'fallback', agent: 'opencode' });
});

test('a user stop, or no agent left, stops', () => {
  assert.equal(planRecovery({ ...base, kind: 'stopped' }).action, 'stop');
  assert.equal(planRecovery({ ...base, kind: 'limit', tried: ['claude', 'codex', 'opencode'] }).action, 'stop');
  assert.equal(planRecovery({ ...base, kind: 'auth', chain: ['claude'] }).action, 'stop');
});

test('the next agent wraps round the chain and skips the ones tried', () => {
  assert.equal(nextAgent(['claude', 'codex', 'opencode'], 'opencode', ['opencode']), 'claude');
  assert.equal(nextAgent(['claude', 'codex'], 'antigravity', ['antigravity']), 'claude');
  assert.equal(nextAgent(['claude'], 'claude', ['claude']), undefined);
});
