import { untrack } from 'svelte';
import { activeSession, history, newSession, NO_PROJECT, pendingProject, watchDroppedDrafts, workspace } from './sessions.svelte';
import { samePath } from './paths';
import { getPref, setPref } from './prefs';
import { fileWrites } from './activity.svelte';
import { drafts } from './file-buffers';
import { fileKey, rememberText } from './file-cache';
import { joinFileText, splitFileText } from './file-edit';
import { inTauri } from './history';
import { toast } from './toast';

// Browser-style tabs over the conversation area. A tab is either a chat (one session) or a file.
// Chats and files share one list; `active` is a tab id, or '' when every tab is closed.
export type ContentTab =
  | { id: string; kind: 'chat'; sessionId: string }
  | { id: string; kind: 'file'; name: string; path: string; root: string; /** The file as it is, or what changed in it. */ view?: 'file' | 'changes'; /** Edited in the viewer and not saved yet. Never persisted. */ dirty?: boolean };

export const contentTabs = $state({
  active: '' as string,
  tabs: [] as ContentTab[],
});

// Open tabs survive a reload. Restored once, then saved on every change.
const ready = $state({ tabs: false });
let tabsHydrated = false;
getPref<{ active: string; tabs: ContentTab[] }>('content.tabs').then((saved) => {
  if (saved) {
    contentTabs.tabs = (saved.tabs ?? []).map((tab) => (tab.kind === 'file' ? { ...tab, dirty: undefined } : tab));
    contentTabs.active = saved.active ?? '';
  }
  tabsHydrated = true;
  ready.tabs = true;
});
$effect.root(() => {
  $effect(() => {
    const snapshot = { active: contentTabs.active, tabs: contentTabs.tabs.map((tab) => (tab.kind === 'file' ? { ...tab, dirty: undefined } : { ...tab })) };
    if (tabsHydrated) setPref('content.tabs', snapshot);
  });
});

const chatId = (sessionId: string) => `chat:${sessionId}`;

// A chat tab whose session is gone (an unsent new session that was dropped, a deleted chat) is closed, once the
// real chat list is known: before that every saved tab would look orphaned.
$effect.root(() => {
  $effect(() => {
    if (!ready.tabs || (!history.imported && history.loading)) return;
    const ids = new Set(workspace.sessions.map((item) => item.id));
    untrack(() => {
      const kept = contentTabs.tabs.filter((tab) => tab.kind !== 'chat' || ids.has(tab.sessionId));
      if (kept.length === contentTabs.tabs.length) return;
      contentTabs.tabs = kept;
      if (!kept.some((tab) => tab.id === contentTabs.active)) contentTabs.active = '';
    });
  });
});

