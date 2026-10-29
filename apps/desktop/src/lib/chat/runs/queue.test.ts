import { test } from 'node:test';
import assert from 'node:assert/strict';
import { dequeue, enqueue, enqueueNext } from './queue.ts';

type Queued = { text: string };

test('queued messages stay visible in order until each one is sent', () => {
    const queue: Record<string, Queued[]> = {};
    enqueue(queue, 'session-1', { text: 'second' });
    enqueue(queue, 'session-1', { text: 'third' });

    assert.deepEqual(queue['session-1'], [
        { text: 'second' },
        { text: 'third' },
    ]);
    assert.deepEqual(dequeue(queue, 'session-1'), { text: 'second' });
    assert.deepEqual(queue['session-1'], [{ text: 'third' }]);
    assert.deepEqual(dequeue(queue, 'session-1'), { text: 'third' });
    assert.equal(queue['session-1'], undefined);
});

test('a next message goes ahead of queued messages without discarding them', () => {
    const queue: Record<string, Queued[]> = {};
    enqueue(queue, 'session-1', { text: 'later' });
    enqueueNext(queue, 'session-1', { text: 'next' });

    assert.deepEqual(dequeue(queue, 'session-1'), { text: 'next' });
    assert.deepEqual(dequeue(queue, 'session-1'), { text: 'later' });
    assert.equal(queue['session-1'], undefined);
});

test('queued message identity survives priority reordering and dequeue', () => {
    const queue: Record<string, { messageId: string; text: string }[]> = {};
    enqueue(queue, 'session-1', { messageId: 'user-later', text: 'later' });
    enqueueNext(queue, 'session-1', { messageId: 'user-next', text: 'next' });

    assert.equal(dequeue(queue, 'session-1')?.messageId, 'user-next');
    assert.equal(dequeue(queue, 'session-1')?.messageId, 'user-later');
});
