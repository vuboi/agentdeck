import { inTauri } from '../app/platform/history';
import type { Permission } from '../chat/sessions.svelte';

// Antigravity's permission modes (`agy --mode ...`) depend on the account: agy answers "--mode auto is not available
// for this account" and quietly runs in its default mode, so choosing Auto-approve in the picker would change nothing.
// Rust (`agy_modes`) finds out once per agy version; the picker greys out what is not there.

/** The picker's setting and the `--mode` it becomes. The others (ask, YOLO) are not modes of agy. */
const AGY_MODE: Partial<Record<Permission, string>> = { auto: 'auto', edits: 'accept-edits', plan: 'plan' };

export const agyModes = $state({ loaded: false, loading: false, unavailable: [] as string[] });

/** Whether Antigravity cannot run `permission` on this account. False until it is known. */
export const agyBlocks = (permission: Permission) => {
  const mode = AGY_MODE[permission];
  return Boolean(mode && agyModes.unavailable.includes(mode));
};

export async function loadAgyModes(force = false) {
  if (!inTauri() || (agyModes.loaded && !force) || agyModes.loading) return;
  if (force) {
    agyModes.loaded = false;
    agyModes.unavailable = [];
  }
  agyModes.loading = true;
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    const found = await invoke<{ unavailable: string[] }>('agy_modes', { force });
    agyModes.unavailable = found.unavailable;
    agyModes.loaded = true;
  } catch {
    // Unknown: nothing is greyed out.
  } finally {
    agyModes.loading = false;
  }
}
