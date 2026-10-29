import { test } from 'node:test';
import assert from 'node:assert/strict';
import { within } from './within.ts';

const never = () => new Promise<string>(() => {});

test('an answer in time passes through', async () => {
  assert.equal(await within(Promise.resolve('ok'), 50, 'x'), 'ok');
});

test('a call that never answers fails and says what it waited for', async () => {
  await assert.rejects(within(never(), 10, 'saving the attached pictures'), /saving the attached pictures/);
});

test('a fallback replaces a late answer', async () => {
  assert.equal(await within(never(), 10, 'x', () => 'raw'), 'raw');
});

test('an error of the call is passed on, not hidden by the timer', async () => {
  await assert.rejects(within(Promise.reject(new Error('disk full')), 50, 'x'), /disk full/);
});
