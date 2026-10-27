import { getPref, setPref } from './prefs';

// Counts the account switches of each agent. A conversation begun under one account is not resumed under another:
// resuming it there stalls the reply, so the next turn starts a new session with the conversation so far.
// The counts are kept between runs of the app: chats remember the count they began under, and a count that went back
// to zero at start would look like a switch.
const KEY = 'accounts.epochs';

export const accountEpoch = $state<Record<string, number>>({});

void getPref<Record<string, number>>(KEY).then((saved) => {
  if (!saved) return;
  const early = Object.keys(accountEpoch).length > 0;
  // A switch made before the saved counts arrived is added on top of them, not lost.
  for (const [agent, count] of Object.entries(saved)) accountEpoch[agent] = count + (accountEpoch[agent] ?? 0);
  if (early) void setPref(KEY, { ...accountEpoch });
});

export function accountSwitched(agent: string) {
  accountEpoch[agent] = (accountEpoch[agent] ?? 0) + 1;
  void setPref(KEY, { ...accountEpoch });
}
