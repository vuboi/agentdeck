import { untrack } from 'svelte';
import { workspace } from './sessions.svelte';
import { getPref, setPref } from './prefs';

// Browser-style tabs over the conversation area. A tab is either a chat (one session) or a file.
// Chats and files share one list; `active` is a tab id, or '' when every tab is closed.
export type ContentTab =
  | { id: string; kind: 'chat'; sessionId: string }
  | { id: string; kind: 'file'; name: string; path: string; root: string; /** The file as it is, or what changed in it. */ view?: 'file' | 'changes' };

export const contentTabs = $state({
  active: '' as string,
  tabs: [] as ContentTab[],
});

// Open tabs survive a reload. Restored once, then saved on every change.
const ready = $state({ tabs: false });
let tabsHydrated = false;
getPref<{ active: string; tabs: ContentTab[] }>('content.tabs').then((saved) => {
  if (saved) {
    contentTabs.tabs = saved.tabs ?? [];
    contentTabs.active = saved.active ?? '';
  }
  tabsHydrated = true;
  ready.tabs = true;
});
$effect.root(() => {
  $effect(() => {
    const snapshot = { active: contentTabs.active, tabs: contentTabs.tabs.map((tab) => ({ ...tab })) };
    if (tabsHydrated) setPref('content.tabs', snapshot);
  });
});

const chatId = (sessionId: string) => `chat:${sessionId}`;

// The open session wins: after a reload the saved tab and the saved session can disagree, and the chat you were
// reading must be the one on screen. A file tab in front is left alone.
$effect.root(() => {
  $effect(() => {
    const id = workspace.activeId;
    if (!ready.tabs || !id) return;
    untrack(() => {
      const current = contentTabs.tabs.find((tab) => tab.id === contentTabs.active);
      if (current?.kind === 'file' || contentTabs.active === chatId(id)) return;
      if (!contentTabs.tabs.some((tab) => tab.id === chatId(id))) {
        const index = contentTabs.tabs.findIndex((tab) => tab.id === contentTabs.active);
        if (index !== -1) contentTabs.tabs[index] = { id: chatId(id), kind: 'chat', sessionId: id };
        else contentTabs.tabs.push({ id: chatId(id), kind: 'chat', sessionId: id });
      }
      contentTabs.active = chatId(id);
    });
  });
});

/** Open (or focus) the chat tab of a session and make it the session on screen. */
export function openChatTab(sessionId: string) {
  if (!contentTabs.tabs.some((tab) => tab.id === chatId(sessionId))) {
    contentTabs.tabs.push({ id: chatId(sessionId), kind: 'chat', sessionId });
  }
  contentTabs.active = chatId(sessionId);
  workspace.activeId = sessionId;
}

/** Sidebar pick: the session takes the place of the active chat tab, so tabs are swapped, not added.
 *  A session that already has a tab is just focused. */
export function showSession(sessionId: string) {
  const id = chatId(sessionId);
  if (!contentTabs.tabs.some((tab) => tab.id === id)) {
    const index = contentTabs.tabs.findIndex((tab) => tab.id === contentTabs.active && tab.kind === 'chat');
    if (index !== -1) contentTabs.tabs[index] = { id, kind: 'chat', sessionId };
    else contentTabs.tabs.push({ id, kind: 'chat', sessionId });
  }
  contentTabs.active = id;
  workspace.activeId = sessionId;
}

/** Open a file as a tab. `path` is absolute; `root` is the project folder it must stay inside. */
export function openFileTab(path: string, root: string, view?: 'file' | 'changes') {
  const name = path.split(/[\\/]/).pop() ?? path;
  const open = contentTabs.tabs.find((tab) => tab.id === path);
  if (!open) contentTabs.tabs.push({ id: path, kind: 'file', name, path, root, view });
  else if (view && open.kind === 'file') open.view = view;
  contentTabs.active = path;
}

