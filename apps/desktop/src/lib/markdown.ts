// Markdown for chat answers: parsed with marked, cleaned with DOMPurify, code fences wrapped in a small card
// with a language label and a copy button (colouring is added afterwards by Markdown.svelte, not here).
import DOMPurify from 'dompurify';
import { marked, type Tokens } from 'marked';

const escapeHtml = (text: string) =>
  text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');

marked.use({
  gfm: true,
  breaks: true,
  renderer: {
    code({ text, lang }: Tokens.Code) {
      const language = (lang ?? '').trim().split(/\s+/)[0];
      return (
        `<div class="md-code" data-lang="${escapeHtml(language)}">` +
        `<div class="md-code-head"><span>${escapeHtml(language || 'text')}</span>` +
        `<button type="button" data-copy>Copy</button></div>` +
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

export function renderMarkdown(source: string): string {
  const html = marked.parse(source, { async: false });
  return DOMPurify.sanitize(html, { ADD_ATTR: ['target'] });
}
