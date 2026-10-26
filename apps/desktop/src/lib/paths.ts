// The one place that reads and builds file paths. Chats run in a project folder, git reports paths from its repository
// root, agents write absolute paths or ones relative to the project, and the machine is Windows (`C:\a\b`, `\\host\x`)
// or POSIX (`/a/b`): every piece of code that opens, compares or shows a path goes through here instead of cutting
// strings by hand (which is how a path got glued onto the wrong folder).

/** A drive path (`C:\x`, `C:/x`), a UNC path (`\\host\share`) or a POSIX one (`/x`). */
export function isAbsolutePath(path: string): boolean {
  return /^[A-Za-z]:[\\/]/.test(path) || /^\\\\/.test(path) || path.startsWith('//') || (path.startsWith('/') && !path.startsWith('//'));
}

/** Windows style: a drive or a UNC path (its separator is `\`). */
const windowsStyle = (path: string) => /^[A-Za-z]:[\\/]/.test(path) || /^\\\\/.test(path);

/** `/` for every separator, none repeated (a UNC `//host` keeps its two), none at the end. */
export function slashed(path: string): string {
  const unc = /^[\\/]{2}(?![\\/])/.test(path);
  const flat = path.replace(/[\\/]+/g, '/').replace(/(.)\/$/, '$1');
  return unc ? `/${flat}` : flat;
}

/** The same place whatever the separators, the case (Windows paths are not case sensitive) or a trailing slash. */
export function samePath(a: string, b: string): boolean {
  const key = (path: string) => (windowsStyle(path) ? slashed(path).toLowerCase() : slashed(path));
  return key(a) === key(b);
}

/** `rel` below `base`, written with the separator `base` uses. A `rel` that is already a full path is left alone. */
export function joinPath(base: string, rel: string): string {
  // A leading `/` is a place on the disk only for a POSIX base; under a Windows folder it is how a tree writes `/src/a.ts`.
  if (windowsStyle(rel) || (rel.startsWith('/') && !windowsStyle(base) && !rel.startsWith('//'))) return rel;
  const sep = windowsStyle(base) ? '\\' : '/';
  const head = base.replace(/[\\/]+$/, '') || (sep === '/' ? '' : base);
  const tail = rel.replace(/^(\.[\\/])+/, '').replace(/^[\\/]+/, '').replace(/[\\/]+/g, sep);
  return `${head}${sep}${tail}`;
}

/**
 * `path` below `root` as a `/` path, when it is inside it (any case on Windows); else `path` unchanged, so a file outside
 * the folder keeps showing its full path.
 */
export function relativeTo(root: string, path: string): string {
  const inside = withinRoot(root, path);
  return inside ?? path.replace(/\\/g, '/').replace(/\/+$/, '');
}

/**
 * The part of `path` inside `root` (a `/` path), or null when it is not inside it, is the folder itself, or climbs out
 * with `..`. A relative `path` counts as being under `root`.
 */
export function withinRoot(root: string, path: string): string | null {
  const windows = windowsStyle(root);
  const folded = (text: string) => (windows ? text.toLowerCase() : text);
  const base = slashed(root);
  const full = slashed(path);
  let rest: string;
  if (isAbsolutePath(path)) {
    if (!folded(full).startsWith(`${folded(base)}/`)) return null;
    rest = full.slice(base.length + 1);
  } else {
    rest = full.replace(/^(\.\/)+/, '').replace(/^\/+/, '');
  }
  return !rest || rest.split('/').some((part) => part === '..') ? null : rest;
}

/** The full path of a file an agent or a link names: absolute as it is, relative as inside the project folder. */
export function resolveInProject(path: string, projectRoot: string): string {
  return isAbsolutePath(path) ? path : projectRoot ? joinPath(projectRoot, path) : path;
}

/** The folder a file is read from: the project when the file is inside it, else the file's own folder. */
export function fileRootOf(path: string, projectRoot: string): string {
  return projectRoot && withinRoot(projectRoot, path) !== null ? projectRoot : dirOf(path);
}

/** The folder part of a path, in the path's own separators (no trailing one). */
export function dirOf(path: string): string {
  return path.replace(/[\\/][^\\/]*$/, '');
}

/** The last part of a path. */
export function nameOf(path: string): string {
  return path.slice(Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\')) + 1);
}
