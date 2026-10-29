import { test } from 'node:test';
import assert from 'node:assert/strict';
import { joinContinuations } from './continuations.ts';
import type { Message } from '../mock.ts';

const reply = (id: string, replyTo?: string): Message => ({
  id,
  role: 'assistant',
  time: '12:00',
  agent: 'claude',
  blocks: [{ kind: 'response', text: id }],
  ...(replyTo ? { replyTo } : {}),
});

test('keeps replies to explicit user messages as separate messages', () => {
  const joined = joinContinuations([
    reply('answer-1', 'user-1'),
    reply('answer-2', 'user-2'),
  ]);

  assert.deepEqual(joined.map((message) => [message.id, message.replyTo]), [
    ['answer-1', 'user-1'],
    ['answer-2', 'user-2'],
  ]);
});

test('still joins adjacent implicit hand-back replies', () => {
  const joined = joinContinuations([reply('main'), reply('hand-back')]);

  assert.equal(joined.length, 1);
  assert.deepEqual(joined[0].blocks, [
    { kind: 'response', text: 'main' },
    { kind: 'response', text: 'hand-back' },
  ]);
});

test('keeps automatic background answers as their own reply', () => {
  const automatic = { ...reply('background-result'), autoTurn: true };
  const joined = joinContinuations([reply('first-answer'), automatic]);

  assert.equal(joined.length, 2);
  assert.equal(joined[1].autoTurn, true);
});
