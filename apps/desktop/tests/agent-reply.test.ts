// @ts-nocheck: run by node (type stripping), not by the Svelte type checker.
// Run with: node --test tests/agent-reply.test.ts
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { applyAgentEvent } from '../src/lib/agent-reply.ts';

test('deltas join into one response block', () => {
  let blocks = [{ kind: 'loading', label: 'Thinking' }] as const;
  let u = applyAgentEvent([...blocks], { kind: 'delta', text: 'hel' });
  u = applyAgentEvent(u.blocks, { kind: 'delta', text: 'lo' });
  assert.deepEqual(u.blocks, [{ kind: 'response', text: 'hello', streaming: true }]);
  assert.equal(u.finished, false);
});

test('loading placeholder is removed by the first real event', () => {
  const u = applyAgentEvent([{ kind: 'loading', label: 'Thinking' }], { kind: 'thinking', text: 'hmm' });
  assert.deepEqual(u.blocks, [{ kind: 'thinking', text: 'hmm', streaming: true }]);
});

test('thinking then answer stay separate blocks', () => {
  let u = applyAgentEvent([], { kind: 'thinking', text: 'a' });
  u = applyAgentEvent(u.blocks, { kind: 'delta', text: 'b' });
  assert.deepEqual(u.blocks, [
    { kind: 'thinking', text: 'a', streaming: false },
    { kind: 'response', text: 'b', streaming: true },
  ]);
});

test('tool call adds a running command block', () => {
  const u = applyAgentEvent([], { kind: 'tool_call', id: 't1', name: 'Bash', input: 'ls' });
  assert.deepEqual(u.blocks, [{ kind: 'command', id: 't1', command: 'Bash  ls', output: '', status: 'running' }]);
});

test('a tool result fills in the output and status of its call', () => {
  let u = applyAgentEvent([], { kind: 'tool_call', id: 't1', name: 'Bash', input: 'ls' });
  u = applyAgentEvent(u.blocks, { kind: 'tool_result', id: 't1', output: 'a\nb', is_error: false });
  assert.equal(u.blocks[0].output, 'a\nb');
  assert.equal(u.blocks[0].status, 'ok');
  u = applyAgentEvent(u.blocks, { kind: 'tool_result', id: 't1', output: 'boom', is_error: true });
  assert.equal(u.blocks[0].status, 'fail');
});

test('done keeps streamed text and returns the conversation id', () => {
  const streamed = [{ kind: 'response', text: 'pong', streaming: true }] as const;
  const u = applyAgentEvent([...streamed], { kind: 'done', session_id: 'abc', text: 'pong', cost_usd: 0.1 });
  assert.equal(u.finished, true);
  assert.equal(u.agentSession, 'abc');
  assert.deepEqual(u.blocks, [{ kind: 'response', text: 'pong', streaming: false }]);
});

test('done fills the answer when nothing streamed', () => {
  const u = applyAgentEvent([], { kind: 'done', session_id: null, text: 'final', cost_usd: null });
  assert.deepEqual(u.blocks, [{ kind: 'response', text: 'final' }]);
  assert.equal(u.agentSession, undefined);
});

test('error ends the turn and shows the message', () => {
  const u = applyAgentEvent([{ kind: 'loading', label: 'Thinking' }], { kind: 'error', message: 'rate limited' });
  assert.equal(u.finished, true);
  assert.deepEqual(u.blocks, [{ kind: 'response', text: '⚠ rate limited' }]);
});

test('a running command settles to ok when the turn ends', () => {
  let u = applyAgentEvent([], { kind: 'tool_call', id: 't', name: 'Bash', input: '' });
  u = applyAgentEvent(u.blocks, { kind: 'done', session_id: null, text: '', cost_usd: null });
  assert.equal(u.blocks[0].status, 'ok');
});

test('a new block closes the previous streaming block', () => {
  let u = applyAgentEvent([], { kind: 'thinking', text: 'a' });
  u = applyAgentEvent(u.blocks, { kind: 'tool_call', id: 't', name: 'Read', input: 'x' });
  assert.equal(u.blocks[0].streaming, false);
  assert.equal(u.blocks[1].status, 'running');
});
