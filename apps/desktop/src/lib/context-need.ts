// Whether a message only makes sense with the conversation before it. A new session (after an account switch, or when a
// thread is gone) starts with no history; the conversation so far is put in front of the message only when the message
// needs it. A message that says what it is about (a file, a command, a URL, enough words of its own) goes as it is.
// Not a matter of length: "ok, do it" is short and needs the history; a long request that names its files does not.
// Free of stores and Tauri so plain node can test it.

// A letter or digit on either side means the word is part of a longer one ("it" in "item", "đó" in "đóng").
const L = String.raw`(?<![\p{L}\p{N}_])`;
const R = String.raw`(?![\p{L}\p{N}_])`;
const words = (list: string[]) => new RegExp(`${L}(?:${list.join('|')})${R}`, 'iu');

// Words that point back at something said before.
const REFERS = words([
  // English
  'it', 'its', 'this', 'that', 'these', 'those', 'them', 'there', 'above', 'below', 'previous', 'previously', 'earlier', 'before', 'same', 'again',
  'continue', 'carry on', 'keep going', 'go on', 'proceed', 'next', 'also', 'instead', 'undo', 'revert', 'the rest', 'the other', 'the one',
  'as before', 'as above', 'like before', 'like that', 'do it', 'try again', 'go ahead',
  // Vietnamese
  'nó', 'cái đó', 'cái này', 'cái kia', 'cái nãy', 'cái trên', 'chỗ đó', 'chỗ này', 'chỗ trên', 'thằng đó', 'thằng này', 'thằng trên',
  'vừa rồi', 'vừa nãy', 'lúc nãy', 'ban nãy', 'ở trên', 'bên trên', 'phía trên', 'như trên', 'như vậy', 'như thế', 'như cũ', 'như lúc nãy',
  'tiếp tục', 'tiếp đi', 'tiếp nhé', 'làm tiếp', 'sửa tiếp', 'làm lại', 'sửa lại', 'làm đi', 'sửa đi', 'thế nào', 'vậy', 'thế', 'đó', 'kia', 'nữa',
]);

// Things that name the subject on their own: a path, a file, a URL, code, a command, an identifier, a quoted text.
const ANCHORS: RegExp[] = [
  /[\w.-]+[\\/][\w.-]+/u, // a path
  /\b[\w-]+\.(?:ts|tsx|js|jsx|mjs|svelte|vue|rs|go|py|java|cs|json|toml|ya?ml|md|css|scss|html|sql|sh|ps1|lock|env)\b/i,
  /https?:\/\//i,
  /`[^`]+`/, // inline code or a fence
  /\b(?:pnpm|npm|npx|yarn|cargo|git|docker|node|bun|pip|make|curl)\s+\S+/i, // a command
  /\b[a-z]+[A-Z]\w+\b|\b[A-Z][a-z]+[A-Z]\w+\b|\b\w+_\w+\b/, // camelCase, PascalCase, snake_case
  /["“'‘][^"”'’]{3,}["”'’]/, // a quoted text
  /\b[A-Z]{2,}-?\d+\b|\b(?:E|TS|ERR)\d{2,}\b/, // an error or ticket code
];

/** Fewer words than this, with nothing that names the subject, cannot stand without the conversation. */
const TOO_SHORT = 4;
/** With a pointing-back word, a message this short or shorter leans on the conversation even if it names a file. */
const SHORT_WITH_POINTER = 8;

/** True when `prompt` should be given the conversation so far. An empty prompt (only attachments) says nothing of its own. */
export function needsContext(prompt: string): boolean {
  const text = prompt.trim();
  if (!text) return true;
  const count = text.split(/\s+/).length;
  const pointsBack = REFERS.test(text);
  const anchored = ANCHORS.some((pattern) => pattern.test(text));
  if (pointsBack && !anchored) return true;
  if (pointsBack && count <= SHORT_WITH_POINTER) return true;
  if (!anchored && count <= TOO_SHORT) return true;
  return false;
}
