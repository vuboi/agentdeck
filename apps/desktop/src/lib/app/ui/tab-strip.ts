/** Scrolls a tab strip just far enough to show the tab with this id, and no further. */
export function revealTab(strip: HTMLElement | undefined, id: string) {
  const el = strip?.querySelector<HTMLElement>(`[data-tab-id="${CSS.escape(id)}"]`);
  if (!strip || !el) return;
  const box = strip.getBoundingClientRect();
  const tab = el.getBoundingClientRect();
  if (tab.left < box.left) strip.scrollLeft -= box.left - tab.left;
  else if (tab.right > box.right) strip.scrollLeft += tab.right - box.right;
}
