// Settings > Replies: the house style given to every new conversation, whether file paths in an answer become links,
// and whether pictures from the web are shown through the app. All on by default; an old conversation is not touched.
import { setMarkdownOptions } from '../../app/text/markdown';
import { getPref, setPref } from '../../app/platform/prefs';

export const replyStyle = $state({
  /** Give agents the reply conventions at the start of a conversation (house-style.ts). */
  houseStyle: true,
  /** The user's own wording for them; blank means the built-in text. */
  houseStyleText: '',
  /** Turn file paths found in an answer into links (path-links.ts). */
  linkPaths: true,
  /** Tell agents about the project's memory file at the start of a conversation (project-memory.ts). */
  projectMemory: true,
  /** Show `![](https://…)` pictures through the app (a copy in the cache), not straight from the web. */
  proxyImages: true,
  /** The Piper voice that reads Vietnamese aloud (see `VOICES` in speech.ts). */
  voice: 'vais1000',
});

const KEY = 'chat.replyStyle';
let hydrated = false;
void getPref<Partial<typeof replyStyle>>(KEY).then((saved) => {
  if (saved) Object.assign(replyStyle, saved);
  hydrated = true;
});

$effect.root(() => {
  $effect(() => {
    setMarkdownOptions({ linkPaths: replyStyle.linkPaths, proxyImages: replyStyle.proxyImages });
  });
  $effect(() => {
    const value = { ...replyStyle };
    if (hydrated) setPref(KEY, value);
  });
});
