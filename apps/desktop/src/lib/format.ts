// Makes minified source readable: JavaScript, CSS and HTML go through Prettier, loaded only when asked for.

const LIMIT = 800_000;
export const FORMATTABLE = ['javascript', 'typescript', 'css', 'html'];

/** A long text with few line breaks: a bundle or a minified file. */
export function looksMinified(text: string): boolean {
  if (text.length < 1500) return false;
  const lines = text.split('\n').length;
  return text.length / lines > 220;
}

export function canFormat(text: string, lang: string | undefined): boolean {
  return Boolean(lang && FORMATTABLE.includes(lang) && text.length <= LIMIT);
}

/** The text indented; the original when Prettier cannot read it. */
export async function prettify(text: string, lang: string): Promise<string> {
  const prettier = await import('prettier/standalone');
  try {
    if (lang === 'css') {
      const postcss = await import('prettier/plugins/postcss');
      return await prettier.format(text, { parser: 'css', plugins: [postcss.default ?? postcss] });
    }
    if (lang === 'html') {
      const [html, babel, estree, postcss] = await Promise.all([import('prettier/plugins/html'), import('prettier/plugins/babel'), import('prettier/plugins/estree'), import('prettier/plugins/postcss')]);
      return await prettier.format(text, { parser: 'html', plugins: [html.default ?? html, babel.default ?? babel, estree.default ?? estree, postcss.default ?? postcss] });
    }
    const [babel, estree, typescript] = await Promise.all([import('prettier/plugins/babel'), import('prettier/plugins/estree'), lang === 'typescript' ? import('prettier/plugins/typescript') : Promise.resolve(null)]);
    const plugins = [babel.default ?? babel, estree.default ?? estree, ...(typescript ? [typescript.default ?? typescript] : [])];
    return await prettier.format(text, { parser: lang === 'typescript' ? 'typescript' : 'babel', plugins, printWidth: 100 });
  } catch {
    return text;
  }
}
