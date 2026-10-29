import { inTauri } from '../app/platform/history';

// Extension -> shiki language the code viewer highlights. Anything else is shown as plain text.
const LANGUAGE: Record<string, string> = {
  ts: 'typescript', mts: 'typescript', cts: 'typescript', tsx: 'tsx',
  js: 'javascript', mjs: 'javascript', cjs: 'javascript', jsx: 'jsx',
  html: 'html', htm: 'html', xhtml: 'html', vue: 'vue', svelte: 'svelte', astro: 'astro',
  css: 'css', scss: 'scss', sass: 'sass', less: 'less', styl: 'stylus', pcss: 'css',
  json: 'json', jsonc: 'jsonc', json5: 'json5', geojson: 'json', webmanifest: 'json', map: 'json',
  yml: 'yaml', yaml: 'yaml', toml: 'toml', ini: 'ini', cfg: 'ini', conf: 'ini', properties: 'properties',
  env: 'dotenv', xml: 'xml', svg: 'xml', xsd: 'xml', xsl: 'xml', plist: 'xml', csproj: 'xml', xaml: 'xml',
  rs: 'rust', py: 'python', pyw: 'python', rb: 'ruby', php: 'php', go: 'go', java: 'java', kt: 'kotlin', kts: 'kotlin',
  swift: 'swift', c: 'c', h: 'c', cpp: 'cpp', cc: 'cpp', cxx: 'cpp', hpp: 'cpp', cs: 'csharp', dart: 'dart',
  lua: 'lua', pl: 'perl', r: 'r', scala: 'scala', ex: 'elixir', exs: 'elixir', zig: 'zig', hs: 'haskell',
  sql: 'sql', graphql: 'graphql', gql: 'graphql', proto: 'proto', prisma: 'prisma',
  sh: 'bash', bash: 'bash', zsh: 'bash', fish: 'fish', ps1: 'powershell', psm1: 'powershell', psd1: 'powershell',
  bat: 'bat', cmd: 'bat', md: 'markdown', markdown: 'markdown', mdx: 'mdx', tex: 'latex',
  diff: 'diff', patch: 'diff', dockerfile: 'docker', tf: 'terraform', hcl: 'hcl', nix: 'nix',
  csv: 'csv', tsv: 'tsv', log: 'log',
};

// Whole file names that say more than the extension.
const BY_NAME: Record<string, string> = {
  dockerfile: 'docker', containerfile: 'docker', makefile: 'makefile', gnumakefile: 'makefile',
  '.env': 'dotenv', '.gitignore': 'ini', '.gitattributes': 'ini', '.dockerignore': 'ini', '.editorconfig': 'ini',
  '.npmrc': 'ini', '.prettierrc': 'json', '.eslintrc': 'json', '.babelrc': 'json', 'tsconfig.json': 'jsonc', 'jsconfig.json': 'jsonc',
  'cargo.lock': 'toml', '.bashrc': 'bash', '.zshrc': 'bash', '.profile': 'bash',
  'cmakelists.txt': 'cmake', gemfile: 'ruby', rakefile: 'ruby', procfile: 'yaml',
};

export function languageOf(name: string): string | undefined {
  const lower = name.toLowerCase();
  const named = BY_NAME[lower];
  if (named) return named;
  if (lower.startsWith('dockerfile.') || lower.endsWith('.dockerfile')) return 'docker';
  if (lower.startsWith('.env.') || lower.endsWith('.env')) return 'dotenv';
  const dot = lower.lastIndexOf('.');
  return dot === -1 ? undefined : LANGUAGE[lower.slice(dot + 1)];
}

const LABELS: Record<string, string> = {
  typescript: 'TypeScript', javascript: 'JavaScript', tsx: 'TSX', jsx: 'JSX', html: 'HTML', css: 'CSS', scss: 'SCSS', less: 'Less', json: 'JSON', jsonc: 'JSON with comments',
  yaml: 'YAML', toml: 'TOML', ini: 'INI', dotenv: '.env', xml: 'XML', rust: 'Rust', python: 'Python', php: 'PHP', sql: 'SQL', bash: 'Shell', powershell: 'PowerShell',
  bat: 'Batch', markdown: 'Markdown', docker: 'Dockerfile', makefile: 'Makefile', svelte: 'Svelte', vue: 'Vue', csharp: 'C#', cpp: 'C++', diff: 'Diff', graphql: 'GraphQL',
};
/** A short, readable name for a language id ("tsx" -> "TSX"). */
export const languageLabel = (lang: string | undefined) => (lang ? (LABELS[lang] ?? lang[0].toUpperCase() + lang.slice(1)) : 'Plain text');

