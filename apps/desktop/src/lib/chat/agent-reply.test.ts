import test from 'node:test';
import assert from 'node:assert/strict';
import { applyAgentEvent } from './agent-reply.ts';
import type { AgentEvent } from './agent';
import type { Block } from './mock';

const run = (events: AgentEvent[]): Block[] => events.reduce<Block[]>((blocks, event) => applyAgentEvent(blocks, event).blocks, []);

const spawn: AgentEvent = { kind: 'tool_call', id: 'agent1', name: 'Agent', input: 'scan', change: null, background: true };

test('a sub-agent\'s steps and words stay under its call and never join the main answer', () => {
  const blocks = run([
    spawn,
    { kind: 'scoped', parent: 'agent1', event: { kind: 'tool_call', id: 'b1', name: 'Bash', input: 'ls', change: null, background: false } },
    { kind: 'scoped', parent: 'agent1', event: { kind: 'delta', text: 'found 3 files' } },
    { kind: 'delta', text: 'Started the scan.' },
  ]);
  assert.equal(blocks.length, 2);
  const call = blocks[0];
  assert.equal(call.kind, 'command');
  if (call.kind !== 'command') return;
  assert.equal(call.children?.length, 2);
  assert.deepEqual(blocks[1], { kind: 'response', text: 'Started the scan.', streaming: true });
});

test('a task event labels the call that started it and keeps what it knew', () => {
  const blocks = run([
    spawn,
    { kind: 'task', id: 't1', tool_use_id: 'agent1', status: 'started', label: 'Explore: scan', summary: null, tools: null },
    { kind: 'task', id: 't1', tool_use_id: 'agent1', status: 'progress', label: null, summary: null, tools: 3 },
  ]);
  const call = blocks[0];
  assert.equal(call.kind === 'command' && call.task?.label, 'Explore: scan');
  assert.equal(call.kind === 'command' && call.task?.tools, 3);
});

test('a child whose call is unknown is dropped and the end of the turn settles the children too', () => {
  const lost = run([{ kind: 'scoped', parent: 'nobody', event: { kind: 'delta', text: 'x' } }]);
  assert.equal(lost.length, 0);
  const blocks = run([
    spawn,
    { kind: 'scoped', parent: 'agent1', event: { kind: 'tool_call', id: 'b1', name: 'Bash', input: 'sleep', change: null, background: false } },
    { kind: 'done', session_id: null, text: '', cost_usd: null },
  ]);
  const call = blocks[0];
  assert.equal(call.kind === 'command' && call.children?.[0].kind === 'command' && call.children[0].status, 'ok');
});

test('an auto turn marker draws nothing in the reply it arrives in', () => {
  assert.deepEqual(applyAgentEvent([], { kind: 'auto_turn', reason: 'x' }), { blocks: [], finished: false });
});
