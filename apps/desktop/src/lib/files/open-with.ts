import type { ZDropdownMenuItem } from '@shival99/z-ui-svelte';
import { inTauri } from '../app/platform/history';
import { toast } from '../app/platform/toast';

// "Open with": the editors and apps this machine has that can open a file (VS Code, Zed, Cursor, the system's own…),
// found by the Rust side (`file_open_options`) and started by it (`file_open_with`). The menus of a file all ask here.
export type OpenWithOption = { id: string; label: string; icon?: string | null };

// Asked once per path: the chat's virtual list rebuilds a reply's file cards each time it scrolls into view.
const known = new Map<string, Promise<OpenWithOption[]>>();

/** The apps that can open `path`; none outside the desktop app or when it cannot be found. */
export function openWithOptions(path: string): Promise<OpenWithOption[]> {
    if (!inTauri()) return Promise.resolve([]);
    let found = known.get(path);
    if (!found) {
        found = import('@tauri-apps/api/core')
            .then(({ invoke }) => invoke<OpenWithOption[]>('file_open_options', { path }))
            .catch(() => {
                known.delete(path);
                return [];
            });
        known.set(path, found);
    }
    return found;
}

/** Opens `path` in the app `appId` (from `openWithOptions`). */
export async function openWithApp(path: string, appId: string): Promise<void> {
    try {
        const { invoke } = await import('@tauri-apps/api/core');
        await invoke('file_open_with', { path, appId });
    } catch (error) {
        toast.error(String(error));
    }
}

async function chooseApp(path: string): Promise<void> {
    try {
        const { invoke } = await import('@tauri-apps/api/core');
        const appPath = await invoke<string | null>('file_pick_app', { path });
        if (!appPath) return;
        await openWithApp(path, `custom:${appPath}`);
    } catch (error) {
        toast.error(String(error));
    }
}

/** The "Open with" row and nested app choices; the selected app is remembered for this extension. */
export function openWithItem(
    path: string,
    options: OpenWithOption[],
    onOpen?: () => void,
): ZDropdownMenuItem[] {
    return [
        {
            label: 'Open with',
            icon: 'lucideExternalLink',
            iconSize: '12',
            class: 'text-sm! py-1',
            submenu: [
                ...options.map((option) => ({
                    label: option.label,
                    icon: 'lucideAppWindow',
                    iconSrc: option.icon ?? undefined,
                    class: 'text-sm! py-1',
                    onClick: () => void openWithApp(path, option.id),
                } as ZDropdownMenuItem & { iconSrc?: string })),
                {
                    label: 'Choose another app…',
                    class: 'text-sm! py-1',
                    divide: options.length > 0 ? 'before' : undefined,
                    onClick: () => void chooseApp(path),
                },
            ],
            // Menu calls this only when the user opens the nested list, not when the context menu itself opens.
            onSubmenuOpen: onOpen,
        } as ZDropdownMenuItem & { onSubmenuOpen?: () => void },
    ];
}
