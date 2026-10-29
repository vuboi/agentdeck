/** How long the steps before a CLI starts (saving a picture, reading a command, starting the process) may take. */
export const PREPARE_LIMIT_MS = 30_000;

/**
 * `work`, but not for longer than `ms`. A call that never answers (a hung invoke, a file read that blocks) would leave a
 * chat on "Thinking" for ever, because no event ever ends it. With `fallback` the late answer is replaced by its
 * value; without one the wait fails with a message that says what was being waited for.
 */
export function within<T>(work: Promise<T>, ms: number, what: string, fallback?: () => T): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const timer = setTimeout(() => {
      if (fallback) resolve(fallback());
      else reject(new Error(`Timed out after ${Math.round(ms / 1000)} s ${what}.`));
    }, ms);
    work.then(
      (value) => {
        clearTimeout(timer);
        resolve(value);
      },
      (error: unknown) => {
        clearTimeout(timer);
        reject(error);
      },
    );
  });
}
