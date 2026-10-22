import { untrack } from 'svelte';
import { workspace } from './sessions.svelte';
import { getPref, setPref } from './prefs';

// Browser-style tabs over the conversation area. A tab is either a chat (one session) or a file.
// Chats and files share one list; `active` is a tab id, or '' when every tab is closed.
export type ContentTab =
  | { id: string; kind: 'chat'; sessionId: string }
  | { id: string; kind: 'file'; name: string; path: string; root: string };

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
export function openFileTab(path: string, root: string) {
  const name = path.split(/[\\/]/).pop() ?? path;
  if (!contentTabs.tabs.some((tab) => tab.id === path)) {
    contentTabs.tabs.push({ id: path, kind: 'file', name, path, root });
  }
  contentTabs.active = path;
}

export function activateTab(id: string) {
  const tab = contentTabs.tabs.find((item) => item.id === id);
  if (!tab) return;
  contentTabs.active = id;
  if (tab.kind === 'chat') workspace.activeId = tab.sessionId;
}

/** Close a tab. Closing a chat tab only hides it: the session stays in the sidebar. */
export function closeTab(id: string) {
  const index = contentTabs.tabs.findIndex((tab) => tab.id === id);
  if (index === -1) return;
  contentTabs.tabs.splice(index, 1);
  if (contentTabs.active === id) {
    // The tab on the left, or the new first tab, becomes active; with no tabs left the area is empty.
    const next = contentTabs.tabs[Math.max(0, index - 1)];
    contentTabs.active = next?.id ?? '';
    if (next?.kind === 'chat') workspace.activeId = next.sessionId;
  }
}
