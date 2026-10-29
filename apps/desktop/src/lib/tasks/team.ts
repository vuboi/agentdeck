// A team run: a planner (a BA) splits the work, then members (frontend, backend…) build their parts side by side in
// their own chats and keep each other informed through a queue. A member ends a turn with lines the app reads:
//   SEND @<member>: <message>     the message waits in that member's queue and is its next turn's input
//   STATUS: done | waiting @<member> | working
// The queue and every member's state are saved (IndexedDB and TEAM.md in the run folder) as they change, so a reload
// or a failed turn loses nothing: the run carries on from the queue. Free of stores so plain node can test it.

export type TeamStatus = 'new' | 'working' | 'waiting' | 'done' | 'failed';

export type TeamMember = {
  stepId: string;
  name: string;
  childId: string;
  status: TeamStatus;
  turns: number;
  /** What the member is waiting for, when it said so. */
  waitingOn?: string;
  /** Its last answer, the hand-back's input. */
  last?: string;
  /** A turn was running when this was saved: a reload finds it cut off and starts it again. */
  busy?: boolean;
};

export type TeamMessage = { id: string; from: string; to: string; text: string; at: number; taken?: number };

export type TeamState = {
  id: string;
  main: string;
  /** The user's message the run answers (its cards). */
  askedId: string;
  task: string;
  root: string | null;
  folder: string;
  plan: string;
  members: TeamMember[];
  queue: TeamMessage[];
  log: string[];
  startedAt: number;
  endedAt?: number;
};

/** Turns one member may take in a run: enough for a long exchange, never a loop for ever. */
export const MAX_TEAM_TURNS = 14;

const nameKey = (name: string) => name.trim().toLowerCase();

/** The messages and the status a member's answer ends with. Messages to an unknown member or to itself are dropped. */
export function parseTeamReply(text: string, self: string, names: string[]) {
  const known = new Map(names.map((name) => [nameKey(name), name]));
  const sends: { to: string; text: string }[] = [];
  let status: TeamStatus = 'working';
  let waitingOn: string | undefined;
  for (const raw of text.split('\n')) {
    const line = raw.trim().replace(/\*\*/g, '').replace(/^[-*>]\s*/, '').trim();
    const send = /^SEND\s+@([^:]+):\s*(.+)$/i.exec(line);
    if (send) {
      const to = known.get(nameKey(send[1]));
      if (to && nameKey(to) !== nameKey(self)) sends.push({ to, text: send[2].trim() });
      continue;
    }
    const state = /^STATUS:\s*(done|waiting|working)\b(?:\s*@?([^\s].*))?$/i.exec(line);
    if (state) {
      status = state[1].toLowerCase() as TeamStatus;
      const on = state[2] ? known.get(nameKey(state[2].replace(/^@/, ''))) : undefined;
      waitingOn = status === 'waiting' ? on : undefined;
    }
  }
  return { sends, status, waitingOn };
}

/** The part of the plan for `name` (`## <name>` up to the next heading of that level), with the shared part. */
export function planFor(plan: string, name: string): string {
  const sections = plan.split(/^(?=##\s)/m);
  const pick = (title: RegExp) => sections.find((section) => title.test(section.split('\n')[0]))?.trim() ?? '';
  const own = pick(new RegExp(`^##\\s+${name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\b`, 'i'));
  const shared = pick(/^##\s+(shared|contracts?|common|api)\b/i);
  return [shared, own].filter(Boolean).join('\n\n') || plan.trim();
}

/** The messages waiting for `name`, oldest first. */
export const inboxOf = (state: Pick<TeamState, 'queue'>, name: string) =>
  state.queue.filter((message) => message.to === name && !message.taken);

/** A member's next input: the messages the others sent it. */
export function inboxText(messages: TeamMessage[]): string {
  return messages.map((message) => `FROM @${message.from}: ${message.text}`).join('\n');
}

/**
 * The run is over: no member is in a turn, nothing waits in a queue, and every member is done or waiting (on someone
 * who has nothing more to send, so nobody would ever answer). A member that took all its turns counts as failed.
 */
export function teamFinished(state: Pick<TeamState, 'members' | 'queue'>): boolean {
  if (state.queue.some((message) => !message.taken)) return false;
  return state.members.every((member) => !member.busy && (member.status === 'done' || member.status === 'waiting' || member.status === 'failed'));
}

/** TEAM.md: the members, their state and the queue, for the user and for the agents (they may read it). */
export function teamText(state: TeamState): string {
  const members = state.members.map((member) => `- ${member.name}: ${member.status}${member.waitingOn ? ` on @${member.waitingOn}` : ''} (${member.turns} turns)`);
  const queue = state.queue.map((message) => `- ${message.taken ? '[x]' : '[ ]'} ${message.from} → ${message.to}: ${message.text}`);
  return [`# Team run`, '', `Task: ${state.task}`, '', '## Members', ...members, '', '## Queue', ...(queue.length ? queue : ['(empty)']), '', '## Log', ...state.log.slice(-200)].join('\n') + '\n';
}