// The open session wins: after a reload the saved tab and the saved session can disagree, and the chat you were
// reading must be the one on screen. A file tab in front is left alone.
$effect.root(() => {
  $effect(() => {
    const id = workspace.activeId;
    // A new session nobody wrote in yet gets no tab: the area shows it with no tab selected. Its first message
    // (the session stops being an empty draft) runs this again and opens its tab.
    const unsent = Boolean(pendingProject());
    if (!ready.tabs || !id) return;
    untrack(() => {
      if (unsent) {
        // Unless it was opened in a tab of its own (the + of the tab bar): then that tab is the one selected.
        contentTabs.active = contentTabs.tabs.some((tab) => tab.id === chatId(id)) ? chatId(id) : '';
        return;
      }
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

// An unsent new session that is dropped (another one took its place) leaves no tab behind.
watchDroppedDrafts((ids) => {
  const gone = new Set(ids.map(chatId));
  const keep = contentTabs.tabs.filter((tab) => !gone.has(tab.id));
  if (keep.length === contentTabs.tabs.length) return;
  contentTabs.tabs = keep;
  if (gone.has(contentTabs.active)) contentTabs.active = keep.find((tab) => tab.kind === 'chat' && tab.sessionId === workspace.activeId)?.id ?? keep[0]?.id ?? '';
});

/** Open (or focus) the chat tab of a session and make it the session on screen. */
export function openChatTab(sessionId: string) {
  if (!contentTabs.tabs.some((tab) => tab.id === chatId(sessionId))) {
    contentTabs.tabs.push({ id: chatId(sessionId), kind: 'chat', sessionId });
  }
  contentTabs.active = chatId(sessionId);
  workspace.activeId = sessionId;
}

/** Starts or reopens the one pending chat and focuses its composer in the current project context. */
export function openNewChatTab(projectId?: string) {
  const active = contentTabs.tabs.find((tab) => tab.id === contentTabs.active);
  const tabProject = active?.kind === 'chat'
    ? workspace.sessions.find((session) => session.id === active.sessionId)?.projectId
    : active?.kind === 'file'
      ? workspace.projects.find((project) => project.path && samePath(project.path, active.root))?.id
      : undefined;
  newSession(projectId ?? tabProject ?? (active ? undefined : activeSession()?.projectId) ?? NO_PROJECT);
  window.dispatchEvent(new Event('agentdeck:new-chat-requested'));
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

// Closing tabs with unsaved edits asks twice instead of with a modal: the first try warns in a toast, the same
// try again within a few seconds closes and drops the edits.
let armedClose = '';
let armedAt = 0;
export function confirmDiscard(tabs: ContentTab[]): boolean {
  const dirty = tabs.filter((tab) => tab.kind === 'file' && tab.dirty);
  if (dirty.length === 0) return true;
  const key = dirty.map((tab) => tab.id).join('|');
  if (armedClose === key && Date.now() - armedAt < 6000) {
    armedClose = '';
    for (const tab of dirty) drafts.delete(tab.id);
    return true;
  }
  armedClose = key;
  armedAt = Date.now();
  const names = dirty.length === 1 && dirty[0].kind === 'file' ? dirty[0].name : `${dirty.length} files`;
  toast.info(`Unsaved changes in ${names}. Close again to discard them.`);
  return false;
}

/**
 * Writes the unsaved edits of a file tab to disk (Ctrl+S, or Save in the tab menu). Answers whether it saved.
 * `given` is a file tab that lives elsewhere (the Files tool of the right panel).
 */
export async function saveTab(id: string, given?: ContentTab): Promise<boolean> {
  const tab = given ?? contentTabs.tabs.find((item) => item.id === id);
  const draft = drafts.get(id);
  if (!tab || tab.kind !== 'file' || !draft || !inTauri()) return false;
  const raw = joinFileText(draft.read?.() ?? draft.text, splitFileText(draft.disk).format);
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('file_write', { root: tab.root, path: tab.path, text: raw });
  } catch (failure) {
    toast.error(`Could not save ${tab.name}: ${String(failure).replace(/^(Error:\s*)+/i, '')}`);
    return false;
  }
  draft.disk = raw;
  rememberText(fileKey(tab.root, tab.path), raw); // the next viewer of this file opens on what was just written
  if (draft.saved) draft.saved(raw);
  else {
    drafts.delete(id);
    tab.dirty = false;
  }
  fileWrites.tick++;
  toast.success('Saved');
  return true;
}

export function closeTab(id: string) {
  const index = contentTabs.tabs.findIndex((tab) => tab.id === id);
  if (index === -1) return;
  if (!confirmDiscard([contentTabs.tabs[index]])) return;
  remember([contentTabs.tabs[index]]);
  contentTabs.tabs.splice(index, 1);
  if (contentTabs.active === id) {
    // The tab on the left, or the new first tab, becomes active; with no tabs left the area is empty.
    const next = contentTabs.tabs[Math.max(0, index - 1)];
    contentTabs.active = next?.id ?? '';
    if (next?.kind === 'chat') workspace.activeId = next.sessionId;
  }
}

/** Closes every open tab. The chats themselves stay in the sidebar. */
export function closeAllTabs() {
  if (!confirmDiscard(contentTabs.tabs)) return;
  remember(contentTabs.tabs);
  contentTabs.tabs.splice(0, contentTabs.tabs.length);
  contentTabs.active = '';
}

/** Closes every tab except `id`, which becomes the active one. */
export function closeOtherTabs(id: string) {
  const keep = contentTabs.tabs.find((tab) => tab.id === id);
  if (!keep) return;
  if (!confirmDiscard(contentTabs.tabs.filter((tab) => tab.id !== id))) return;
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
    if (mod && !event.shiftKey && !event.altKey && key === 't') {
      event.preventDefault();
      openNewChatTab();
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
      return;
    }
    // Ctrl+W: the tab in front closes (the menu of a tab has said so all along). Not inside the terminal, which uses keys
    // of its own.
    if (mod && !event.shiftKey && !event.altKey && key === 'w' && contentTabs.active && !(event.target as HTMLElement | null)?.closest?.('.xterm')) {
      event.preventDefault();
      closeTab(contentTabs.active);
    }
  };
  window.addEventListener('keydown', onKey);
  return () => window.removeEventListener('keydown', onKey);
}
