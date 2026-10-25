/**
 * Closes a floating menu when the page or a list scrolls, so it never drifts away from what it belongs to.
 * Scrolls inside the menu itself are ignored. Use from a `$effect`: it runs while `isOpen()` is true.
 */
export function closeOnScroll(isOpen: () => boolean, close: () => void): (() => void) | undefined {
  if (!isOpen()) return undefined;
  const onScroll = (event: Event) => {
    const target = event.target as Element | null;
    if (target?.closest?.('[role="menu"]')) return;
    close();
  };
  window.addEventListener('scroll', onScroll, { capture: true, passive: true });
  return () => window.removeEventListener('scroll', onScroll, { capture: true });
}
