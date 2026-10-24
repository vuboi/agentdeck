// Makes source readable: JavaScript, TypeScript, JSON, CSS/SCSS/Less, HTML, Markdown and YAML go through Prettier,
// loaded only when asked for.

const LIMIT = 800_000;
export const FORMATTABLE = ['javascript', 'typescript', 'css', 'html', 'json', 'jsonc', 'json5', 'scss', 'less', 'markdown', 'yaml', 'jsx', 'tsx'];

/** A long text with few line breaks: a bundle or a minified file. */
export function looksMinified(text: string): boolean {
  if (text.length < 1500) return false;
  const lines = text.split('\n').length;
  return text.length / lines > 220;
}

export function canFormat(text: string, lang: string | undefined): boolean {
  return Boolean(lang && FORMATTABLE.includes(lang) && text.length <= LIMIT);
}

type Plugin = () => Promise<unknown>;
const plugin = (load: Plugin) => load().then((mod) => (mod as { default?: unknown }).default ?? mod);
const PLUGINS = {
  babel: () => plugin(() => import('prettier/plugins/babel')),
  estree: () => plugin(() => import('prettier/plugins/estree')),
  typescript: () => plugin(() => import('prettier/plugins/typescript')),
  postcss: () => plugin(() => import('prettier/plugins/postcss')),
  html: () => plugin(() => import('prettier/plugins/html')),
  markdown: () => plugin(() => import('prettier/plugins/markdown')),
  yaml: () => plugin(() => import('prettier/plugins/yaml')),
} as const;

const SETUP: Record<string, { parser: string; plugins: (keyof typeof PLUGINS)[]; width?: number }> = {
  javascript: { parser: 'babel', plugins: ['babel', 'estree'], width: 100 },
  jsx: { parser: 'babel', plugins: ['babel', 'estree'], width: 100 },
  typescript: { parser: 'typescript', plugins: ['babel', 'estree', 'typescript'], width: 100 },
  tsx: { parser: 'typescript', plugins: ['babel', 'estree', 'typescript'], width: 100 },
  json: { parser: 'json', plugins: ['babel', 'estree'] },
  jsonc: { parser: 'json', plugins: ['babel', 'estree'] },
  json5: { parser: 'json5', plugins: ['babel', 'estree'] },
  css: { parser: 'css', plugins: ['postcss'] },
  scss: { parser: 'scss', plugins: ['postcss'] },
  less: { parser: 'less', plugins: ['postcss'] },
  html: { parser: 'html', plugins: ['html', 'babel', 'estree', 'postcss'] },
  markdown: { parser: 'markdown', plugins: ['markdown'] },
  yaml: { parser: 'yaml', plugins: ['yaml'] },
};

/** The text indented; throws when Prettier cannot read it. */
export async function prettifyStrict(text: string, lang: string): Promise<string> {
  const setup = SETUP[lang];
  if (!setup) throw new Error(`No formatter for ${lang}`);
  const prettier = await import('prettier/standalone');
  const plugins = await Promise.all(setup.plugins.map((name) => PLUGINS[name]()));
  return prettier.format(text, { parser: setup.parser, plugins: plugins as never[], ...(setup.width ? { printWidth: setup.width } : {}) });
}

/** The text indented; the original when Prettier cannot read it. */
export async function prettify(text: string, lang: string): Promise<string> {
  try {
    return await prettifyStrict(text, lang);
  } catch {
    return text;
  }
}
