// Where the chat draws a thin line between two replies of the agent, and the line that says a session was resumed,
// compacted or replaced. Pure (no stores, no Tauri) so plain node can test it.
//
// A "reply" is one message of the agent (`role: 'assistant'`): the text, tool calls and text again of one turn are blocks
// of the same message, so they never get a line inside. A line goes between a reply and the next reply when no message of
// the user stands between them (the turn ended and the agent spoke again: a report of a background task, a recovery turn).
// A message of the user already separates what is above and below it, so it needs no line.

export type SessionMarkKind = 'resumed' | 'compacted' | 'new';
/** Said once, above the message that started the session. `at` is an ISO time. */
export type SessionMark = { kind: SessionMarkKind; at: string };

type Item = { role: 'user' | 'assistant'; mark?: SessionMark };

type Shown = Item & { live?: boolean; stoppedAfter?: number; blocks: { kind: string; text?: string }[] };

/** A reply with nothing to show (cut off before it wrote anything, because a newer message came in): it is not drawn, and the
 *  replies around it are neighbours as if it were not there. A live reply or one the user stopped is always drawn. */
export function hollow(message: Shown | undefined, later = false): boolean {
  // A stopped reply is drawn while it is the last message (the user just pressed Stop); once a newer message follows, an
  // empty one is only noise: chats saved before this carry the "stopped" note on replies a newer message cut off.
  if (!message || message.role !== 'assistant' || message.live || (message.stoppedAfter !== undefined && !later) || message.mark) return false;
  return !message.blocks.some((block) => block.kind !== 'loading' && (block.kind !== 'thinking' && block.kind !== 'response' ? true : Boolean(block.text?.trim())));
}

/** A finished reply that only ran steps (searched, read, edited) and wrote no text: cut off by a newer message, or ending on
 *  its tools. It is drawn as its step line alone, with no name line and no footer, as Claude does. */
export function stepsOnly(message: Shown | undefined): boolean {
  if (!message || message.role !== 'assistant' || message.live || message.stoppedAfter !== undefined || hollow(message, true)) return false;
  return !message.blocks.some((block) => block.kind === 'response' && Boolean(block.text?.trim()));
}

/** The nearest message before / after index `at` that is drawn (see `hollow`). */
export function neighbour<T extends Shown>(list: readonly T[], at: number, step: -1 | 1): T | undefined {
  for (let index = at + step; index >= 0 && index < list.length; index += step) if (!hollow(list[index], index < list.length - 1)) return list[index];
  return undefined;
}

/** Where a reply sits in a run of replies of the same agent and model that follow one another (no message of the user
 *  between, hollow ones skipped): the first carries the name line, the last the footer, and a reply that only ran steps is
 *  drawn as its step line alone (`quiet`). `fallbackAgent` is the chat's agent, for a reply that names none. */
export function runPosition<T extends Shown & { agent?: string; model?: string | null }>(
  list: readonly T[],
  at: number,
  fallbackAgent: string,
): { quiet: boolean; grouped: boolean; lastOfRun: boolean } {
  const message = list[at];
  const agentOf = (reply: T) => reply.agent ?? fallbackAgent;
  const joins = (other: T | undefined) =>
    other !== undefined && other.role === 'assistant' && agentOf(other) === agentOf(message) && other.model === message.model && !stepsOnly(other);
  const quiet = stepsOnly(message);
  return { quiet, grouped: joins(neighbour(list, at, -1)), lastOfRun: !quiet && !joins(neighbour(list, at, 1)) };
}

/** Whether `current` gets a line above it: it is a reply, the message right above it is a reply too, and nothing else marks it. */
export function turnDivider(previous: Item | undefined, current: Item | undefined, enabled: boolean): boolean {
  if (!enabled || !previous || !current) return false;
  if (current.mark) return false; // the session line stands in its place
  return previous.role === 'assistant' && current.role === 'assistant';
}

/** The indexes of a list that get a line above them. One pass; used by the tests, and equal to asking `turnDivider` per row. */
export function dividerIndexes(list: readonly Item[], enabled: boolean): number[] {
  const out: number[] = [];
  if (!enabled) return out;
  for (let index = 1; index < list.length; index++) if (turnDivider(list[index - 1], list[index], true)) out.push(index);
  return out;
}

const LABEL: Record<SessionMarkKind, string> = { resumed: 'Session resumed', compacted: 'Context compacted', new: 'New session' };
const pad = (value: number) => String(value).padStart(2, '0');

/** "Session resumed · 28/10/2026 14:32" (the viewer's local time). */
export function markerText(mark: SessionMark): string {
  const date = new Date(mark.at);
  const when = Number.isNaN(date.getTime()) ? '' : ` · ${pad(date.getDate())}/${pad(date.getMonth() + 1)}/${date.getFullYear()} ${pad(date.getHours())}:${pad(date.getMinutes())}`;
  return `${LABEL[mark.kind]}${when}`;
}

export type MarkInput = {
  /** The turn resumes the CLI's conversation of the chat. */
  resume: boolean;
  /** The turn starts another conversation than the chat had (a model, account or edit change). */
  changed: boolean;
  /** The turn is the `/compact` of the context. */
  compacting: boolean;
  /** The chat had no turn since the app started: what it resumes is a session from before. */
  firstThisRun: boolean;
  /** The chat has messages besides the one being sent. */
  hasHistory: boolean;
  now: Date;
};

/** The line the turn's first message carries, if any. A first message of a chat has none: nothing was resumed or replaced. */
export function sessionMark(input: MarkInput): SessionMark | undefined {
  const at = input.now.toISOString();
  if (input.compacting) return { kind: 'compacted', at };
  if (!input.hasHistory) return undefined;
  if (input.changed) return { kind: 'new', at };
  if (input.resume && input.firstThisRun) return { kind: 'resumed', at };
  return undefined;
}
