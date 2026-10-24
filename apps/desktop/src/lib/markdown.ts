// Markdown for chat answers: parsed with marked, cleaned with DOMPurify, code fences wrapped in a small card
// with a language label and a copy button (colouring is added afterwards by Markdown.svelte, not here).
import DOMPurify from 'dompurify';
import { marked, type Tokens } from 'marked';

const escapeHtml = (text: string) =>
  text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');

// The copy button holds both icons; `data-copied` on the button swaps them (see .md-code-head in app.css).
const svg = (cls: string, body: string) =>
  `<svg class="${cls}" viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${body}</svg>`;
const RUN_ICON = svg('ic-run', '<polygon points="6 3 20 12 6 21 6 3"/>');
const SHELLS = new Set(['bash', 'sh', 'shell', 'zsh', 'powershell', 'pwsh', 'ps1', 'cmd', 'bat', 'console']);
const COPY_ICON =
  svg('ic-copy', '<rect width="14" height="14" x="8" y="8" rx="2" ry="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/>') +
  svg('ic-check', '<path d="M20 6 9 17l-5-5"/>');

marked.use({
  gfm: true,
  breaks: true,
  renderer: {
    // `http://localhost:4200` in backticks is an address too: make it a link.
    codespan({ text }: Tokens.Codespan) {
      const code = `<code>${text}</code>`;
      return /^https?:\/\/[^\s<>"]+$/.test(text) ? `<a href="${escapeHtml(text.replace(/&amp;/g, '&'))}">${code}</a>` : code;
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
        `<div class="md-code-head"><span>${escapeHtml(language || 'text')}</span><span class="md-code-actions">` +
        (SHELLS.has(language.toLowerCase()) ? `<button type="button" data-run title="Run in terminal" aria-label="Run in terminal">${RUN_ICON}</button>` : '') +
        `<button type="button" data-copy title="Copy" aria-label="Copy code">${COPY_ICON}</button></span></div>` +
        `<pre><code>${escapeHtml(text)}</code></pre></div>`
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
// sanitised is taken from here. Only recent texts are kept (a streaming answer adds one entry per step).
const rendered = new Map<string, string>();
const RENDERED_MAX = 400;

export function renderMarkdown(source: string): string {
  const known = rendered.get(source);
  if (known !== undefined) {
    rendered.delete(source);
    rendered.set(source, known);
    return known;
  }
  const html = marked.parse(source, { async: false });
  // Tables sit in a rounded, scrollable card like the code blocks.
  const framed = html.replaceAll('<table>', '<div class="md-table"><table>').replaceAll('</table>', '</table></div>');
  const clean = DOMPurify.sanitize(framed, { ADD_ATTR: ['target'] });
  rendered.set(source, clean);
  if (rendered.size > RENDERED_MAX) rendered.delete(rendered.keys().next().value as string);
  return clean;
}
