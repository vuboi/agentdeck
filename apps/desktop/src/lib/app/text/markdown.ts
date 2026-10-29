// Markdown for chat answers: parsed with marked, cleaned with DOMPurify, code fences wrapped in a small card
// with a language label and a copy button (colouring is added afterwards by Markdown.svelte, not here).
import DOMPurify from 'dompurify';
import { marked, type Tokens } from 'marked';
import { labelOf, matchPath, pathStart, type PathSpan } from '../../files/path-links';

const escapeHtml = (text: string) =>
  text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');

// The copy button holds both icons; `data-copied` on the button swaps them (see .md-code-head in app.css).
const svg = (cls: string, body: string) =>
  `<svg class="${cls}" viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${body}</svg>`;
const RUN_ICON = svg('ic-run', '<polygon points="6 3 20 12 6 21 6 3"/>');
const LONG_BLOCK = 15;
const SHELLS = new Set(['bash', 'sh', 'shell', 'zsh', 'powershell', 'pwsh', 'ps1', 'cmd', 'bat', 'console']);
const COPY_ICON =
  svg('ic-copy', '<rect width="14" height="14" x="8" y="8" rx="2" ry="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/>') +
  svg('ic-check', '<path d="M20 6 9 17l-5-5"/>');
const KIND_ICON = {
  code: svg('ic-kind', '<polyline points="16 18 22 12 16 6"/><polyline points="8 6 2 12 8 18"/>'),
  table: svg('ic-kind', '<rect width="18" height="18" x="3" y="3" rx="2"/><path d="M3 9h18M3 15h18M9 3v18"/>'),
};
const PREVIEW_ICON = svg('ic-preview', '<path d="M15 3h6v6"/><path d="M9 21H3v-6"/><path d="M21 3l-7 7"/><path d="M3 21l7-7"/>');

// Settings > Replies. A change drops the cached HTML, which was made under the old setting.
let linkPaths = true;
let proxyImages = true;
export function setMarkdownOptions(options: { linkPaths: boolean; proxyImages: boolean }) {
  if (options.linkPaths === linkPaths && options.proxyImages === proxyImages) return;
  linkPaths = options.linkPaths;
  proxyImages = options.proxyImages;
  rendered.clear();
}

type PathToken = Tokens.Generic & { type: 'mdpath'; span: PathSpan };

// A file path in plain text becomes a link (path-links.ts). It is drawn with the shortest guess as the link and the rest of
// the longest guess as plain text after it; Markdown.svelte then checks the disk and keeps the longest guess that exists,
// which is how a path with spaces gets its right end. Inside a link, code or a code block it never runs.
const pathExtension = {
  name: 'mdpath',
  level: 'inline' as const,
  start: (src: string) => (linkPaths ? pathStart(src) : undefined),
  tokenizer(this: { lexer: { state: { inLink: boolean } } }, src: string): PathToken | undefined {
    if (!linkPaths || this.lexer.state.inLink) return undefined;
    const span = matchPath(src);
    return span ? { type: 'mdpath', raw: span.raw, span } : undefined;
  },
  renderer(token: Tokens.Generic) {
    const { span } = token as PathToken;
    const short = span.candidates[span.candidates.length - 1];
    const cands = span.candidates.map(({ path, length, line }) => [path, length, line]);
    return (
      `<a href="#" class="md-path" data-file="${escapeHtml(short.path)}${escapeHtml(short.line)}" data-cands="${escapeHtml(JSON.stringify(cands))}" data-raw="${escapeHtml(span.raw)}">${escapeHtml(labelOf(short.path, short.line))}</a>` +
      `<span class="md-path-rest">${escapeHtml(span.raw.slice(short.length))}</span>`
    );
  },
};

