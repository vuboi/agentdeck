// One syntax highlighter for the file viewer and the chat code blocks. shiki runs in a Web Worker
// (highlight.worker.ts) with the JavaScript regex engine (no WebAssembly, so the page CSP never gets in the way), and
// each grammar is fetched the first time a file of that language is shown. Colours come from the GitHub light/dark
// themes, picked by the app theme.
// In the page's own thread shiki took hundreds of ms per long block, and that landed in the middle of scrolling the
// chat (profiled as frozen frames of 0.5 to 1.2 s); in the worker the page only puts the colours in. Where a worker
// cannot start, shiki runs here as before.
import type { HighlighterGeneric } from 'shiki';
import type { WorkerReply, WorkerRequest, WorkerToken } from './highlight.worker';

export type Token = WorkerToken;

export const themeName = (dark: boolean) => (dark ? 'github-dark' : 'github-light');

// ---------- the worker ----------

let worker: Worker | null | undefined;
let nextId = 0;
const waiting = new Map<number, (tokens: Token[][] | null) => void>();

function startWorker(): Worker | null {
  if (worker !== undefined) return worker;
  try {
    worker = new Worker(new URL('./highlight.worker.ts', import.meta.url), { type: 'module' });
    worker.onmessage = (event: MessageEvent<WorkerReply>) => {
      const done = waiting.get(event.data.id);
      waiting.delete(event.data.id);
      done?.(event.data.tokens);
    };
    worker.onerror = () => {
      // The worker died (a failed import): what waits for it gets no colours, later calls colour in this thread.
      worker = null;
      for (const done of waiting.values()) done(null);
      waiting.clear();
    };
  } catch {
    worker = null;
  }
  return worker;
}

type Request = WorkerRequest extends infer R ? (R extends { id: number } ? Omit<R, 'id'> : never) : never;

function ask(request: Request): Promise<Token[][] | null> | null {
  const thread = startWorker();
  if (!thread) return null;
  const id = ++nextId;
  return new Promise((resolve) => {
    waiting.set(id, resolve);
    thread.postMessage({ ...request, id } as WorkerRequest);
  });
}

// ---------- the fallback: shiki in this thread ----------

type Hl = HighlighterGeneric<string, string>;
let local: Promise<Hl> | undefined;
const localLangs = new Map<string, Promise<boolean>>();

async function localTokens(code: string, lang: string, theme: string): Promise<Token[][] | null> {
  local ??= import('shiki').then(
    ({ createHighlighter, createJavaScriptRegexEngine }) =>
      createHighlighter({ themes: ['github-light', 'github-dark'], langs: [], engine: createJavaScriptRegexEngine({ forgiving: true }) }) as unknown as Promise<Hl>,
  );
  const hl = await local;
  let loading = localLangs.get(lang);
  if (!loading) {
    loading = hl.loadLanguage(lang as never).then(
      () => true,
      () => false,
    );
    localLangs.set(lang, loading);
  }
  if (!(await loading)) return null;
  // A task of its own, so the page can draw between blocks.
  await new Promise((resolve) => setTimeout(resolve, 0));
  return hl.codeToTokens(code, { lang, theme }).tokens.map((line) => line.map(({ content, color }) => ({ content, color })));
}

// ---------- the API ----------

/** Warms shiki up once the app has settled, so the first file or code block is not waiting for it. */
// Svelte first: its grammar is the heaviest (it brings TypeScript, CSS and HTML along) and the app's own files are Svelte.
const WARM = ['svelte', 'typescript', 'rust', 'json', 'markdown', 'toml', 'yaml', 'bash', 'javascript', 'css', 'html'];
export function preloadHighlighter() {
  // The editor's own code, and the grammars of the usual project files (in the worker, so it costs the page nothing).
  void import('./editor-cm').catch(() => undefined);
  void ask({ kind: 'warm', langs: WARM });
}

/** Tokens already worked out, by theme, language and code: a view mounted again (a chat row scrolled back into view,
 *  a file reopened) gets them at once. */
const tokenCache = new Map<string, Token[][]>();
const TOKEN_CACHE_MAX = 400;

/** Coloured tokens per line, or null when the language is unknown or the colouring fails. */
export async function highlightTokens(code: string, lang: string, dark: boolean): Promise<Token[][] | null> {
  const key = `${dark ? 'd' : 'l'}|${lang}|${code}`;
  const hit = tokenCache.get(key);
  if (hit) return hit;
  try {
    const theme = themeName(dark);
    const tokens = await (ask({ kind: 'tokens', code, lang, theme }) ?? localTokens(code, lang, theme));
    if (!tokens) return null;
    tokenCache.set(key, tokens);
    if (tokenCache.size > TOKEN_CACHE_MAX) tokenCache.delete(tokenCache.keys().next().value as string);
    return tokens;
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
