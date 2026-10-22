import { cubicOut } from 'svelte/easing';

/** Horizontal counterpart of Svelte's `slide`: animates `width` plus padding and border. */
export function slideWidth(node: HTMLElement, { duration = 240, easing = cubicOut } = {}) {
  const style = getComputedStyle(node);
  const width = parseFloat(style.width);
  return {
    duration,
    easing,
    css: (t: number) =>
      `overflow: hidden; white-space: nowrap; min-width: 0; width: ${t * width}px; opacity: ${t};`,
  };
}
