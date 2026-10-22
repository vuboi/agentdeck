import { test } from 'node:test';
import assert from 'node:assert/strict';
import { dequeue, enqueue } from './queue.ts';

type Queued = { text: string };

test('queued messages stay visible in order until each one is sent', () => {
  const queue: Record<string, Queued[]> = {};
  enqueue(queue, 'session-1', { text: 'second' });
  enqueue(queue, 'session-1', { text: 'third' });

  assert.deepEqual(queue['session-1'], [{ text: 'second' }, { text: 'third' }]);
  assert.deepEqual(dequeue(queue, 'session-1'), { text: 'second' });
  assert.deepEqual(queue['session-1'], [{ text: 'third' }]);
  assert.deepEqual(dequeue(queue, 'session-1'), { text: 'third' });
  assert.equal(queue['session-1'], undefined);
});
