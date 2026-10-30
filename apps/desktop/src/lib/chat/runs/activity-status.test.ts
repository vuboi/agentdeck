import assert from 'node:assert/strict';
import test from 'node:test';
import { endedByTask, statusAfterResult } from './activity-status.ts';

test('a call its task notification ended keeps that status when the result comes later', () => {
  // Foreground Bash: task_notification (completed) first, tool_result after.
  const ended = { background: true, status: 'ok' as const, nativeTaskId: 'bau2wzszj', endedAt: 2 };
  assert.equal(endedByTask(ended), true);
  assert.equal(statusAfterResult(ended, false), 'ok');
  assert.equal(statusAfterResult({ ...ended, status: 'stopped' }, false), 'stopped');
});

test('a background shell whose result comes before its task end stays background', () => {
  // run_in_background: the result carries the shell id; no task event yet.
  const shell = { background: true, status: 'running' as const, nativeTaskId: undefined, endedAt: undefined };
  assert.equal(endedByTask(shell), false);
  assert.equal(statusAfterResult(shell, false), 'background');
  // A task event that started but has not ended yet is still background.
  assert.equal(statusAfterResult({ background: true, status: 'background', nativeTaskId: 'bau2wzszj', endedAt: undefined }, false), 'background');
});

test('a plain foreground call becomes ok', () => {
  const call = { background: false, status: 'running' as const, endedAt: undefined };
  assert.equal(endedByTask(call), false);
  assert.equal(statusAfterResult(call, false), 'ok');
});

test('an error result becomes fail unless the task already ended the call', () => {
  assert.equal(statusAfterResult({ background: false, status: 'running' }, true), 'fail');
  assert.equal(statusAfterResult({ background: true, status: 'background' }, true), 'fail');
  assert.equal(statusAfterResult({ background: true, status: 'fail', nativeTaskId: 'x', endedAt: 3 }, true), 'fail');
  assert.equal(statusAfterResult({ background: true, status: 'ok', nativeTaskId: 'x', endedAt: 3 }, true), 'ok');
});
