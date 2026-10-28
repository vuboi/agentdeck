import { confirmDiscard, type ContentTab } from './content-tabs.svelte';
import { layout, setToolsOpen } from './layout.svelte';
import { getPref, setPref } from './prefs';
import { fileRootOf, resolveInProject } from './paths';

// The Files tool of the right panel: files opened from the chat (a link in an answer, a file a tool wrote) open here,
// in tabs of their own, beside the conversation instead of over it.
export type ViewerTab = Extract<ContentTab, { kind: 'file' }>;

export const viewer = $state({
  active: '' as string,
  tabs: [] as ViewerTab[],
  /** The pane beside the open file: the project tree, or the files git sees changed. */
  // Closed by default: the file has the whole pane. The Files / Changes panel opens from its button.
  side: { open: false, mode: 'files' as 'files' | 'changes', width: 240 },
  /** Counts the opens, the same file again included: a narrow Files tool shows the file each time. */
  opened: 0,
});

/** Ids differ from the content tabs' (the path): the same file open in both keeps two separate unsaved drafts. */
const viewerId = (path: string) => `viewer:${path}`;

// Open tabs survive a reload. Restored once, then saved on every change (unsaved edits are never saved).
let hydrated = false;
getPref<{ active: string; tabs: ViewerTab[]; side?: typeof viewer.side }>('viewer.tabs').then((saved) => {
  if (saved) {
    viewer.tabs = (saved.tabs ?? []).map((tab) => ({ ...tab, dirty: undefined }));
    viewer.active = saved.active ?? '';
    // The panel starts closed each time, whatever it was when the app closed; its width and mode are kept.
    if (saved.side) viewer.side = { ...viewer.side, ...saved.side, open: false };
  }
  hydrated = true;
});
$effect.root(() => {
  $effect(() => {
    const snapshot = { active: viewer.active, tabs: viewer.tabs.map((tab) => ({ ...tab, dirty: undefined })), side: { ...viewer.side } };
    if (hydrated) setPref('viewer.tabs', snapshot);
  });
});

/** Opens (or focuses) a file in the Files tool and shows the tool. `root` is the folder the file must stay inside. */
export function openInViewer(path: string, root: string, view?: 'file' | 'changes') {
  const id = viewerId(path);
  // With no file open the tree is the only thing shown: the first file opens beside it, not in its place.
  const treeWasAlone = viewer.tabs.length === 0;
  const open = viewer.tabs.find((tab) => tab.id === id);
  if (!open) viewer.tabs.push({ id, kind: 'file', name: path.split(/[\\/]/).pop() ?? path, path, root, view });
  else if (view) open.view = view;
  viewer.active = id;
  viewer.opened++;
  // A file opened as its changes (the diff) shows the side panel from the start, so the list it came from is there.
  // A plain file leaves the panel as it is (closed by default).
  if (view === 'changes' || treeWasAlone) viewer.side.open = true;
  layout.activeTool = 'Explore';
  setToolsOpen(true);
}

/**
 * Opens a file named by a path of any kind (full, or relative to the chat's project folder) in the Files tool. The folder
 * it is read from is the project when the file is inside it, else the file's own folder. Use this, not a hand-built path.
 */
export function openProjectFile(path: string, projectRoot: string, view?: 'file' | 'changes') {
  const full = resolveInProject(path, projectRoot);
  openInViewer(full, fileRootOf(full, projectRoot), view);
}

export const isViewerTab = (id: string) => viewer.tabs.some((tab) => tab.id === id);

export function closeViewerTab(id: string) {
  const index = viewer.tabs.findIndex((tab) => tab.id === id);
  if (index === -1 || !confirmDiscard([viewer.tabs[index]])) return;
  viewer.tabs.splice(index, 1);
  if (viewer.active === id) viewer.active = viewer.tabs[Math.max(0, index - 1)]?.id ?? '';
}

export function closeOtherViewerTabs(id: string) {
  const keep = viewer.tabs.find((tab) => tab.id === id);
  if (!keep || !confirmDiscard(viewer.tabs.filter((tab) => tab.id !== id))) return;
  viewer.tabs.splice(0, viewer.tabs.length, keep);
  viewer.active = id;
}

export function closeAllViewerTabs() {
  if (!confirmDiscard(viewer.tabs)) return;
  viewer.tabs.splice(0, viewer.tabs.length);
  viewer.active = '';
}
