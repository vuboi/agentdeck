/** Execution lanes sharing one visible chat transcript. */
export type RunLane = {
  id: string;
  role: 'primary' | 'listener';
  state: 'running' | 'waiting' | 'done' | 'failed' | 'stopped';
  /** The provider explicitly reported child/background work that can outlive the current assistant step. */
  backgroundWork: boolean;
};

export type MessageDelivery = 'send' | 'queue' | 'listener';

/** Codex's wait call repeats the task count already presented by provider-native Agent activity cards. */
export function isRedundantAgentWait(command: string, hasAgentTasks: boolean): boolean {
  return hasAgentTasks && /^wait\s+\d+\s+agent\(s\)/i.test(command.trimStart());
}

/** Choose the route for a new message without treating ordinary tool execution as an idle wait. */
export function deliveryFor(lanes: readonly RunLane[]): MessageDelivery {
  const active = lanes.filter((lane) => lane.state === 'running' || lane.state === 'waiting');
  if (active.length === 0) return 'send';

  const listenerActive = active.some((lane) => lane.role === 'listener');
  const hasBackgroundWait = active.some(
    (lane) => lane.role === 'primary' && (lane.state === 'waiting' || lane.backgroundWork),
  );
  if (hasBackgroundWait && !listenerActive) return 'listener';
  return 'queue';
}
