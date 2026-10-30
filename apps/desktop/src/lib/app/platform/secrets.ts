// Provider API keys and the Console sign-in live in the system credential store (Windows Credential Manager), not in
// the app's database. The database keeps the provider's record without them; a key is read back when the app starts.
// Off Windows, or when the store is not reachable, the caller keeps the value where it was (a fallback, not a leak).

import { inTauri } from './history';

const PREFIX = 'agentdeck:';

/** Saves `value` under `name`; an empty value removes it. Returns false when the store could not take it. */
export async function saveSecret(name: string, value: string): Promise<boolean> {
  if (!inTauri()) return false;
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    if (value) await invoke('secret_save', { target: PREFIX + name, value });
    else await invoke('secret_delete', { target: PREFIX + name });
    return true;
  } catch {
    return false;
  }
}

/** The value saved under `name`, or null. */
export async function loadSecret(name: string): Promise<string | null> {
  if (!inTauri()) return null;
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    return (await invoke<string | null>('secret_load', { target: PREFIX + name })) ?? null;
  } catch {
    return null;
  }
}
