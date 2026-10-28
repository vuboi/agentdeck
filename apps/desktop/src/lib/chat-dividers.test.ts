import assert from 'node:assert/strict';
import test from 'node:test';
import { dividerIndexes, hollow, markerText, neighbour, stepsOnly, sessionMark, turnDivider, type SessionMark } from './chat-dividers.ts';

test('a reply of steps only is drawn as its step line, not as a named reply', () => {
  const steps = { role: 'assistant' as const, blocks: [{ kind: 'command' }] };
  assert.equal(stepsOnly(steps), true);
  assert.equal(stepsOnly({ ...steps, blocks: [{ kind: 'command' }, { kind: 'response', text: 'done' }] }), false);
  assert.equal(stepsOnly({ ...steps, live: true }), false);
  assert.equal(stepsOnly({ ...steps, stoppedAfter: 500 }), false);
  assert.equal(stepsOnly({ role: 'assistant', blocks: [] }), false);
});

test('a reply cut off before it wrote anything is hollow, and its neighbours skip it', () => {
  const empty = { role: 'assistant' as const, blocks: [] };
  const loading = { role: 'assistant' as const, blocks: [{ kind: 'loading' }, { kind: 'thinking', text: ' ' }] };
  const real = { role: 'assistant' as const, blocks: [{ kind: 'response', text: 'hi' }] };
  assert.equal(hollow(empty), true);
  assert.equal(hollow(loading), true);
  assert.equal(hollow(real), false);
  assert.equal(hollow({ ...empty, live: true }), false);
  assert.equal(hollow({ ...empty, stoppedAfter: 1200 }), false);
  assert.equal(hollow({ ...empty, stoppedAfter: 1200 }, true), true, 'an old stopped note on an empty reply with a newer message after it');
  assert.equal(hollow({ role: 'user', blocks: [] }), false);
  const list = [{ role: 'user' as const, blocks: [{ kind: 'response', text: 'a' }] }, empty, { role: 'user' as const, blocks: [{ kind: 'response', text: 'b' }] }, real];
  assert.equal(neighbour(list, 3, -1), list[2]);
  assert.equal(neighbour(list, 0, 1), list[2]);
  assert.equal(neighbour(list, 3, 1), undefined);
});

const user = { role: 'user' as const };
const agent = { role: 'assistant' as const };
const input = { resume: false, changed: false, compacting: false, firstThisRun: false, hasHistory: true, now: new Date('2026-10-28T07:32:00Z') };

test('a line goes between two replies of the agent with no message of the user between them', () => {
  // user, agent, agent, user, agent: the line is above the second reply only.
  assert.deepEqual(dividerIndexes([user, agent, agent, user, agent], true), [2]);
});

test('no line next to a message of the user, and none above the first message', () => {
  assert.equal(turnDivider(user, agent, true), false);
  assert.equal(turnDivider(agent, user, true), false);
  assert.equal(turnDivider(undefined, agent, true), false);
  assert.deepEqual(dividerIndexes([agent], true), []);
});

test('one reply with text, a tool call and text again is one message: no line inside it', () => {
  // The blocks of a reply are not messages: a list with that reply alone has no line, and turning blocks into more
  // text changes nothing, because the decision reads roles only.
  const reply = { role: 'assistant' as const, blocks: [{ kind: 'response' }, { kind: 'command' }, { kind: 'response' }] };
  assert.deepEqual(dividerIndexes([user, reply], true), []);
});

test('the decision does not depend on the text, so a streaming reply keeps one line and does not blink', () => {
  const growing = { role: 'assistant' as const, live: true, blocks: [{ text: 'a' }] };
  const before = dividerIndexes([user, agent, growing], true);
  growing.blocks[0].text += ' more text arrives';
  growing.live = false;
  assert.deepEqual(dividerIndexes([user, agent, growing], true), before);
  // Asked row by row, as the chat does, it is the same single line.
  assert.equal(turnDivider(agent, growing, true), true);
  assert.equal(dividerIndexes([user, agent, growing], true).length, 1);
});

test('the setting turns every line off', () => {
  assert.deepEqual(dividerIndexes([user, agent, agent, agent], false), []);
  assert.equal(turnDivider(agent, agent, false), false);
});

test('a message that carries a session line has no second line above it', () => {
  const mark: SessionMark = { kind: 'resumed', at: '2026-10-28T07:32:00Z' };
  assert.equal(turnDivider(agent, { role: 'assistant', mark }, true), false);
  assert.deepEqual(dividerIndexes([agent, { role: 'assistant', mark }, agent], true), [2]);
});

test('the session line names what happened and when', () => {
  const at = new Date(2026, 9, 28, 14, 32).toISOString();
  assert.equal(markerText({ kind: 'resumed', at }), 'Session resumed · 28/10/2026 14:32');
  assert.equal(markerText({ kind: 'compacted', at }), 'Context compacted · 28/10/2026 14:32');
  assert.equal(markerText({ kind: 'new', at }), 'New session · 28/10/2026 14:32');
  assert.equal(markerText({ kind: 'new', at: 'not a date' }), 'New session');
});

test('which turn gets a session line', () => {
  // A compaction always does; a changed conversation and a first resume of an old chat do when the chat has history.
  assert.equal(sessionMark({ ...input, compacting: true })?.kind, 'compacted');
  assert.equal(sessionMark({ ...input, changed: true })?.kind, 'new');
  assert.equal(sessionMark({ ...input, resume: true, firstThisRun: true })?.kind, 'resumed');
  // Nothing to mark: a later turn of a running chat, or the first message of a new chat.
  assert.equal(sessionMark({ ...input, resume: true, firstThisRun: false }), undefined);
  assert.equal(sessionMark({ ...input, changed: true, hasHistory: false }), undefined);
  assert.equal(sessionMark({ ...input, resume: true, firstThisRun: true, hasHistory: false }), undefined);
  // A new conversation wins over a resume in the same turn.
  assert.equal(sessionMark({ ...input, changed: true, resume: true, firstThisRun: true })?.kind, 'new');
});

test('a chat of thousands of messages is decided in one quick pass', () => {
  const list = Array.from({ length: 5000 }, (_, index) => (index % 7 === 0 ? user : agent));
  const started = performance.now();
  const lines = dividerIndexes(list, true);
  const took = performance.now() - started;
  assert.ok(lines.length > 0);
  assert.ok(took < 50, `took ${took} ms`);
});
