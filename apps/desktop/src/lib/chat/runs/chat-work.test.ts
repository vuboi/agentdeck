import assert from 'node:assert/strict';
import test from 'node:test';
import { isWorkActive, projectChatWork, stopTarget } from './chat-work.ts';
import type { Activity } from './activity.svelte';
import type { TaskRecord } from '../../tasks/delegate-task';

const shell: Activity = { key: 'shell', sessionId: 'chat', name: 'Bash', input: '{"command":"pnpm build","description":"Build the app"}', background: true, status: 'background', startedAt: 1, agent: 'codex', sourceMessageId: 'request' };
const project: Activity = { key: 'project', sessionId: 'chat', name: 'Run project', input: 'pnpm dev\nE:\\WORK\\PROJECTS\\agentdeck', background: false, status: 'background', startedAt: 1, agent: 'codex', sourceMessageId: 'request' };
const worker: TaskRecord = { id: 'worker', caller: 'chat', childId: 'child', task: 'Review changes', sourceMessageId: 'request', state: 'running', wait: false, agent: 'claude', agentId: 'claude', model: 'model', label: 'Claude', startedAt: 2 };

test('one tray shows native background commands and cross-provider workers without needing the transcript', () => {
  const items = projectChatWork([shell], [worker], []);
  assert.equal(items.length, 2);
  assert.equal(items.find((item) => item.kind === 'command')?.title, 'Build the app');
  assert.equal(items.find((item) => item.delegateId)?.agent, 'claude');
  assert.equal(items.every((item) => item.sourceMessageId === 'request'), true);
});

test('finishing work keeps its identity, output and result available', () => {
  const before = projectChatWork([shell], [], [])[0];
  const after = projectChatWork([{ ...shell, status: 'ok', output: 'Build passed', endedAt: 10 }], [], [])[0];
  assert.equal(after.id, before.id);
  assert.equal(after.detail, 'Build passed');
  assert.equal(after.state, 'done');
  assert.equal(isWorkActive(after), false);
});

test('only an AgentDeck-owned project run gets an independent Stop action', () => {
  const [run] = projectChatWork([project], [], []);
  const [command] = projectChatWork([shell], [], []);
  assert.equal(run.canStop, true);
  assert.equal(run.activityKey, 'project');
  assert.equal(command.canStop, false);
});

test('Stop targets the row-owned work and never falls back to the main chat', () => {
  assert.equal(stopTarget({ activityKey: 'project', canStop: true }), 'project');
  assert.equal(stopTarget({ delegateId: 'worker', canStop: true }), 'delegate');
  assert.equal(stopTarget({ childId: 'child', canStop: true }), 'child');
  assert.equal(stopTarget({ activityKey: 'native-shell', canStop: false }), null);
  assert.equal(stopTarget({ canStop: true }), null);
});

test('a completed worker waiting for integration remains visible as work awaiting its result', () => {
  const item = projectChatWork([], [{ ...worker, state: 'done', reportState: 'waiting', digest: 'Review passed' }], [])[0];
  assert.equal(item.state, 'waiting');
  assert.equal(isWorkActive(item), true);
  assert.equal(item.canStop, false);
  assert.match(item.detail, /Review passed/);
});

test('workflow nodes already represented by delegate records are not duplicated', () => {
  const items = projectChatWork([], [worker], [{ id: 'flow', main: 'chat', task: 'Review', startedAt: 1, handback: 'waiting', nodes: [{ childId: 'child', agent: 'claude', model: 'model', label: 'Claude', startedAt: 2, state: 'running' }] }]);
  assert.equal(items.length, 1);
});

test('a restored worker is unconfirmed rather than pretending it was stopped or is still running', () => {
  const item = projectChatWork([], [{ ...worker, state: 'stopped', note: 'Stopped: AgentDeck was closed or reloaded while it ran.' }], [])[0];
  assert.equal(item.state, 'unknown');
  assert.equal(item.canStop, false);
});
