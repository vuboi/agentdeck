// File paths in the text of an answer, found so the chat can make them links (markdown.ts). Free of stores and Tauri so
// plain node can test it, and it knows every platform's paths at once: it does not look at the OS of this machine, because
// an answer can name a Windows path, a macOS path and a Linux path in one sentence.
//
// A path starts at one of: `file://`, `\\?\C:\`, `\\server\share` (UNC, WSL too), `//server/share`, a drive `C:\` or `C:/`,
// `~/`, or `/` followed by a well-known root folder and one more segment (`/Users/me`, `/etc/hosts`, `/Volumes/Disk/x`).
// It must stand alone: the character before it is a space, a bracket, a quote, `=` or the start. That is what keeps out
// `and/or`, `24/7`, `TCP/IP`, `2024/10/28`, and the path part of an address (`https://host/a/b`, `localhost:3000/api`).

export type Platform = 'windows' | 'mac' | 'linux';

/** The top folders of macOS and Linux a bare `/x/y` path may start with. */
const ROOTS = new Set([
  'Users', 'Volumes', 'Applications', 'Library', 'System', 'private', 'Network', 'cores',
  'opt', 'home', 'root', 'mnt', 'media', 'etc', 'var', 'usr', 'srv', 'tmp', 'bin', 'sbin', 'lib', 'lib64', 'dev', 'proc',
  'sys', 'run', 'boot', 'snap', 'nix',
]);

