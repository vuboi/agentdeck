// How well each model does the work it is given, from what the app already keeps: every finished reply (usage log) and
// every delegated task. Free of stores so plain node can test it.
//
// A delegated task runs in a chat of its own, and every reply in that chat is one round: the first answer, then each
// correction the caller sent back. A model that needs three or four rounds for one task is a worse choice than one that
// gets it right the first time, whatever it costs per reply, so rounds are the headline number.

/** The part of a usage record the scorecard reads. */
export type ScoredReply = { sessionId: string; model?: string; ms?: number };
/** The part of a delegated task the scorecard reads. `childId` is the worker's chat. */
export type ScoredTask = { childId: string; state: string; startedAt: number; endedAt?: number };

/** From this many rounds on, a task counts as rework. */
export const REWORK_ROUNDS = 3;

export type ScoreRow = {
  model: string;
  /** All replies of the model in the range, in any chat. */
  replies: number;
  /** Mean time of a reply (ms), over the replies that have one. */
  avgReplyMs: number;
  /** Delegated tasks the model ran, and how they ended. */
  tasks: number;
  done: number;
  failed: number;
  /** Mean rounds per task, and how the tasks split: right the first time, one correction, rework (`REWORK_ROUNDS` or more). */
  avgRounds: number;
  firstTry: number;
  oneFix: number;
  rework: number;
  /** Mean time from the task's start to its end (ms), over the tasks that ended. */
  avgTaskMs: number;
  /** Cost of all its replies in the range (USD), over the replies that have a price; and per task. */
  cost: number;
  costPerTask: number | null;
};

const mean = (values: number[]) => (values.length ? values.reduce((sum, value) => sum + value, 0) / values.length : 0);

/** One row per model, the model with the most rework first (then by tasks). `costOf` gives a reply's cost, or null. */
export function scorecard<T extends ScoredReply>(replies: T[], tasks: ScoredTask[], costOf: (reply: T) => number | null): ScoreRow[] {
  const byChat = new Map<string, T[]>();
  for (const reply of replies) byChat.set(reply.sessionId, [...(byChat.get(reply.sessionId) ?? []), reply]);

  const rows = new Map<string, { replies: T[]; rounds: number[]; ends: number[]; done: number; failed: number }>();
  const row = (model: string) => {
    if (!rows.has(model)) rows.set(model, { replies: [], rounds: [], ends: [], done: 0, failed: 0 });
    return rows.get(model)!;
  };
  for (const reply of replies) row(reply.model ?? '').replies.push(reply);
  for (const task of tasks) {
    const chat = byChat.get(task.childId);
    // A task whose chat has no reply yet (queued, or still on its first answer) says nothing about its model.
    if (!chat?.length) continue;
    const entry = row(chat[0].model ?? '');
    entry.rounds.push(chat.length);
    if (task.state === 'done') entry.done++;
    else if (task.state === 'failed' || task.state === 'stopped') entry.failed++;
    if (task.endedAt) entry.ends.push(task.endedAt - task.startedAt);
  }

  return [...rows.entries()]
    .map(([model, entry]): ScoreRow => {
      const priced = entry.replies.map(costOf).filter((value): value is number => value !== null);
      const cost = priced.reduce((sum, value) => sum + value, 0);
      return {
        model,
        replies: entry.replies.length,
        avgReplyMs: mean(entry.replies.map((reply) => reply.ms ?? 0).filter(Boolean)),
        tasks: entry.rounds.length,
        done: entry.done,
        failed: entry.failed,
        avgRounds: mean(entry.rounds),
        firstTry: entry.rounds.filter((rounds) => rounds === 1).length,
        oneFix: entry.rounds.filter((rounds) => rounds === 2).length,
        rework: entry.rounds.filter((rounds) => rounds >= REWORK_ROUNDS).length,
        avgTaskMs: mean(entry.ends),
        cost,
        costPerTask: entry.rounds.length && priced.length ? cost / entry.rounds.length : null,
      };
    })
    .sort((a, b) => b.rework - a.rework || b.tasks - a.tasks || b.replies - a.replies);
}
