// Shared pieces for the TanStack virtual lists (chat, code view): measuring rows without forcing a layout.

/**
 * A `measureElement` for a virtualizer whose rows have heights of their own. Sizes come from the ResizeObserver entry,
 * which reports them after layout. TanStack's default reads offsetHeight when a row is put in, a forced layout per row
 * in the middle of a scroll frame (profiled as most of the chat's stutter). A row with no entry yet stands at the
 * height it had before, or `fallback`, until the observer reports. Rows need `data-index`.
 */
export function observedMeasure(fallback: (index: number) => number) {
  const sizes = new Map<number, number>();
  return {
    measureElement(element: Element, entry: ResizeObserverEntry | undefined): number {
      const index = Number((element as HTMLElement).dataset.index);
      const box = entry?.borderBoxSize?.[0]?.blockSize;
      if (box === undefined) return sizes.get(index) ?? fallback(index);
      const height = Math.round(box);
      sizes.set(index, height);
      return height;
    },
    /** Forgets the heights (new text, wrapping switched): rows are measured again as they show. */
    reset() {
      sizes.clear();
    },
  };
}
