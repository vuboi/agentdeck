/**
 * Undo and redo for a text field whose value is set from code. A browser keeps a textarea's own undo history only while
 * the user types in it; a value written by the app (a cleared field, a sent message, a suggestion, another chat's draft)
 * drops it. This keeps the history itself: typing in a burst is one step, a big change (everything deleted, a message
 * sent, a paste) is a step of its own.
 */
export type TextHistory = {
  /** Tells the history the field now holds `value`. `boundary` makes it a step of its own (a send, a clear). */
  note: (value: string, options?: { boundary?: boolean }) => void;
  /** The text before the last change, or undefined when there is nothing to go back to. */
  undo: () => string | undefined;
  redo: () => string | undefined;
  /** Handles Ctrl/Cmd+Z and Ctrl+Y / Ctrl+Shift+Z. True when it did and the key must not reach the field. */
  key: (event: KeyboardEvent, apply: (value: string) => void) => boolean;
};

const LIMIT = 200;
/** Typing keeps one step while the pauses between keys are shorter than this. */
const BURST_MS = 1000;
/** A change this large (a paste, a clear) is a step of its own. */
const BIG_CHANGE = 20;

export function createTextHistory(initial = ''): TextHistory {
  const past: string[] = [];
  let future: string[] = [];
  let last = initial;
  let at = 0;

  function note(value: string, { boundary = false }: { boundary?: boolean } = {}) {
    if (value === last) return;
    const now = Date.now();
    const big = boundary || value === '' || last === '' || Math.abs(value.length - last.length) > BIG_CHANGE;
    if (big || now - at > BURST_MS || past.length === 0) {
      past.push(last);
      if (past.length > LIMIT) past.shift();
    }
    at = big ? 0 : now;
    last = value;
    future = [];
  }

  function undo() {
    const previous = past.pop();
    if (previous === undefined) return undefined;
    future.push(last);
    last = previous;
    at = 0;
    return previous;
  }

  function redo() {
    const next = future.pop();
    if (next === undefined) return undefined;
    past.push(last);
    last = next;
    at = 0;
    return next;
  }

  function key(event: KeyboardEvent, apply: (value: string) => void) {
    if (!(event.ctrlKey || event.metaKey) || event.altKey) return false;
    const name = event.key.toLowerCase();
    const wantsRedo = (name === 'z' && event.shiftKey) || (name === 'y' && !event.shiftKey);
    const wantsUndo = name === 'z' && !event.shiftKey;
    if (!wantsUndo && !wantsRedo) return false;
    const value = wantsUndo ? undo() : redo();
    if (value === undefined) return false;
    event.preventDefault();
    apply(value);
    return true;
  }

  return { note, undo, redo, key };
}
