// The keyboard shortcuts the app really has, for the Ctrl+/ list (KeyboardShortcuts.svelte). Each entry names where
// the key is handled; keep it in step when a shortcut is added or changed. `Mod` is Ctrl (Cmd on macOS).

/** `keys`: the combinations that do it; `then`: they are pressed one after the other (else: any one of them). */
export type Shortcut = { label: string; keys: string[][]; then?: boolean };
export type ShortcutGroup = { title: string; items: Shortcut[] };

const isMac = typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform);
/** One key combination as the key caps to show: `Mod+Shift+T` → ['Ctrl', 'Shift', 'T']. */
const k = (combo: string) => combo.split('+').map((part) => (part === 'Mod' ? (isMac ? '⌘' : 'Ctrl') : part));

export const SHORTCUT_GROUPS: ShortcutGroup[] = [
  {
    title: 'General',
    items: [
      { label: 'Find in the app', keys: [k('Mod+F')] }, // FindBar.svelte (a file tab has its own find)
      { label: 'Keyboard shortcuts', keys: [k('Mod+/')] }, // +page.svelte
      { label: 'Search chats', keys: [k('Mod+P')] }, // Sidebar.svelte
      { label: 'Settings', keys: [k('Mod+,')] }, // +page.svelte
      { label: 'Toggle sidebar', keys: [k('Mod+B')] }, // +page.svelte
    ],
  },
  {
    title: 'Tabs',
    items: [
      { label: 'New chat tab', keys: [k('Mod+T')] }, // content-tabs.svelte.ts
      { label: 'Close tab', keys: [k('Mod+W')] },
      { label: 'Reopen closed tab', keys: [k('Mod+Shift+T')] },
      { label: 'Close all tabs', keys: [k('Mod+K'), k('Mod+W')], then: true },
      { label: 'Close other tabs', keys: [k('Mod+K'), k('Mod+O')], then: true },
    ],
  },
  {
    title: 'Message box',
    items: [
      { label: 'Send (queued while the agent works)', keys: [k('Enter')] }, // Composer.svelte
      { label: 'New line', keys: [k('Shift+Enter')] },
      { label: 'Send next, right after the current answer', keys: [k('Alt+Enter')] },
      { label: 'Stop the agent and send', keys: [k('Mod+Enter')] },
    ],
  },
  {
    title: 'Files',
    items: [
      { label: 'Save', keys: [k('Mod+S')] }, // FileView.svelte
      { label: 'Find in the file', keys: [k('Mod+F')] },
      { label: 'Undo / redo', keys: [k('Mod+Z'), k('Mod+Shift+Z')] },
    ],
  },
  {
    title: 'Terminal',
    items: [
      { label: 'Show or hide the terminal', keys: [k('Mod+`'), k('Mod+J')] }, // terminal.svelte.ts
      { label: 'New terminal', keys: [k('Mod+Shift+`')] },
      { label: 'Close terminal', keys: [k('Mod+Shift+W')] },
      { label: 'Next / previous terminal', keys: [k('Mod+PageDown'), k('Mod+PageUp')] },
    ],
  },
  {
    title: 'Browser',
    items: [{ label: 'Go to the address bar', keys: [k('Mod+L')] }], // BrowserView.svelte
  },
  {
    title: 'Chat menu (while it is open)',
    items: [
      { label: 'Pin / unpin', keys: [k('P')] }, // Sidebar.svelte
      { label: 'Rename', keys: [k('R')] },
      { label: 'Fork', keys: [k('F')] },
      { label: 'Delete', keys: [k('D')] },
    ],
  },
];
