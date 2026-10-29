// The icon buttons at the right of the Store's top row (add, refresh, …). The tab on screen says which they are: it sets
// them while it is open and takes them away when it goes, so one row serves all four tabs.

export type BarAction = { id: string; label: string; icon: string; onclick: () => void; disabled?: boolean; busy?: boolean };

export const topActions = $state<{ items: BarAction[]; owner: symbol | null }>({ items: [], owner: null });

/** Shows `items` in the top row for `owner` (any unique symbol, one per tab). A later owner replaces an earlier one. */
export function setTopActions(owner: symbol, items: BarAction[]) {
  topActions.owner = owner;
  topActions.items = items;
}

/** Takes `owner`'s buttons away. A tab that was already replaced by another does not clear that one's buttons. */
export function clearTopActions(owner: symbol) {
  if (topActions.owner !== owner) return;
  topActions.owner = null;
  topActions.items = [];
}
