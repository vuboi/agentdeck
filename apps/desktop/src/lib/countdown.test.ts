import { test } from 'node:test';
import assert from 'node:assert/strict';
import { resetCountdown } from './countdown.ts';

test('a reset countdown shows days, hours, minutes and seconds', () => {
  const now = 1_000_000_000_000;
  const at = (s: number) => now / 1000 + s;
  assert.equal(resetCountdown(at(5), now), '5s');
  assert.equal(resetCountdown(at(65), now), '1m 05s');
  assert.equal(resetCountdown(at(3 * 3600 + 4 * 60 + 5), now), '3h 04m 05s');
  assert.equal(resetCountdown(at(2 * 86400 + 3 * 3600 + 4 * 60 + 5), now), '2d 3h 04m 05s');
  assert.equal(resetCountdown(at(-10), now), '0s');
});

test('a reset time reported a few seconds off stays put; a new window is taken', async () => {
  const { steadyReset } = await import('./countdown.ts');
  assert.equal(steadyReset('codex:a:5h', 1000), 1000);
  assert.equal(steadyReset('codex:a:5h', 1002), 1000);
  assert.equal(steadyReset('codex:a:5h', 998), 1000);
  assert.equal(steadyReset('codex:a:5h', 1000 + 5 * 3600), 1000 + 5 * 3600);
});
