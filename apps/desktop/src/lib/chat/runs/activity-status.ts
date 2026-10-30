// The status of a tool call when its result arrives. Free of stores and Tauri so plain node can test it.

import type { Activity, ActivityStatus } from './activity.svelte';

/**
 * Whether the provider's own task event already ended this call. The CLI reports some foreground calls (a Bash) as
 * tasks too, and that event can arrive before the call's tool result: the call is then done and recorded.
 */
export function endedByTask(item: Pick<Activity, 'status' | 'nativeTaskId' | 'endedAt'>): boolean {
  return item.nativeTaskId !== undefined && item.endedAt !== undefined && item.status !== 'running' && item.status !== 'background';
}

/**
 * The status a call takes from its tool result. A call its task event already ended keeps that final status: the
 * result only adds output. Otherwise an error is 'fail', a background call (a shell that answers with its id) stays
 * 'background' until its task ends, and any other call is 'ok'.
 */
export function statusAfterResult(item: Pick<Activity, 'background' | 'status' | 'nativeTaskId' | 'endedAt'>, isError: boolean): ActivityStatus {
  if (endedByTask(item)) return item.status;
  if (isError) return 'fail';
  return item.background ? 'background' : 'ok';
}
