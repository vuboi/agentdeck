// How tall a chat row will be before it is drawn, for the chat's virtual list (Chat.svelte). A row the list has not
// measured yet sits at this guess; the closer it is, the less the rows around it move when it is measured, which is
// what makes a fast scroll or a scrollbar drag stutter. Measured heights are kept per message and width, so a row seen
// once is exact the next time (after a page of older messages is put in front, or the chat is opened again). Free of
// stores so plain node can test it.

type SizedBlock = { kind: string; text?: string };
type SizedMessage = { id: string; role: string; blocks: SizedBlock[] };

/** Row padding (py-2.5), the name line of an answer (with its margin) and the time / actions line under every row. */
const ROW_PAD = 20;
const NAME_LINE = 25;
const FOOTER = 30;
const LINE = 22;
const CODE_LINE = 20;
/** Average width of a character of the chat text, in px (text-sm). */
const CHAR_PX = 7.4;

function textHeight(text: string, width: number): number {
  const perLine = Math.max(20, Math.floor(width / CHAR_PX));
  let height = 0;
  let inCode = false;
  for (const line of text.split('\n')) {
    if (line.trimStart().startsWith('```')) {
      inCode = !inCode;
      height += inCode ? 16 : 8; // the code box's own padding
      continue;
    }
    if (inCode) height += CODE_LINE;
    else if (!line.trim()) height += 8; // paragraph gap
    else height += Math.ceil(line.length / perLine) * LINE;
  }
  return height;
}

/** The guessed height of a message's row at a column width (the list's width). */
export function guessRowHeight(message: SizedMessage, listWidth: number): number {
  // The row is max-w-3xl inside the list's left padding; its own side padding is px-3, or pl-12 pr-6 from 720px.
  const row = Math.min(768, listWidth - 10);
  const width = Math.max(160, row - (listWidth >= 720 ? 72 : 24));
  let height = ROW_PAD + FOOTER + (message.role === 'assistant' ? NAME_LINE : 0);
  let tools = 0;
  for (const block of message.blocks) {
    if (block.kind === 'response') height += textHeight(block.text ?? '', message.role === 'user' ? width * 0.85 - 24 : width) + 4;
    else if (block.kind === 'command') tools++;
    else if (block.kind === 'thinking' || block.kind === 'loading') height += 28;
    else if (block.kind === 'image') height += 180;
    else height += 40;
  }
  // Tool calls in a row fold into one group line.
  if (tools) height += 32;
  if (message.role === 'user') height += 20; // the bubble's padding
  return Math.round(height);
}

/** Heights measured before, by message id and a width bucket (a resize of a few pixels keeps them). */
const measured = new Map<string, number>();
const MEASURED_MAX = 5000;
const bucket = (width: number) => Math.round(width / 40);
const keyOf = (id: string, width: number) => `${id}@${bucket(width)}`;

// Kept across a reload too: after F5 every row would otherwise stand at its guess and jump to its real height as it is
// measured. Only the most recent heights are saved, as a viewer's own convenience (a plain cache, not data to keep).
const STORE = 'chat.rowHeights';
const SAVED_MAX = 1500;
try {
  const saved = typeof localStorage === 'undefined' ? null : localStorage.getItem(STORE);
  if (saved) for (const [key, height] of JSON.parse(saved) as [string, number][]) measured.set(key, height);
} catch {
  // A cache that cannot be read is just empty.
}
let saveTimer: ReturnType<typeof setTimeout> | undefined;
function saveSoon() {
  clearTimeout(saveTimer);
  saveTimer = setTimeout(() => {
    try {
      localStorage.setItem(STORE, JSON.stringify([...measured].slice(-SAVED_MAX)));
    } catch {
      // Storage full or blocked: the heights are measured again next time.
    }
  }, 800);
}

export function rememberRowHeight(id: string, width: number, height: number) {
  if (height <= 0) return;
  const key = keyOf(id, width);
  measured.delete(key);
  measured.set(key, height);
  if (measured.size > MEASURED_MAX) measured.delete(measured.keys().next().value as string);
  saveSoon();
}

/** The height a row had before at this width, else the guess. A live reply is never taken from the cache: it grows. */
export function rowHeight(message: SizedMessage & { live?: boolean }, listWidth: number): number {
  if (message.live) return guessRowHeight(message, listWidth);
  const key = keyOf(message.id, listWidth);
  const known = measured.get(key) ?? guessed.get(key);
  if (known !== undefined) return known;
  // The virtualizer asks for every row not measured yet each time any row is measured: the guess reads the whole text,
  // so it is worked out once per message and width.
  const guess = guessRowHeight(message, listWidth);
  guessed.set(key, guess);
  if (guessed.size > MEASURED_MAX) guessed.delete(guessed.keys().next().value as string);
  return guess;
}

const guessed = new Map<string, number>();
