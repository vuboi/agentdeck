// Pure reducer: one agent event applied to the blocks of the reply it belongs to.
// Kept free of stores and Tauri so it can be unit-tested with plain node.
//
// A block is "streaming" while the CLI is still writing it: only the last thinking or response block can be.
// Starting any other block, or ending the turn, closes it. A command stays "running" until the turn ends.

import type { Block } from './mock';
import type { AgentEvent } from './agent';

export type ReplyUpdate = {
  blocks: Block[];
  /** The turn is over (done or error): stop waiting, clear the live reply. */
  finished: boolean;
  /** The CLI's conversation id, when the turn reports one. */
  agentSession?: string;
};

/** Stop every streaming block; with `endTurn`, also settle running commands. */
function close(blocks: Block[], endTurn: boolean, endStatus: 'ok' | 'fail' = 'ok') {
  for (const block of blocks) {
    if (block.kind === 'thinking' || block.kind === 'response') block.streaming = false;
    if (endTurn && block.kind === 'command' && block.status === 'running') block.status = endStatus;
  }
}

export function applyAgentEvent(current: Block[], event: AgentEvent): ReplyUpdate {
  // The loading placeholder goes as soon as anything real arrives.
  const blocks = current.filter((block) => block.kind !== 'loading');
  switch (event.kind) {
    case 'delta': {
      const last = blocks.at(-1);
      if (last?.kind === 'response' && last.streaming) {
        last.text += event.text;
      } else {
        close(blocks, false);
        blocks.push({ kind: 'response', text: event.text, streaming: true });
      }
      return { blocks, finished: false };
    }
    case 'thinking': {
      const last = blocks.at(-1);
      if (last?.kind === 'thinking' && last.streaming) {
        last.text += event.text;
      } else {
        close(blocks, false);
        blocks.push({ kind: 'thinking', text: event.text, streaming: true });
      }
      return { blocks, finished: false };
    }
    case 'tool_call':
      close(blocks, false);
      blocks.push({
        kind: 'command',
        id: event.id,
        command: `${event.name}  ${event.input}`.trim(),
        output: '',
        status: 'running',
        ...(event.change ? { change: { ...event.change, create: event.name === 'Write' && event.change.removed === 0 } } : {}),
      });
      return { blocks, finished: false };
    case 'tool_result': {
      const call = blocks.find((block) => block.kind === 'command' && block.id === event.id);
      if (call?.kind === 'command') {
        call.output = event.output.length > 6000 ? `${event.output.slice(0, 6000)}…` : event.output;
        call.status = event.is_error ? 'fail' : 'ok';
        // A Write over an existing file is an edit, not a creation: the tool's own message says which.
        if (call.change?.create && /updated|overwr/i.test(event.output)) call.change.create = false;
      }
      return { blocks, finished: false };
    }
    case 'permission_denied':
      close(blocks, false);
      blocks.push({ kind: 'permission', requests: event.requests, state: 'pending' });
      return { blocks, finished: false };
    case 'done': {
      close(blocks, true);
      // Streamed text already shows the answer; the final result only fills in when nothing streamed.
      if (!blocks.some((block) => block.kind === 'response') && event.text) blocks.push({ kind: 'response', text: event.text });
      return { blocks, finished: true, agentSession: event.session_id ?? undefined };
    }
    case 'error':
      close(blocks, true, 'fail');
      blocks.push({ kind: 'response', text: `⚠ ${event.message}` });
      return { blocks, finished: true };
  }
}
