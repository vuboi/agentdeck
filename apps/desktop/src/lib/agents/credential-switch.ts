/** Serializes credential mutations for a shared native connector or custom provider. */
const lanes = new Map<string, Promise<unknown>>();
const rotations = new Map<string, Set<AbortController>>();

export const credentialBusy = (scope: string): boolean => lanes.has(scope);
export async function waitForCredential(scope: string): Promise<void> { await lanes.get(scope)?.catch(() => undefined); }

export async function exclusiveCredential<T>(scope: string, action: () => Promise<T>): Promise<T> {
  const previous = lanes.get(scope) ?? Promise.resolve();
  const next = previous.catch(() => undefined).then(action);
  lanes.set(scope, next);
  try { return await next; }
  finally { if (lanes.get(scope) === next) lanes.delete(scope); }
}

/** A manual selection takes priority over pending automatic rotations. */
export function cancelCredentialRotations(scope: string): void {
  for (const controller of rotations.get(scope) ?? []) controller.abort();
}

export type CredentialAttempt = { signal: AbortSignal; tried: Set<string>; model: string; originalId?: string; scope?: string; failedId?: string };

/** Keeps automatic attempts cancellable while queued and while native calls are in flight. */
export async function rotateCredential<T>(scope: string, attempt: CredentialAttempt, action: (signal: AbortSignal) => Promise<T>): Promise<T> {
  const controller = new AbortController();
  const cancel = () => controller.abort();
  attempt.signal.addEventListener('abort', cancel, { once: true });
  if (attempt.signal.aborted) cancel();
  const active = rotations.get(scope) ?? new Set<AbortController>();
  active.add(controller);
  rotations.set(scope, active);
  try {
    return await exclusiveCredential(scope, () => {
      controller.signal.throwIfAborted();
      return action(controller.signal);
    });
  } finally {
    attempt.signal.removeEventListener('abort', cancel);
    active.delete(controller);
    if (!active.size) rotations.delete(scope);
  }
}

/** Waits before an attempt; cancellation releases the timer immediately. */
export function credentialDelay(signal: AbortSignal): Promise<void> {
  signal.throwIfAborted();
  return new Promise((resolve, reject) => {
    const cancel = () => { clearTimeout(timer); reject(new DOMException('Cancelled', 'AbortError')); };
    const timer = setTimeout(() => { signal.removeEventListener('abort', cancel); resolve(); }, 5000 + Math.floor(Math.random() * 5001));
    signal.addEventListener('abort', cancel, { once: true });
  });
}
