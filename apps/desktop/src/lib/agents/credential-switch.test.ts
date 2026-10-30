import assert from 'node:assert/strict';
import test from 'node:test';
import { cancelCredentialRotations, credentialBusy, credentialDelay, exclusiveCredential, rotateCredential, waitForCredential } from './credential-switch.ts';

test('shared connector mutations serialize while different connectors proceed', async () => {
  const order: string[] = [];
  let release!: () => void;
  const gate = new Promise<void>((resolve) => { release = resolve; });
  const first = exclusiveCredential('codex', async () => { order.push('first'); await gate; order.push('restored'); });
  const second = exclusiveCredential('codex', async () => { order.push('second'); });
  await exclusiveCredential('claude', async () => { order.push('independent'); });
  assert.equal(credentialBusy('codex'), true);
  assert.equal(order.includes('second'), false);
  release();
  await Promise.all([first, second, waitForCredential('codex')]);
  assert.ok(order.indexOf('second') > order.indexOf('restored'));
  assert.equal(credentialBusy('codex'), false);
});

test('manual switch cancels an automatic delay and waits for rollback', async () => {
  const order: string[] = [];
  let ready!: () => void;
  const started = new Promise<void>((resolve) => { ready = resolve; });
  const controller = new AbortController();
  const automatic = rotateCredential('copilot', { signal: controller.signal, tried: new Set(), model: 'model' }, async (signal) => {
    ready();
    try { await credentialDelay(signal); order.push('commit'); }
    finally { order.push('rollback'); }
  });
  const rejection = assert.rejects(automatic, { name: 'AbortError' });
  await started;
  cancelCredentialRotations('copilot');
  await exclusiveCredential('copilot', async () => { order.push('manual'); });
  await rejection;
  assert.deepEqual(order, ['rollback', 'manual']);
});

test('a cancelled queued recovery never activates a credential', async () => {
  let release!: () => void;
  const gate = new Promise<void>((resolve) => { release = resolve; });
  const busy = exclusiveCredential('agy', () => gate);
  const controller = new AbortController();
  let activated = false;
  const queued = rotateCredential('agy', { signal: controller.signal, tried: new Set(), model: 'gemini' }, async () => { activated = true; });
  const rejection = assert.rejects(queued, { name: 'AbortError' });
  controller.abort();
  release();
  await busy;
  await rejection;
  assert.equal(activated, false);
});
