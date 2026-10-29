import assert from 'node:assert/strict';
import test from 'node:test';
import { deliveryFor, isRedundantAgentWait, type RunLane } from './run-lanes.ts';

const lane = (
  role: RunLane['role'],
  state: RunLane['state'],
  backgroundWork = false,
): RunLane => ({ id: `${role}-${state}`, role, state, backgroundWork });

test('a background waiter releases one independent listener lane', () => {
  assert.equal(deliveryFor([lane('primary', 'waiting')]), 'listener');
  assert.equal(deliveryFor([lane('primary', 'running', true)]), 'listener');
});

test('an actively generating foreground listener keeps later messages in the queue', () => {
  assert.equal(
    deliveryFor([
      lane('primary', 'waiting'),
      lane('listener', 'running'),
    ]),
    'queue',
  );
});

test('ordinary active work and unknown provider state keep current behavior', () => {
  assert.equal(deliveryFor([lane('primary', 'running')]), 'queue');
  assert.equal(deliveryFor([]), 'send');
});

test('a completed waiter does not leave the listener lane unlocked', () => {
  assert.equal(deliveryFor([lane('primary', 'done')]), 'send');
});

test('Codex wait count is hidden when Agent activity already shows the child tasks', () => {
  assert.equal(isRedundantAgentWait('wait  3 agent(s)', true), true);
  assert.equal(isRedundantAgentWait('wait  3 agent(s)', false), false);
  assert.equal(isRedundantAgentWait('Run command pnpm check', true), false);
});
