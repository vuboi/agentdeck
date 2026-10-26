import { test } from 'node:test';
import assert from 'node:assert/strict';
import { guessRowHeight, rememberRowHeight, rowHeight } from './row-size.ts';

test('a longer answer is guessed taller, a narrower list too', () => {
  const short = { id: 'a', role: 'assistant', blocks: [{ kind: 'response', text: 'Done.' }] };
  const long = { id: 'b', role: 'assistant', blocks: [{ kind: 'response', text: 'word '.repeat(400) }] };
  assert.ok(guessRowHeight(long, 900) > guessRowHeight(short, 900) * 4);
  assert.ok(guessRowHeight(long, 400) > guessRowHeight(long, 900));
});

test('code lines count one by one', () => {
  const code = { id: 'c', role: 'assistant', blocks: [{ kind: 'response', text: '```ts\n' + 'x();\n'.repeat(30) + '```' }] };
  assert.ok(guessRowHeight(code, 900) >= 30 * 20);
});

test('a measured height wins over the guess, except for a live reply', () => {
  const message = { id: 'm1', role: 'assistant', blocks: [{ kind: 'response', text: 'hi' }] };
  rememberRowHeight('m1', 900, 333);
  assert.equal(rowHeight(message, 905), 333);
  assert.notEqual(rowHeight({ ...message, live: true }, 900), 333);
});
