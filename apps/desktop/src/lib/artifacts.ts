// Files an agent made or names in its answer (a sound, a picture, a video, a document), found in the text of a reply so
// the chat can show them as cards. Free of stores and Tauri so plain node can test it. It works from the words and the
// Write calls of a reply, so it is the same for every agent and every provider.

export type ArtifactKind = 'audio' | 'video' | 'image' | 'document';
export type Artifact = { path: string; name: string; kind: ArtifactKind };

const AUDIO = ['wav', 'mp3', 'ogg', 'oga', 'm4a', 'flac', 'aac', 'opus', 'weba'];
const VIDEO = ['mp4', 'webm', 'mov', 'mkv', 'm4v', 'avi'];
const IMAGE = ['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp'];
const DOCUMENT = ['pdf', 'docx', 'xlsx', 'pptx', 'csv', 'zip', 'doc', 'xls', 'ppt', 'odt', 'ods', 'odp', 'epub', 'svg'];

const ext = (name: string) => (name.includes('.') ? name.split('.').pop()!.toLowerCase() : '');

export function kindOfName(name: string): ArtifactKind | null {
  const e = ext(name);
  if (AUDIO.includes(e)) return 'audio';
  if (VIDEO.includes(e)) return 'video';
  if (IMAGE.includes(e)) return 'image';
  if (DOCUMENT.includes(e)) return 'document';
  return null;
}

export const nameOfPath = (path: string) => path.replace(/[\\/]+$/, '').split(/[\\/]/).pop() ?? path;
const isAbsolute = (path: string) => /^[A-Za-z]:[\\/]/.test(path) || path.startsWith('/') || path.startsWith('\\\\');

/** A relative path made absolute under `root`, with the separator `root` uses; an absolute one stays as it is. */
export function resolveArtifactPath(path: string, root: string | null): string {
  // `/C:/Users/…` (a markdown link written URL-style, as Codex does) is the Windows path `C:/Users/…`.
  const clean = path.replace(/^file:\/\/\/?/, '').replace(/^\/([A-Za-z]:[\\/])/, '$1').replace(/^\.[\\/]/, '');
  if (isAbsolute(clean) || !root) return clean;
  const sep = root.includes('\\') ? '\\' : '/';
  return `${root.replace(/[\\/]+$/, '')}${sep}${clean.replace(/[\\/]/g, sep)}`;
}

const EXTENSIONS = [...AUDIO, ...VIDEO, ...IMAGE, ...DOCUMENT].join('|');
// A path ends at a known extension: absolute Windows or POSIX paths (spaces allowed inside when quoted or in a link),
// or a relative one without spaces. Remote addresses are not files.
const QUOTED = new RegExp(String.raw`(?:\`|"|')((?:[A-Za-z]:[\\/]|/|\.{0,2}[\\/]|[\w.-]+[\\/])[^\`"'\n]*?\.(?:${EXTENSIONS}))(?:\`|"|')`, 'gi');
const LINK = new RegExp(String.raw`\]\(\s*<?([^)>\s][^)>\n]*?\.(?:${EXTENSIONS}))>?\s*\)`, 'gi');
const BARE = new RegExp(String.raw`(?<![\w:/\\.-])((?:[A-Za-z]:[\\/]|/)?(?:[\w.@+-]+[\\/])*[\w.@+-]+\.(?:${EXTENSIONS}))(?![\w-])`, 'gi');

/** The file paths a text names, in order, each once (remote addresses and fenced code are skipped). */
export function pathsInText(text: string): string[] {
  const noCode = text.replace(/```[\s\S]*?```/g, ' ');
  const found: string[] = [];
  const add = (raw: string) => {
    const path = raw.trim().replace(/[.,;:!?)\]]+$/, '');
    if (!path || /^[a-z][a-z0-9+.-]*:\/\//i.test(path) && !path.startsWith('file://')) return;
    if (!found.includes(path)) found.push(path);
  };
  for (const pattern of [LINK, QUOTED, BARE]) {
    pattern.lastIndex = 0;
    for (const match of noCode.matchAll(pattern)) add(match[1]);
  }
  return found;
}

type BlockLike = { kind: string; text?: string; command?: string };
const WRITE_TOOL = /^(Write|NotebookEdit|apply_patch)$/;
const toolOf = (command: string) => (command.split(/\s+/, 1)[0] ?? '').replace(/^mcp__.*__/, '');

/**
 * The files of a reply: paths in its words and in the files its Write calls name, as cards. Relative paths are read
 * under `root` (the project folder). At most `max`, the ones the reply names last first, so a finished file comes up
 * above the drafts of an earlier step.
 */
export function findArtifacts(blocks: BlockLike[], root: string | null, max = 8): Artifact[] {
  const raw: string[] = [];
  for (const block of blocks) {
    if (block.kind === 'response' && block.text) raw.push(...pathsInText(block.text));
    else if (block.kind === 'command' && block.command && WRITE_TOOL.test(toolOf(block.command))) {
      const first = block.command.slice(toolOf(block.command).length).trim().split(/\s+/, 1)[0]?.replace(/^["'`]|["'`,]$/g, '') ?? '';
      if (kindOfName(first)) raw.push(first);
    }
  }
  const seen = new Set<string>();
  const out: Artifact[] = [];
  for (const path of raw) {
    const kind = kindOfName(path);
    if (!kind) continue;
    const full = resolveArtifactPath(path, root);
    const key = full.toLowerCase().replace(/\\/g, '/');
    if (seen.has(key)) continue;
    seen.add(key);
    out.push({ path: full, name: nameOfPath(full), kind });
  }
  return out.slice(-max);
}

/** Whether a reply wrote or named an image only to say it is a picture it was shown (a link in a Markdown image). */
export const isMediaKind = (kind: ArtifactKind) => kind === 'audio' || kind === 'video' || kind === 'image';

/** "5.4 s" as a clock: 0:05, 12:03. */
export function clock(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds < 0) return '0:00';
  const whole = Math.floor(seconds);
  return `${Math.floor(whole / 60)}:${String(whole % 60).padStart(2, '0')}`;
}
