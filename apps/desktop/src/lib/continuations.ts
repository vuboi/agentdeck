// A reply that continues the one before it (the agent answers after the work it handed out) is one message on screen:
// one header, one token count, one time. Only finished replies join; a reply still being written keeps its own message.

import type { Message } from './mock';
import type { Tokens } from './usage';

function sumTokens(a?: Tokens, b?: Tokens): Tokens | undefined {
  if (!a && !b) return undefined;
  return {
    input: (a?.input ?? 0) + (b?.input ?? 0),
    output: (a?.output ?? 0) + (b?.output ?? 0),
    cacheRead: (a?.cacheRead ?? 0) + (b?.cacheRead ?? 0),
    cacheWrite: (a?.cacheWrite ?? 0) + (b?.cacheWrite ?? 0),
  };
}

/** Joins each assistant reply to the finished assistant reply of the same agent right before it. */
export function joinContinuations(messages: Message[]): Message[] {
  const out: Message[] = [];
  for (const message of messages) {
    const prev = out.at(-1);
    if (prev && prev.role === 'assistant' && message.role === 'assistant' && !prev.live && !message.live && prev.agent === message.agent) {
      out[out.length - 1] = {
        ...prev,
        blocks: [...prev.blocks, ...message.blocks],
        tokens: sumTokens(prev.tokens, message.tokens),
        cost: prev.cost != null || message.cost != null ? (prev.cost ?? 0) + (message.cost ?? 0) : undefined,
        ms: prev.ms != null || message.ms != null ? (prev.ms ?? 0) + (message.ms ?? 0) : undefined,
        model: prev.model ?? message.model,
      };
    } else {
      out.push(message);
    }
  }
  return out;
}
