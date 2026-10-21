import { inTauri } from './history';

// Extension -> language the code viewer can highlight. Anything else is shown as plain text.
const LANGUAGE: Record<string, string> = {
  ts: 'typescript', mts: 'typescript', cts: 'typescript',
  js: 'javascript', mjs: 'javascript', cjs: 'javascript',
  tsx: 'tsx', jsx: 'jsx', html: 'html', htm: 'html',
  css: 'css', scss: 'scss', json: 'json',
  sh: 'bash', bash: 'bash', zsh: 'bash',
  md: 'markdown', yml: 'yaml', yaml: 'yaml', toml: 'toml',
  py: 'python', java: 'java', sql: 'sql', xml: 'xml', svg: 'xml',
  diff: 'diff', patch: 'diff', svelte: 'svelte', rs: 'rust',
};

export function languageOf(name: string): string | undefined {
  const ext = name.split('.').pop()?.toLowerCase() ?? '';
  return LANGUAGE[ext];
}

/** Text of a file inside the project folder. The desktop app reads it; the browser preview cannot. */
export async function readProjectFile(root: string, path: string): Promise<string> {
  if (!inTauri()) throw new Error('Open the desktop app to read files.');
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<string>('file_read', { root, path });
}