/** What may stand right before a path. */
const BEFORE = /[\s([{<"'`=,;*|\u201c\u2018]/;
/** What ends a path that has no space in it (or a quoted one). */
const STOP = /[\s"'`<>|*\u201d\u2019\u00a0]/;
const TRAIL = /[.,;:!?)\]}\u2026]+$/;
const LINE = /:\d+(?::\d+)?$/;
const EXTENSION = /\.[A-Za-z0-9]{1,8}$/;

/** Whether a path starts at `at` in `src` (the character before it is not looked at here). */
export function startsAt(src: string, at: number): boolean {
  const rest = src.slice(at, at + 64);
  if (/^file:\/\/(?:\/[^\s/]|[^\s/]+\/[^\s/])/i.test(rest)) return true;
  if (/^\\\\\?\\[A-Za-z]:\\\S/.test(rest)) return true;
  if (/^\\\\[^\s\\/]+[\\/][^\s\\/]/.test(rest)) return true;
  if (/^\/\/[A-Za-z0-9_.$-]+\/[^\s/]/.test(rest)) return true;
  if (/^[A-Za-z]:[\\/]\S/.test(rest)) return true;
  if (/^~\/\S/.test(rest)) return true;
  const root = /^\/([A-Za-z0-9_]+)\/[^\s/]/.exec(rest);
  return root !== null && ROOTS.has(root[1]);
}

/** For marked's `start`: the first place a path may start in `src`, or -1. A candidate after the first character is
 *  checked against the character before it; one at 0 is left to `matchPath`, which sees the token before. */
export function pathStart(src: string): number {
  for (let at = 0; at < src.length; at++) {
    const ch = src[at];
    if (ch !== '/' && ch !== '\\' && ch !== '~' && ch !== 'f' && ch !== 'F' && !/[A-Za-z]/.test(ch)) continue;
    if (at > 0 && !BEFORE.test(src[at - 1])) continue;
    if (startsAt(src, at)) return at;
  }
  return -1;
}

export type PathSpan = {
  /** Where the span is in the text it was found in, and its raw text (the longest guess, up to the last candidate). */
  start: number;
  raw: string;
  /** The guesses, longest first: the path (as written, file URI and line suffix removed) and how many characters of `raw` it covers. */
  candidates: { path: string; length: number; line: string }[];
};

const hasExtension = (path: string) => EXTENSION.test(path.replace(LINE, '').replace(TRAIL, '').split(/[\\/]/).pop() ?? '');

/** The guesses for a path that begins `src` (whose first character starts it), or null when it is not one. `before` is
 *  the character before it ('' at the start of the text). */
export function matchPath(src: string, before = ''): PathSpan | null {
  if (before && !BEFORE.test(before)) return null;
  if (!startsAt(src, 0)) return null;
  // The token up to the first space or quote.
  let end = 0;
  while (end < src.length && !STOP.test(src[end])) end++;
  const cuts = [end];
  // A path with spaces ("C:\Users\me\My Documents\báo cáo.docx"): when the first token does not end in a file extension,
  // the words after it may be the rest. Each cut is a guess; the chat keeps the longest that exists.
  if (!hasExtension(src.slice(0, end))) {
    let at = end;
    for (let words = 0; words < 6 && src[at] === ' '; words++) {
      let next = at + 1;
      while (next < src.length && !STOP.test(src[next])) next++;
      if (next === at + 1) break;
      if (startsAt(src, at + 1) && BEFORE.test(src[at])) break; // another path begins here
      cuts.push(next);
      at = next;
      if (hasExtension(src.slice(0, next))) break;
    }
  }
  const out: PathSpan['candidates'] = [];
  const seen = new Set<string>();
  for (const cut of [...cuts].reverse()) {
    const text = src.slice(0, cut);
    // As written first (a file can end in a dot or a bracket), then without the punctuation the sentence adds.
    for (const spanEnd of new Set([text.length, text.replace(TRAIL, '').length])) {
      const span = text.slice(0, spanEnd);
      if (span.length < 3) continue;
      const line = LINE.exec(span)?.[0] ?? '';
      const path = normalizePath(line ? span.slice(0, -line.length) : span);
      const key = `${path}\u0000${span.length}`;
      if (!path || seen.has(key)) continue;
      seen.add(key);
      out.push({ path, length: span.length, line });
    }
  }
  if (!out.length) return null;
  return { start: 0, raw: src.slice(0, out[0].length), candidates: out };
}

/** A text with every file path in it replaced by the file's name (for reading aloud). */
export function pathsAsNames(text: string): string {
  let out = text;
  for (const span of [...findPaths(text)].reverse()) {
    const pick = span.candidates[span.candidates.length - 1];
    out = out.slice(0, span.start) + labelOf(pick.path) + out.slice(span.start + pick.length);
  }
  return out;
}

/** The paths of a text, left to right (for tests and tools; the chat gets them through markdown.ts). */
export function findPaths(text: string): PathSpan[] {
  const found: PathSpan[] = [];
  let at = 0;
  while (at < text.length) {
    const next = pathStart(text.slice(at));
    if (next < 0) break;
    const start = at + next;
    const span = matchPath(text.slice(start), start > 0 ? text[start - 1] : '');
    if (!span) {
      at = start + 1;
      continue;
    }
    // The shortest guess is what the chat shows first: the span ends there.
    found.push({ ...span, start });
    at = start + Math.max(...span.candidates.map((candidate) => candidate.length));
  }
  return found;
}

/** A path as a plain one: `file://` URIs turned into paths (percent-decoded), `/C:/x` into `C:/x`. */
export function normalizePath(raw: string): string {
  let path = raw;
  if (/^file:\/\//i.test(path)) {
    path = path.replace(/^file:\/\//i, '');
    if (!path.startsWith('/')) path = `//${path}`; // file://server/share/x
    else if (/^\/[A-Za-z]:[\\/]/.test(path)) path = path.slice(1); // file:///C:/x
    try {
      path = decodeURIComponent(path);
    } catch {
      // Left as written.
    }
  }
  return path;
}

/** The name a link shows: the last part of the path (the folder's own name for one), with the line it points at. */
export function labelOf(path: string, line = ''): string {
  const parts = path.replace(/[\\/]+$/, '').split(/[\\/]/).filter(Boolean);
  const name = /^[A-Za-z]:[\\/]?$/.test(path) ? path : parts.at(-1) ?? path;
  return `${name}${line}`;
}

/** `file://` address of a path: spaces and special characters percent-encoded; a drive keeps its colon. */
export function fileUrl(path: string): string {
  const plain = normalizePath(path).replace(/\\/g, '/');
  const encode = (part: string) => encodeURIComponent(part).replace(/%3A/gi, ':');
  if (/^[A-Za-z]:\//.test(plain)) return `file:///${plain.slice(0, 2)}${plain.slice(2).split('/').map(encode).join('/')}`;
  if (plain.startsWith('//')) {
    const [host, ...rest] = plain.slice(2).split('/');
    return `file://${host}/${rest.map(encode).join('/')}`;
  }
  return `file://${plain.split('/').map(encode).join('/')}`;
}

/** The system this app runs on, from the browser engine's own report. */
export function hostPlatform(): Platform {
  const agent = typeof navigator === 'undefined' ? '' : navigator.userAgent;
  return /Windows/i.test(agent) ? 'windows' : /Mac/i.test(agent) ? 'mac' : 'linux';
}

/** A path as the platform writes it: backslashes on Windows for a drive or UNC path, as it is for a POSIX one. */
export function nativePath(path: string, platform: Platform): string {
  const plain = normalizePath(path);
  if (platform === 'windows') {
    if (/^[A-Za-z]:[\\/]/.test(plain) || /^[\\/]{2}/.test(plain)) return plain.replace(/\//g, '\\');
    return plain;
  }
  // macOS and Linux: separators are slashes; a Windows path is left alone (it means nothing here).
  return /^[A-Za-z]:[\\/]/.test(plain) || /^\\\\/.test(plain) ? plain : plain.replace(/\\/g, '/');
}
