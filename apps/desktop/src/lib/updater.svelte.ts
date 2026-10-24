// App updates: ask the release feed (GitHub latest.json) if a newer signed build exists, download it with progress,
// install it and restart. Only works inside the packaged desktop app; in a browser or dev it stays idle.
import { check, type Update } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';

export type UpdateStatus = 'idle' | 'checking' | 'available' | 'downloading' | 'ready' | 'error';

export const updater = $state({
  status: 'idle' as UpdateStatus,
  version: '',
  notes: '',
  /** 0..1 while downloading; stays 0 when the size is unknown. */
  progress: 0,
  error: '',
});

let pending: Update | null = null;
const inTauri = () => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

/** Look for an update. `quiet` keeps a failed check silent (the background checks). Returns true if one is waiting. */
export async function checkForUpdate(quiet = false): Promise<boolean> {
  if (!inTauri()) return false;
  if (updater.status === 'available' || updater.status === 'ready') return true;
  if (updater.status === 'checking' || updater.status === 'downloading') return false;
  updater.status = 'checking';
  updater.error = '';
  try {
    const update = await check();
    if (!update) {
      updater.status = 'idle';
      return false;
    }
    pending = update;
    updater.version = update.version;
    updater.notes = update.body ?? '';
    updater.status = 'available';
    return true;
  } catch (error) {
    // No release feed yet (nothing published, or the repository is private): not a failure worth a red toast.
    const none = /valid release|404|not found/i.test(String(error));
    updater.status = quiet || none ? 'idle' : 'error';
    updater.error = none ? 'none' : String(error);
    return false;
  }
}

/** Download and install the waiting update, then restart into it. */
export async function installUpdate() {
  if (!pending || updater.status === 'downloading') return;
  updater.status = 'downloading';
  updater.progress = 0;
  updater.error = '';
  let total = 0;
  let done = 0;
  try {
    await pending.downloadAndInstall((event) => {
      if (event.event === 'Started') total = event.data.contentLength ?? 0;
      else if (event.event === 'Progress') {
        done += event.data.chunkLength;
        if (total > 0) updater.progress = Math.min(done / total, 1);
      }
    });
    updater.status = 'ready';
    await relaunch();
  } catch (error) {
    updater.status = 'error';
    updater.error = String(error);
  }
}

/** Check shortly after start, then every six hours. */
export function startUpdateChecks() {
  if (!inTauri()) return () => {};
  const first = setTimeout(() => void checkForUpdate(true), 8000);
  const every = setInterval(() => void checkForUpdate(true), 6 * 60 * 60 * 1000);
  return () => {
    clearTimeout(first);
    clearInterval(every);
  };
}
