// What the file viewer last knew about a file, kept outside the viewer. Only the active tab's viewer is mounted, so
// going to another tab and back builds the viewer again: with these, it opens on the last diff and the last text at
// once while git and the disk are read again, instead of an empty area until they answer.
import { parseDiff, type DiffRow } from './diff';

/** Same file, whatever the slashes or the case of the drive letter. */
export const fileKey = (root: string, path: string) => `${root.replace(/\\/g, '/').toLowerCase()}\0${path.replace(/\\/g, '/').toLowerCase()}`;

/** A map that forgets the entry used longest ago once it holds `max`. */
export class Recent<T> {
  private readonly items = new Map<string, T>();
  private readonly max: number;
  constructor(max: number) {
    this.max = max;
  }
  get(key: string): T | undefined {
    const value = this.items.get(key);
    if (value !== undefined) {
      this.items.delete(key);
      this.items.set(key, value);
    }
    return value;
  }
  set(key: string, value: T) {
    this.items.delete(key);
    this.items.set(key, value);
    while (this.items.size > this.max) this.items.delete(this.items.keys().next().value as string);
  }
  delete(key: string) {
    this.items.delete(key);
  }
  get size() {
    return this.items.size;
  }
}

/** The change of a file as git last gave it: the raw diff (to tell whether a new answer differs) and its rows. */
export type CachedDiff = { diff: string; rows: DiffRow[] };

export const diffCache = new Recent<CachedDiff>(24);
const diffPreloads = new Map<string, Promise<CachedDiff | undefined>>();
const diffKeys = new Set<string>();
let diffEpoch = 0;

/** Invalidates both completed and in-flight reads after an agent or editor writes to the project. */
export function invalidateDiffCache() {
  diffEpoch++;
  // Delete through the stable cache API rather than relying on a newer Recent method during Vite HMR.
  for (const key of diffKeys) diffCache.delete(key);
  diffKeys.clear();
  diffPreloads.clear();
}

/** Starts a background Git read once per file, so opening Changes can reuse work already in flight. */
export function preloadDiff(key: string, root: string, path: string, force = false): Promise<CachedDiff | undefined> {
  const cached = force ? undefined : diffCache.get(key);
  if (cached) return Promise.resolve(cached);
  const running = diffPreloads.get(key);
  if (running) return running;
  const epoch = diffEpoch;
  const promise = import('@tauri-apps/api/core')
    .then(({ invoke }) => invoke<string>('change_diff', { root, path }))
    .then((diff) => {
      const value = { diff, rows: parseDiff(diff) };
      if (epoch === diffEpoch) {
        diffCache.set(key, value);
        diffKeys.add(key);
        return value;
      }
      return undefined;
    })
    .catch(() => undefined)
    .finally(() => {
      if (diffPreloads.get(key) === promise) diffPreloads.delete(key);
    });
  diffPreloads.set(key, promise);
  return promise;
}
// A big file is not worth holding: it opens in the read-only view, which reads it again anyway.
export const MAX_CACHED_TEXT = 512 * 1024;
export const textCache = new Recent<string>(12);

export function rememberText(key: string, raw: string) {
  if (raw.length <= MAX_CACHED_TEXT) textCache.set(key, raw);
  else textCache.delete(key);
}
