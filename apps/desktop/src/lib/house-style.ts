// The reply conventions AgentDeck gives every new conversation (Settings > Replies). Plain data and one function, so
// node can test it; the switches and the user's own wording live in reply-style.svelte.ts.

export const DEFAULT_HOUSE_STYLE =
  'Reply conventions of this app: when you name a file or folder, write it as a markdown link whose label is the file name ' +
  'and whose target is the absolute path with forward slashes, like [index.html](C:/work/site/index.html); never print a raw ' +
  'path in a sentence (paths stay only inside code blocks, for a command or its output). A picture from the web is downloaded ' +
  'to a local file first and shown from there: serve the folder on localhost and give a link like ' +
  '[cat.png](http://127.0.0.1:8765/cat.png).';

/** The wording to send, or '' when the convention is off. A blank custom text means the default. */
export function houseStyleHint(on: boolean, custom: string): string {
  if (!on) return '';
  return custom.trim() || DEFAULT_HOUSE_STYLE;
}
