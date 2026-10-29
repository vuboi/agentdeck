import { test } from 'node:test';
import assert from 'node:assert/strict';
import { parseResetAt } from './limit-reset.ts';

// 2026-10-27 01:00 UTC = 08:00 in Bangkok (UTC+7).
const NOW = Date.UTC(2026, 9, 27, 1, 0, 0);
const iso = (ms: number | null) => (ms === null ? null : new Date(ms).toISOString());

test('a relative time', () => {
  assert.equal(iso(parseResetAt('You have hit your usage limit. Try again in 4 hours 59 minutes.', NOW, 'UTC')), '2026-10-27T05:59:00.000Z');
  assert.equal(iso(parseResetAt('5-hour limit reached · resets in 2h 30m', NOW, 'UTC')), '2026-10-27T03:30:00.000Z');
  assert.equal(iso(parseResetAt('rate limited, retry in 45 seconds', NOW, 'UTC')), '2026-10-27T01:00:45.000Z');
});

test('a clock time with its own zone is the next time that clock shows it', () => {
  // 12pm in Bangkok is 05:00 UTC the same day (it is 08:00 there now).
  assert.equal(iso(parseResetAt("You've hit your weekly limit · resets 12pm (Asia/Bangkok)", NOW, 'UTC')), '2026-10-27T05:00:00.000Z');
  // 4:31am in Bangkok already passed today: tomorrow, 21:31 UTC.
  assert.equal(iso(parseResetAt('Session limit reached · resets 4:31am (Asia/Bangkok)', NOW, 'UTC')), '2026-10-27T21:31:00.000Z');
});

test('a clock time without a zone uses the local one', () => {
  assert.equal(iso(parseResetAt('try again at 3:30 PM', NOW, 'UTC')), '2026-10-27T15:30:00.000Z');
});

test('a dated time', () => {
  assert.equal(iso(parseResetAt('You can try again at Oct 28th, 2026 3:31 AM.', NOW, 'UTC')), '2026-10-28T03:31:00.000Z');
  assert.equal(iso(parseResetAt('resets Nov 2, 9am (Asia/Bangkok)', NOW, 'UTC')), '2026-11-02T02:00:00.000Z');
});

test('no time in the message', () => {
  assert.equal(parseResetAt('429 too many requests', NOW, 'UTC'), null);
  assert.equal(parseResetAt('quota exceeded, resets 5', NOW, 'UTC'), null);
});
