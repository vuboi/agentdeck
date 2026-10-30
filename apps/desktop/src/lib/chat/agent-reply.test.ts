import test from 'node:test';
import assert from 'node:assert/strict';
import { applyAgentEvent, updateHistoricalTask } from './agent-reply.ts';
import type { AgentEvent } from './agent';
import type { Block } from './mock';
import type { Message } from './mock';

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

test('a provider session id is captured before the final Done event', () => {
  assert.deepEqual(applyAgentEvent([], { kind: 'session', session_id: 'provider-session-1' }), {
    blocks: [],
    finished: false,
    agentSession: 'provider-session-1',
  });
});

test('a task completing in a later turn closes its original card and ignores late progress', () => {
  const messages: Message[] = [
    { id: 'old', role: 'assistant', time: '', blocks: run([spawn]) },
    { id: 'new', role: 'assistant', time: '', blocks: [{ kind: 'response', text: 'New answer' }] },
  ];
  const event: Extract<AgentEvent, { kind: 'task' }> = { kind: 'task', id: 'task-1', tool_use_id: 'agent1', status: 'done', label: 'Scan', summary: 'Finished', tools: 3 };
  assert.equal(updateHistoricalTask(messages, event), true);
  const call = messages[0].blocks[0];
  assert.equal(call.kind === 'command' && call.task?.status, 'done');
  updateHistoricalTask(messages, { ...event, status: 'progress' });
  assert.equal(call.kind === 'command' && call.task?.status, 'done');
  assert.deepEqual(messages[1].blocks, [{ kind: 'response', text: 'New answer' }]);
  assert.equal(updateHistoricalTask(messages, { ...event, tool_use_id: 'unknown' }), false);
});

test('a foreground Bash that the CLI also reports as a task stays an ordinary command row', () => {
  const foreground = run([
    { kind: 'tool_call', id: 'fg', name: 'Bash', input: 'cargo test', background: false },
    { kind: 'task', id: 'bash-1', tool_use_id: 'fg', status: 'started', label: 'Run tests' },
  ] as AgentEvent[]);
  const call = foreground.find((block) => block.kind === 'command');
  assert.equal(call?.kind === 'command' && call.task, undefined, 'no task row for a foreground call');
  const background = run([
    { kind: 'tool_call', id: 'bg', name: 'Bash', input: 'pnpm dev', background: true },
    { kind: 'task', id: 'bash-2', tool_use_id: 'bg', status: 'started', label: 'Dev server' },
  ] as AgentEvent[]);
  const shell = background.find((block) => block.kind === 'command');
  assert.equal(shell?.kind === 'command' && shell.task?.status, 'started', 'a background call takes its task');
});
