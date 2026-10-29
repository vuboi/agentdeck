// Pure reducer: one agent event applied to the blocks of the reply it belongs to.
// Kept free of stores and Tauri so it can be unit-tested with plain node.
//
// A block is "streaming" while the CLI is still writing it: only the last thinking or response block can be.
// Starting any other block, or ending the turn, closes it. A command stays "running" until the turn ends.

import type { Block, Message } from './mock';
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
    if (endTurn && block.kind === 'command' && block.children) close(block.children, true, endStatus);
  }
}

/** The tool call `id`, at any depth: a sub-agent's own calls can start sub-agents. */
function findCommand(blocks: Block[], id: string): Extract<Block, { kind: 'command' }> | undefined {
  for (const block of blocks) {
    if (block.kind !== 'command') continue;
    if (block.id === id) return block;
    const inner = block.children ? findCommand(block.children, id) : undefined;
    if (inner) return inner;
  }
  return undefined;
}

export function applyAgentEvent(current: Block[], event: AgentEvent): ReplyUpdate {
  // The loading placeholder goes as soon as anything real arrives. Otherwise the list is changed in place: copying it
  // for every delta of a long reply, and assigning the copy back, redrew every block of the reply for each token.
  const blocks = current.some((block) => block.kind === 'loading') ? current.filter((block) => block.kind !== 'loading') : current;
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
        ...(event.change ? { change: { ...event.change, create: /^(Write|write_file|writefile|create_file|createfile)$/i.test(event.name) && event.change.removed === 0 } } : {}),
      });
      return { blocks, finished: false };
    case 'tool_result': {
      const call = blocks.find((block) => block.kind === 'command' && block.id === event.id);
      if (call?.kind === 'command') {
        call.output = event.output.length > 6000 ? `${event.output.slice(0, 6000)}…` : event.output;
        call.status = event.is_error ? 'fail' : 'ok';
        if (event.image) call.image = event.image;
        if (event.change && !event.is_error) {
          const writes = /^(Write|write_file|writefile|create_file|createfile)$/i.test(call.command.trim().split(/\s+/, 1)[0] ?? '');
          call.change = { ...event.change, create: writes && event.change.removed === 0 };
        }
        // A Write over an existing file is an edit, not a creation: the tool's own message says which.
        if (call.change?.create && /updated|overwr/i.test(event.output)) call.change.create = false;
      }
      return { blocks, finished: false };
    }
    case 'tool_output_delta': {
      const call = blocks.find((block) => block.kind === 'command' && block.id === event.id);
      if (call?.kind === 'command') {
        call.output = `${call.output}${event.delta}`.slice(-6000);
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
    case 'scoped': {
      // Child work goes under the call that started it. A child whose call is not here (yet) is dropped, not drawn as
      // part of the main answer.
      const parent = findCommand(blocks, event.parent);
      if (parent) parent.children = applyAgentEvent(parent.children ?? [], event.event).blocks;
      return { blocks, finished: false };
    }
    case 'task': {
      const call = event.tool_use_id ? findCommand(blocks, event.tool_use_id) : undefined;
      if (call) {
        call.task = {
          status: event.status,
          label: event.label ?? call.task?.label,
          summary: event.summary ?? call.task?.summary,
          tools: event.tools ?? call.task?.tools,
        };
      }
      return { blocks, finished: false };
    }
    case 'auto_turn':
      // Starts a reply of its own (see sessions.svelte.ts); nothing to draw in this one.
      return { blocks: current, finished: false };
    case 'usage':
      // Counted on the message, not drawn as a block (see sessions.svelte.ts).
      return { blocks: current, finished: false };
    case 'error':
      close(blocks, true, 'fail');
      // Stopping a run yourself is not a failure.
      if (event.message !== 'stopped') blocks.push({ kind: 'response', text: `⚠ ${event.message}` });
      return { blocks, finished: true };
    case 'raw':
      // Only for the Audit log: the chat shows nothing of it.
      return { blocks: current, finished: false };
  }
}

/** Counts a `usage` event on the reply: a new model call adds its input, the output grows to the call's latest total. */
export function addUsage(reply: Message, event: Extract<AgentEvent, { kind: 'usage' }>) {
  reply.tokens ??= { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 };
  const tokens = reply.tokens;
  if (event.new_call) {
    tokens.input += event.input_tokens;
    tokens.cacheRead += event.cache_read_tokens;
    tokens.cacheWrite += event.cache_write_tokens;
    reply.context = event.input_tokens + event.cache_read_tokens + event.cache_write_tokens;
    reply.callOutput = 0;
  }
  tokens.output += Math.max(0, event.output_tokens - (reply.callOutput ?? 0));
  reply.callOutput = event.output_tokens;
}