marked.use({
  gfm: true,
  breaks: true,
  extensions: [pathExtension],
  renderer: {
    // `http://localhost:4200` in backticks is an address too: make it a link.
    // marked passes the code as written (not escaped): `<pre>` in backticks must show as text, not become a tag.
    codespan({ text }: Tokens.Codespan) {
      const code = `<code>${escapeHtml(text)}</code>`;
      return /^https?:\/\/[^\s<>"]+$/.test(text) ? `<a href="${escapeHtml(text)}">${code}</a>` : code;
    },
    // A picture, sound or video on disk (`![Sound](/C:/out/cry.wav)`) is shown by the cards under the answer
    // (ArtifactCards: a player, a picture): as an <img> here it was a broken picture. Web and data pictures stay.
    image({ href, text }: Tokens.Image) {
      if (!/^(https?:|data:image\/)/i.test(href)) return '';
      // A picture on the web is fetched by the app and shown from its cache (the window may not load it itself): the
      // tag waits with `data-remote` until Markdown.svelte has the copy.
      if (proxyImages && /^https?:/i.test(href)) return `<img data-remote="${escapeHtml(href)}" alt="${escapeHtml(text)}">`;
      return `<img src="${escapeHtml(href)}" alt="${escapeHtml(text)}">`;
    },
    // A link to a file (`E:/repo/a.txt`, `/home/me/a.txt`, `src/a.ts:12`) keeps its path in data-file: DOMPurify drops
    // a Windows drive from href, and the click opens the file in a tab.
    link({ href, text }: Tokens.Link) {
      if (/^(file:\/\/|[a-zA-Z]:[\\/]|\/(?!\/))/.test(href) || (!/^[a-z][a-z0-9+.-]*:/i.test(href) && /\.\w+(:\d+)?$/.test(href) && !href.startsWith('#'))) {
        return `<a href="#" data-file="${escapeHtml(href)}">${text}</a>`;
      }
      return `<a href="${escapeHtml(href)}">${text}</a>`;
    },
    code({ text, lang }: Tokens.Code) {
      const language = (lang ?? '').trim().split(/\s+/)[0];
      return (
        `<div class="md-code" data-lang="${escapeHtml(language)}">` +
        `<div class="md-code-head"><span class="md-code-kind" title="${escapeHtml(language || 'text')}" aria-label="${escapeHtml(language || 'text')}">${KIND_ICON.code}</span><span class="md-code-actions">` +
        (SHELLS.has(language.toLowerCase()) ? `<button type="button" data-run title="Run in terminal" aria-label="Run in terminal">${RUN_ICON}</button>` : '') +
        `<button type="button" data-copy title="Copy" aria-label="Copy code">${COPY_ICON}</button></span></div>` +
        `<pre><code>${escapeHtml(text)}</code></pre>` +
        // A long block gets a copy button at its foot too (sticky), so nobody scrolls back up to copy it.
        (text.split('\n').length > LONG_BLOCK ? `<div class="md-code-foot"><button type="button" data-copy title="Copy" aria-label="Copy code">${COPY_ICON}</button></div>` : '') +
        '</div>'
      );
    },
  },
});

// Links open outside the app window, never inside it.
DOMPurify.addHook('afterSanitizeAttributes', (node) => {
  if (node.tagName === 'A') {
    node.setAttribute('target', '_blank');
    node.setAttribute('rel', 'noopener noreferrer');
  }
});

// Rows of a long chat are mounted again each time they scroll back into view: an answer that was already parsed and
// sanitised is taken from here, block by block (see `markdownBlocks`). Only recent blocks are kept.
const rendered = new Map<string, string>();
const RENDERED_MAX = 2000;

/** `keep: false` for text that is still being written: its next step differs, so caching it only pushes out answers. */
export function renderMarkdown(source: string, keep = true): string {
  const known = rendered.get(source);
  if (known !== undefined) {
    rendered.delete(source);
    rendered.set(source, known);
    return known;
  }
  const html = marked.parse(source, { async: false });
  // Tables sit in a rounded, scrollable card like the code blocks.
  // Each table has a head bar with a copy button (as a code block has), which copies it as tab-separated rows.
  const framed = html
    .replaceAll('<table>', `<div class="md-table"><div class="md-code-head"><span class="md-code-kind" title="Table" aria-label="Table">${KIND_ICON.table}</span><span class="md-code-actions"><button type="button" data-preview-table title="Preview table" aria-label="Preview table">${PREVIEW_ICON}</button><button type="button" data-copy-table title="Copy table" aria-label="Copy table">${COPY_ICON}</button></span></div><table>`)
    .replaceAll('</table>', '</table></div>')
    // A cell's content in a box of its own: a table cell ignores `max-width`, a box does not, so no column takes the
    // whole width and squeezes the others (see .md-cell).
    .replace(/<td([^>]*)>([\s\S]*?)<\/td>/g, '<td$1><div class="md-cell">$2</div></td>');
  const clean = DOMPurify.sanitize(framed, { ADD_ATTR: ['target'] });
  if (!keep) return clean;
  rendered.set(source, clean);
  if (rendered.size > RENDERED_MAX) rendered.delete(rendered.keys().next().value as string);
  return clean;
}

/**
 * An answer cut at its top-level blocks (paragraphs, headings, lists, code, tables), each with the blank lines after
 * it. Rendered one by one, a block that is complete keeps its HTML while the answer streams: only the last one is
 * parsed again. Joined, the pieces are the source.
 */
export function markdownBlocks(source: string, previous: string[] = []): string[] {
  // While an answer grows, the blocks before the last one are done: only the text from the last block on is read again.
  const kept = previous.slice(0, -1);
  const prefix = kept.join('');
  if (kept.length && source.startsWith(prefix)) return [...kept, ...markdownBlocks(source.slice(prefix.length))];
  const blocks: string[] = [];
  for (const token of marked.lexer(source)) {
    if (token.type === 'space' && blocks.length) blocks[blocks.length - 1] += token.raw;
    else blocks.push(token.raw);
  }
  // The lexer can drop a trailing piece it has no token for; it stays with the last block so nothing is lost.
  const joined = blocks.join('');
  if (joined.length < source.length && source.startsWith(joined)) {
    if (blocks.length) blocks[blocks.length - 1] += source.slice(joined.length);
    else blocks.push(source);
  }
  return blocks;
}

// Code colouring in flight. A chat that is being opened waits for it (a short while at most) before it is shown, so code
// does not appear plain and then change colour in front of the reader.
let colouring = 0;
let settled: (() => void)[] = [];
export function colourStarted() {
  colouring++;
}
export function colourFinished() {
  colouring = Math.max(0, colouring - 1);
  if (colouring === 0) for (const done of settled.splice(0)) done();
}
export function whenColoured(maxMs: number): Promise<void> {
  if (colouring === 0) return Promise.resolve();
  return new Promise((resolve) => {
    settled.push(resolve);
    setTimeout(resolve, maxMs);
  });
}

// While a chat is being opened (still hidden) code is coloured at once, so it is never seen plain. Afterwards rows that
// mount as the reader scrolls are coloured when the page is idle: shiki is heavy and would stall the scrolling.
let eager = false;
export const setEagerColour = (on: boolean) => (eager = on);
export const eagerColour = () => eager;
