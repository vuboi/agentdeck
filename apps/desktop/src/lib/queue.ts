/** Add a message waiting behind the current reply and replace the array for reactive consumers. */
export function enqueue<T>(queue: Record<string, T[]>, sessionId: string, message: T) {
  queue[sessionId] = [...(queue[sessionId] ?? []), message];
}

/** Take the next waiting message without mutating the array held by a reactive consumer. */
export function dequeue<T>(queue: Record<string, T[]>, sessionId: string): T | undefined {
  const waiting = queue[sessionId];
  if (!waiting?.length) return undefined;
  const [next, ...rest] = waiting;
  if (rest.length) queue[sessionId] = rest;
  else delete queue[sessionId];
  return next;
}