export function activateTab(id: string) {
  const tab = contentTabs.tabs.find((item) => item.id === id);
  if (!tab) return;
  contentTabs.active = id;
  if (tab.kind === 'chat') workspace.activeId = tab.sessionId;
}

/** Close a tab. Closing a chat tab only hides it: the session stays in the sidebar. */
/** Tabs closed lately, the newest last: Ctrl+Shift+T brings the newest back. */
const closed: ContentTab[] = [];
function remember(tabs: ContentTab[]) {
  for (const tab of tabs) {
    const at = closed.findIndex((item) => item.id === tab.id);
    if (at !== -1) closed.splice(at, 1);
    closed.push({ ...tab });
  }
  if (closed.length > 20) closed.splice(0, closed.length - 20);
}

/** Opens the tab closed last again (a chat that was deleted since is skipped). Answers whether one came back. */
export function reopenTab(): boolean {
  while (closed.length > 0) {
    const tab = closed.pop()!;
    if (contentTabs.tabs.some((item) => item.id === tab.id)) continue;
    if (tab.kind === 'chat') {
      if (!workspace.sessions.some((item) => item.id === tab.sessionId)) continue;
      openChatTab(tab.sessionId);
    } else {
      openFileTab(tab.path, tab.root);
    }
    return true;
  }
  return false;
}

export function closeTab(id: string) {
  const index = contentTabs.tabs.findIndex((tab) => tab.id === id);
  if (index === -1) return;
  remember([contentTabs.tabs[index]]);
  contentTabs.tabs.splice(index, 1);
  if (contentTabs.active === id) {
    // The tab on the left, or the new first tab, becomes active; with no tabs left the area is empty.
    const next = contentTabs.tabs[Math.max(0, index - 1)];
    contentTabs.active = next?.id ?? '';
    if (next?.kind === 'chat') workspace.activeId = next.sessionId;
  }
}

/** The folder a file is read from: the project when the file is inside it, else the file's own folder. */
export function fileRootOf(path: string, projectRoot: string): string {
  const norm = (text: string) => text.replace(/\\/g, '/').toLowerCase();
  return projectRoot && norm(path).startsWith(`${norm(projectRoot)}/`) ? projectRoot : path.replace(/[\\/][^\\/]*$/, '');
}

/** Closes every open tab. The chats themselves stay in the sidebar. */
export function closeAllTabs() {
  remember(contentTabs.tabs);
  contentTabs.tabs.splice(0, contentTabs.tabs.length);
  contentTabs.active = '';
}

/** Closes every tab except `id`, which becomes the active one. */
export function closeOtherTabs(id: string) {
  const keep = contentTabs.tabs.find((tab) => tab.id === id);
  if (!keep) return;
  remember(contentTabs.tabs.filter((tab) => tab.id !== id));
  contentTabs.tabs.splice(0, contentTabs.tabs.length, keep);
  activateTab(id);
}

/** Ctrl+K then Ctrl+W closes all tabs, Ctrl+K then Ctrl+O closes the other tabs (within a moment), as in VS Code. */
export function installTabShortcuts() {
  let waiting = 0;
  const onKey = (event: KeyboardEvent) => {
    const mod = event.ctrlKey || event.metaKey;
    const key = event.key.toLowerCase();
    // Ctrl+Shift+T: the tab closed last comes back, as in a browser.
    if (mod && event.shiftKey && !event.altKey && key === 't') {
      event.preventDefault();
      reopenTab();
      return;
    }
    if (mod && !event.shiftKey && !event.altKey && key === 'k') {
      waiting = Date.now();
      return;
    }
    if (waiting && Date.now() - waiting < 1500 && mod && !event.shiftKey && !event.altKey && (key === 'w' || key === 'o')) {
      event.preventDefault();
      const active = contentTabs.active;
      waiting = 0;
      if (key === 'w') closeAllTabs();
      else if (active) closeOtherTabs(active);
    }
  };
  window.addEventListener('keydown', onKey);
  return () => window.removeEventListener('keydown', onKey);
}