// Pictures the viewer shows as an image, not as text. SVG too: an <img> never runs its script.
const IMAGE_MIME: Record<string, string> = {
  png: 'image/png', jpg: 'image/jpeg', jpeg: 'image/jpeg', jpe: 'image/jpeg', gif: 'image/gif', webp: 'image/webp',
  bmp: 'image/bmp', ico: 'image/x-icon', avif: 'image/avif', svg: 'image/svg+xml',
};
/** The picture type of a file name, or undefined when the file is not a picture. */
export function imageMimeOf(name: string): string | undefined {
  const dot = name.lastIndexOf('.');
  return dot === -1 ? undefined : IMAGE_MIME[name.slice(dot + 1).toLowerCase()];
}

const MEDIA_MIME: Record<string, { kind: 'audio' | 'video'; mime: string }> = {
  wav: { kind: 'audio', mime: 'audio/wav' }, mp3: { kind: 'audio', mime: 'audio/mpeg' }, ogg: { kind: 'audio', mime: 'audio/ogg' },
  oga: { kind: 'audio', mime: 'audio/ogg' }, m4a: { kind: 'audio', mime: 'audio/mp4' }, flac: { kind: 'audio', mime: 'audio/flac' },
  aac: { kind: 'audio', mime: 'audio/aac' }, opus: { kind: 'audio', mime: 'audio/ogg' }, weba: { kind: 'audio', mime: 'audio/webm' },
  mp4: { kind: 'video', mime: 'video/mp4' }, m4v: { kind: 'video', mime: 'video/mp4' }, webm: { kind: 'video', mime: 'video/webm' },
  mov: { kind: 'video', mime: 'video/quicktime' }, mkv: { kind: 'video', mime: 'video/x-matroska' },
};
/** A sound or a video by its file name (the viewer plays it), or undefined. */
export function mediaOf(name: string): { kind: 'audio' | 'video'; mime: string } | undefined {
  const dot = name.lastIndexOf('.');
  return dot === -1 ? undefined : MEDIA_MIME[name.slice(dot + 1).toLowerCase()];
}

/** A picture inside the project folder, as a data URL the page can show. */
export async function readImageUrl(root: string, path: string, mime: string): Promise<string> {
  if (!inTauri()) throw new Error('Open the desktop app to read files.');
  const { invoke } = await import('@tauri-apps/api/core');
  return `data:${mime};base64,${await invoke<string>('file_read_bytes', { root, path })}`;
}

/** Text of a file inside the project folder. The desktop app reads it; the browser preview cannot. */
export async function readProjectFile(root: string, path: string): Promise<string> {
  if (!inTauri()) throw new Error('Open the desktop app to read files.');
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<string>('file_read', { root, path });
}

export type OfficeFormat = 'docx' | 'xlsx' | 'pptx';

/** The Office Open XML formats this viewer can render without changing the source file. */
export function officeFormatOf(name: string): OfficeFormat | undefined {
  const extension = name.slice(name.lastIndexOf('.') + 1).toLowerCase();
  if (extension === 'docx') return 'docx';
  if (['xls', 'xlsx', 'xlsm'].includes(extension)) return 'xlsx';
  if (extension === 'pptx') return 'pptx';
  return undefined;
}

/** Bytes of a supported Office file inside the project folder, decoded from the desktop's bounded base64 response. */
export async function readProjectFileBytes(root: string, path: string): Promise<Uint8Array> {
  if (!inTauri()) throw new Error('Open the desktop app to read Office files.');
  const { invoke } = await import('@tauri-apps/api/core');
  const base64 = await invoke<string>('file_read_bytes', { root, path });
  const binary = atob(base64);
  return Uint8Array.from(binary, (character) => character.charCodeAt(0));
}
