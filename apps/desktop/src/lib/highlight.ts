// One syntax highlighter for the file viewer and the chat code blocks. shiki is loaded on first use, with the
// JavaScript regex engine (no WebAssembly, so the page CSP never gets in the way), and each grammar is fetched
// the first time a file of that language is shown. Colours come from the GitHub light/dark themes, picked by the app theme.
import type { HighlighterGeneric, ThemedToken } from 'shiki';

export type Token = ThemedToken;
type Hl = HighlighterGeneric<string, string>;

const THEMES = ['github-light', 'github-dark'];
let highlighter: Promise<Hl> | undefined;
const loadedLangs = new Map<string, Promise<boolean>>();

function core(): Promise<Hl> {
  highlighter ??= import('shiki').then(
    ({ createHighlighter, createJavaScriptRegexEngine }) =>
      createHighlighter({ themes: THEMES, langs: [], engine: createJavaScriptRegexEngine({ forgiving: true }) }) as unknown as Promise<Hl>,
  );
  return highlighter;
}

/** Warms shiki up once the app has settled, so the first file or code block is not waiting for it. */
const WARM = ['typescript', 'javascript', 'json', 'yaml', 'markdown', 'toml', 'rust', 'svelte', 'css', 'html', 'bash'];
export function preloadHighlighter() {
  // The editor's own code and the grammars of the usual project files, one at a time while the app is idle.
  void import('./editor-cm').catch(() => undefined);
  void core().then(async (hl) => {
    for (const lang of WARM) {
      await ready(hl, lang);
      await new Promise((resolve) => setTimeout(resolve, 50));
    }
  }, () => undefined);
}

/** True when the grammar for `lang` is ready (loads it the first time); false for a language shiki does not know. */
async function ready(hl: Hl, lang: string): Promise<boolean> {
  let loading = loadedLangs.get(lang);
  if (!loading) {
    loading = hl.loadLanguage(lang as never).then(
      () => true,
      () => false,
    );
    loadedLangs.set(lang, loading);
  }
  return loading;
}

export const themeName = (dark: boolean) => (dark ? 'github-dark' : 'github-light');

/** Coloured tokens per line, or null when the language is unknown or the colouring fails. */
export async function highlightTokens(code: string, lang: string, dark: boolean): Promise<Token[][] | null> {
  try {
    const hl = await core();
    if (!(await ready(hl, lang))) return null;
    return hl.codeToTokens(code, { lang, theme: themeName(dark) }).tokens;
  } catch {
    return null;
  }
}

const escapeHtml = (text: string) => text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');

/** The inner HTML of a `<code>` for a block of code (lines joined by newlines), or null when it cannot be coloured. */
export async function highlightHtml(code: string, lang: string, dark: boolean): Promise<string | null> {
  const lines = await highlightTokens(code, lang, dark);
  if (!lines) return null;
  return lines
    .map((line) => line.map((token) => (token.color ? `<span style="color:${token.color}">${escapeHtml(token.content)}</span>` : escapeHtml(token.content))).join(''))
    .join('\n');
}
