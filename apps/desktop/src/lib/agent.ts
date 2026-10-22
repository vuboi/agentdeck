// The UI side of the agent event stream. The Rust side (crates/adapters/src/agent) turns each CLI's output
// into these events; nothing here knows a CLI's format.

export type AgentEvent =
  | { kind: 'delta'; text: string }
  | { kind: 'thinking'; text: string }
  | { kind: 'tool_call'; id: string; name: string; input: string }
  | { kind: 'tool_result'; id: string; output: string; is_error: boolean }
  | { kind: 'done'; session_id: string | null; text: string; cost_usd: number | null }
  | { kind: 'error'; message: string };

export type AgentEnvelope = { session: string; event: AgentEvent };

/** Start one turn. Resolves as soon as the CLI is started; the answer arrives as `agent-event`s. */
export async function sendAgentTurn(args: {
  session: string;
  agent: string;
  prompt: string;
  agentSession: string | null;
  resume: boolean;
  model: string | null;
  cwd: string | null;
}): Promise<void> {
  const { invoke } = await import('@tauri-apps/api/core');
  await invoke('agent_send', args);
}

/** Subscribe to every agent event. Returns the unlisten function. */
export async function listenAgentEvents(onEvent: (envelope: AgentEnvelope) => void): Promise<() => void> {
  const { listen } = await import('@tauri-apps/api/event');
  return listen<AgentEnvelope>('agent-event', (event) => onEvent(event.payload));
}
