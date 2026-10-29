import { invoke } from '@tauri-apps/api/core';
import { zModalService } from '@shival99/z-ui-svelte';
import { checkUpdates, updates, store } from './store.svelte';
import { loadMcp, mcp } from '../agents/mcp.svelte';
import { inTauri } from '../app/platform/history';
import { toast } from '../app/platform/toast';

export type SourceEdit = { kind: 'skill'; name: string } | { kind: 'mcp'; id: string; name: string; transport: string };
export type McpUpdate = { id: string; current: string; latest: string | null; changed: boolean; error: string | null };
/** Update discovery is read-only and cached for the Store's current app session. */
export const maintenance = $state({ mcp: [] as McpUpdate[], checking: false, checkedAt: 0, busy: '' });

let recheck = false;

/** Check on opening Store and every thirty minutes while it stays mounted; never install automatically. */
export async function checkStoreUpdates(force = false) {
  if (maintenance.checking) { recheck ||= force; return; }
  if (!inTauri() || (!force && Date.now() - maintenance.checkedAt < 30 * 60_000)) return;
  maintenance.checking = true;
  try {
    const [, found] = await Promise.all([checkUpdates(), invoke<McpUpdate[]>('store_mcp_updates')]);
    maintenance.mcp = found;
    maintenance.checkedAt = Date.now();
  } catch (error) { toast.error(String(error)); }
  finally {
    maintenance.checking = false;
    if (recheck) { recheck = false; void checkStoreUpdates(true); }
  }
}

/** Recheck the registry and update the MCP package pin only after the user confirms. */
export function updateMcp(item: McpUpdate, name: string) {
  zModalService.show({
    zTitle: `Update ${name}?`, zContent: `Change the npm package from ${item.current} to ${item.latest}. This applies to new provider sessions.`, zConfirmStyle: 'dialog', zOkText: 'Update', zCancelText: 'Cancel',
    onOk: async () => {
      maintenance.busy = item.id;
      try {
        await invoke('store_mcp_update', { id: item.id });
        maintenance.mcp = maintenance.mcp.filter((old) => old.id !== item.id);
        await loadMcp();
        toast.success(`${name} updated`);
        void checkStoreUpdates(true);
        return true;
      } catch (error) { toast.error(String(error)); return false; }
      finally { maintenance.busy = ''; }
    },
  });
}

/** Combined badge count is a notification, not an instruction to auto-update. */
export function availableUpdateCount() {
  return updates.items.filter((item) => item.changed && !item.error && store.skills.some((skill) => skill.name === item.name && skill.origin === 'library')).length + maintenance.mcp.filter((item) => item.changed && !item.error && mcp.overview?.entries.some((entry) => entry.id === item.id)).length;
}
