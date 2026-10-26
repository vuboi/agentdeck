// Time left until a usage limit resets, shown as a live countdown (account switcher, Settings). Free of stores so plain
// node can test it.

/**
 * The provider reports a window's reset time again on every usage refresh, a few seconds off each time, so a countdown
 * read straight from it went 58, 57, 58. The first time seen is kept per window while new reports stay within
 * `JITTER_S` of it; a real change (the window rolled over) is taken at once.
 */
const JITTER_S = 90;
const steady = new Map<string, number>();
export function steadyReset(key: string, resetsAt: number): number {
  const kept = steady.get(key);
  if (kept !== undefined && Math.abs(kept - resetsAt) <= JITTER_S) return kept;
  steady.set(key, resetsAt);
  return resetsAt;
}

/** Time left until `resetsAt` (Unix seconds) as a live countdown: `2d 3h 04m 05s`, `3h 04m 05s`, `4m 05s`, `5s`. */
export function resetCountdown(resetsAt: number, nowMs: number): string {
  let left = Math.max(0, Math.floor(resetsAt - nowMs / 1000));
  const days = Math.floor(left / 86_400);
  left -= days * 86_400;
  const hours = Math.floor(left / 3600);
  left -= hours * 3600;
  const minutes = Math.floor(left / 60);
  const seconds = left - minutes * 60;
  const two = (value: number) => String(value).padStart(2, '0');
  if (days) return `${days}d ${hours}h ${two(minutes)}m ${two(seconds)}s`;
  if (hours) return `${hours}h ${two(minutes)}m ${two(seconds)}s`;
  if (minutes) return `${minutes}m ${two(seconds)}s`;
  return `${seconds}s`;
}
