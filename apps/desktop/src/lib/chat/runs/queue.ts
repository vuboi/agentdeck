/** Add a message waiting behind the current reply and replace the array for reactive consumers. */
export function enqueue<T>(
    queue: Record<string, T[]>,
    sessionId: string,
    message: T,
) {
    queue[sessionId] = [...(queue[sessionId] ?? []), message];
}

/** Add a message to the front of the queue without interrupting the turn already running. */
export function enqueueNext<T>(
    queue: Record<string, T[]>,
    sessionId: string,
    message: T,
) {
    queue[sessionId] = [message, ...(queue[sessionId] ?? [])];
}

/** Take every waiting message at once, oldest first, leaving the queue empty. */
export function dequeueAll<T>(
    queue: Record<string, T[]>,
    sessionId: string,
): T[] {
    const waiting = queue[sessionId] ?? [];
    delete queue[sessionId];
    return waiting;
}

/** Take the next waiting message without mutating the array held by a reactive consumer. */
export function dequeue<T>(
    queue: Record<string, T[]>,
    sessionId: string,
): T | undefined {
    const waiting = queue[sessionId];
    if (!waiting?.length) return undefined;
    const [next, ...rest] = waiting;
    if (rest.length) queue[sessionId] = rest;
    else delete queue[sessionId];
    return next;
}
