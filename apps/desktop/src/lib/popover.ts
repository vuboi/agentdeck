import { zHoverGlide } from '@shival99/z-ui-svelte';

// The rules every popover of the app follows (see AGENTS.md, "Popovers"): the button that opened one stays lit while
// it is open, a list inside one highlights hovered rows with one box that glides between them, and the chosen row
// keeps a tint of its own.

/** Classes of the trigger while its popover is open (the model picker's look). Spread after the resting classes. */
export const TRIGGER_ON = '!bg-primary/15 !text-primary hover:!bg-primary/20';

/** The same, for a trigger that has no `!important` utilities of its own (a plain button). */
export const TRIGGER_ON_PLAIN = 'bg-primary/15 text-primary hover:bg-primary/20';

/**
 * How tall the lists of popovers are: a share of the window's height (so they follow the window, short in a small one and
 * roomier in a tall one), never under `LIST_MIN` px. A popover with a search box keeps this height whatever it shows, so it
 * does not jump about while the list is narrowed; it never goes past the room on its side of the button that opened it.
 */
export const LIST_VH = 0.3;
export const LIST_MIN = 180;
export const listHeight = (room = Number.POSITIVE_INFINITY) => Math.round(Math.min(room, Math.max(LIST_MIN, window.innerHeight * LIST_VH)));

/** The same share as a class, for the short lists that only need it as their limit. */
export const LIST_MAX = 'max-h-[max(180px,30vh)]';

/**
 * An item that is on (the open tab, a toggle that is pressed, the tool in front) keeps its colour while the pointer is over
 * it: hover must never paint over active. The `!` is what holds against the hover colours a button brings of its own.
 */
export const ACTIVE_ON = '!bg-primary/15 !text-primary hover:!bg-primary/20 hover:!text-primary';

/** The tint of the chosen (or keyboard-active) row of a list. */
export const ROW_ON = 'bg-primary/15 text-primary';

/**
 * Hover highlight for the list of a popover: `{@attach glide()}` on the list, `data-glide-item` on each row. Rows with
 * `aria-selected="true"` or `aria-checked="true"` keep their own tint and never take the hover box.
 */
export const glide = () => zHoverGlide({ class: 'bg-primary/10 rounded-md', exclude: '[aria-selected="true"], [aria-checked="true"]' });
