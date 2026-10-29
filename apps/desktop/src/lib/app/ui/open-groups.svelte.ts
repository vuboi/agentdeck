// What the user opened in the step lists of a chat (the line "Ran 2 commands, looked at 3 things" and its steps).
// A list is rebuilt when its chat comes back on screen (another chat, the parent and back, a reload of the view), so
// what is open is kept here, by chat, message and position, and not in the list itself. Not saved to disk.

export type GroupState = { open: boolean; thinking: boolean; rows: Record<number, boolean> };

const states = new Map<string, GroupState>();

export function groupState(key: string): GroupState {
  let found = states.get(key);
  if (!found) {
    const fresh: GroupState = $state({ open: false, thinking: false, rows: {} });
    states.set(key, fresh);
    found = fresh;
  }
  return found;
}
