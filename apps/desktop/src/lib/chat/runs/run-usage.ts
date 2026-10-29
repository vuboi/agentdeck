// The tokens and cost of a whole run of replies: a message of the user (sent while the agent was still working) cuts a reply
// short, and the chat draws the replies after it as one. The numbers on the name line add them all up, the cut-off ones too.
import type { Tokens } from './usage.ts';

type Reply = { role: string; agent?: string; model?: string | null; tokens?: Tokens; cost?: number; runGroup?: string };

export type RunUsage = { tokens: Tokens; cost: number | null; /** Every reply had its real cost (none was estimated). */ exact: boolean; replies: number };

const same = (a: Reply, b: Reply) => (a.agent ?? '') === (b.agent ?? '') && a.model === b.model;

/** Adds up replies adjacent to `at`, or every reply in its continuation group when one is set. */
export function runUsage(list: readonly Reply[], at: number, estimate: (tokens: Tokens, model: string | undefined) => number | null): RunUsage | undefined {
  const owner = list[at];
  if (!owner || owner.role !== 'assistant') return undefined;
  const run = owner.runGroup
    ? list.flatMap((reply, index) =>
        reply.role === 'assistant' && reply.runGroup === owner.runGroup && same(reply, owner) ? [index] : [],
      )
    : (() => {
        let from = at;
        while (from > 0 && list[from - 1].role === 'assistant' && same(list[from - 1], owner)) from--;
        let to = at;
        while (to < list.length - 1 && list[to + 1].role === 'assistant' && same(list[to + 1], owner)) to++;
        return Array.from({ length: to - from + 1 }, (_, offset) => from + offset);
      })();
  const tokens: Tokens = { input: 0, cacheRead: 0, cacheWrite: 0, output: 0 };
  let cost = 0;
  let known = false;
  let exact = true;
  let replies = 0;
  for (const index of run) {
    const reply = list[index];
    if (!reply.tokens) continue;
    replies++;
    tokens.input += reply.tokens.input;
    tokens.cacheRead += reply.tokens.cacheRead;
    tokens.cacheWrite += reply.tokens.cacheWrite;
    tokens.output += reply.tokens.output;
    const own = reply.cost ?? estimate(reply.tokens, reply.model ?? undefined);
    if (own == null) continue;
    known = true;
    cost += own;
    if (reply.cost == null) exact = false;
  }
  return replies ? { tokens, cost: known ? cost : null, exact, replies } : undefined;
}
