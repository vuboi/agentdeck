// Waiting for typing to stop before acting on it: a search box filters (or asks Rust) once, not on every key.

/** Default pause after the last key, in ms. */
export const DEBOUNCE_MS = 250;

/** `fn`, run only once calls have stopped for `ms`; `cancel` drops a pending run. */
export function debounce<A extends unknown[]>(fn: (...args: A) => void, ms = DEBOUNCE_MS): ((...args: A) => void) & { cancel: () => void } {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const run = (...args: A) => {
    clearTimeout(timer);
    timer = setTimeout(() => fn(...args), ms);
  };
  return Object.assign(run, { cancel: () => clearTimeout(timer) });
}

/**
 * A value that follows `read()` once it has stopped changing for `ms`: `debounced(() => query).value`. Clearing it
 * (an empty string) applies at once, so a cleared search shows everything without waiting. Call it while a component
 * is set up (it uses `$effect`).
 */
export function debounced<T>(read: () => T, ms = DEBOUNCE_MS): { readonly value: T } {
  let value = $state(read());
  $effect(() => {
    const next = read();
    if (next === '' || next === value) {
      value = next;
      return;
    }
    const timer = setTimeout(() => (value = next), ms);
    return () => clearTimeout(timer);
  });
  return {
    get value() {
      return value;
    },
  };
}
