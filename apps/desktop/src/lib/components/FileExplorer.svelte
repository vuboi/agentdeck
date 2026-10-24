<script lang="ts">
  import { ZDropdownMenu, ZIcon, ZInput, ZLoading, ZModal, ZTree, zModalService } from '@shival99/z-ui-svelte';
  import type { ZDropdownMenuItem, ZIconName, ZTreeNode } from '@shival99/z-ui-svelte';
  import { tick, untrack } from 'svelte';
  import { activeSession, workspace } from '../sessions.svelte';
  import { openFileTab } from '../content-tabs.svelte';
  import { inTauri } from '../history';
  import { toast } from '../toast';
  import { fileWrites } from '../activity.svelte';

  // Files of the active session's project, read from disk one folder at a time as folders are opened.
  // Right click renames or deletes; a delete goes to ~/.agentdeck/trash/files, so Ctrl+Z can put it back.
  // Outside the desktop app (the web preview) a small mock listing stands in.
  type File = { name: string; children?: File[] };
  type Entry = { name: string; dir: boolean };
  const mockFiles: Record<string, File[]> = {
    agentdeck: [
      { name: 'apps', children: [{ name: 'desktop', children: [{ name: 'src', children: [{ name: 'lib', children: [{ name: 'components' }, { name: 'sessions.svelte.ts' }] }, { name: 'routes' }] }, { name: 'package.json' }] }] },
      { name: 'crates', children: [{ name: 'protocol' }, { name: 'store' }, { name: 'core' }] },
      { name: 'AGENTS.md' },
      { name: 'PLAN.md' },
      { name: 'README.md' },
    ],
  };

  const project = $derived(workspace.projects.find((item) => item.id === activeSession()?.projectId));
  const root = $derived(project?.path ?? '');

  async function call<T = void>(command: string, args: Record<string, unknown>): Promise<T> {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<T>(command, args);
  }

  // Folder listings by path inside the project ('' is the project folder itself, '/src' a folder in it).
  let listing = $state<Record<string, Entry[]>>({});
  let failure = $state('');
  const loading = new Set<string>();

  async function load(dir: string) {
    if (loading.has(dir) || listing[dir]) return;
    loading.add(dir);
    const from = root;
    try {
      const entries = await call<Entry[]>('folder_list', { root: from, path: `${from}${dir}` });
      if (from === root) listing[dir] = entries;
    } catch (error) {
      if (dir === '') failure = String(error);
    } finally {
      loading.delete(dir);
    }
  }

  /** Reads a folder again after a change in it; folders below a renamed or deleted one are forgotten. */
  function refresh(dir: string, gone?: string) {
    if (gone) for (const key of Object.keys(listing)) if (key === gone || key.startsWith(`${gone}/`)) delete listing[key];
    delete listing[dir];
    void load(dir);
  }

  // An agent wrote, edited or deleted a file: read the folders already on screen again, in place. The tree keeps
  // its open folders and its scroll; each folder's rows swap in when its new listing arrives.
  async function reread(dir: string) {
    const from = root;
    try {
      const entries = await call<Entry[]>('folder_list', { root: from, path: `${from}${dir}` });
      if (from === root) listing[dir] = entries;
    } catch {
      // A folder that went away with the change stops showing on the next read of its parent.
    }
  }
  $effect(() => {
    void fileWrites.tick;
    untrack(() => {
      if (!root || !inTauri()) return;
      for (const dir of ['', ...Object.keys(listing)]) void reread(dir);
    });
  });

  // Another project: start again from its folder.
  $effect(() => {
    const current = root;
    untrack(() => {
      listing = {};
      failure = '';
      loading.clear();
      undo.length = 0;
      if (current && inTauri()) void load('');
    });
  });

  let expanded = $state<string[]>([]);
  // An opened folder reads its entries the first time.
  $effect(() => {
    for (const id of expanded) if (!listing[id]) void load(id);
  });

  const PLACEHOLDER = '/…';
  function fromDisk(dir: string): ZTreeNode<null>[] {
    return (listing[dir] ?? []).map((entry) => {
      const id = `${dir}/${entry.name}`;
      return {
        id,
        label: entry.name,
        icon: (entry.dir ? 'lucideFolder' : 'lucideFile') as ZIconName,
        // A folder not read yet shows one "Loading…" row, so it still has an arrow to open it.
        children: entry.dir ? (listing[id] ? fromDisk(id) : [{ id: `${id}${PLACEHOLDER}`, label: 'Loading…', icon: 'lucideLoaderCircle' as ZIconName }]) : undefined,
      };
    });
  }

  // Same shape as the sidebar tree: folders expand, files are leaves.
  function fromMock(files: File[], parent = ''): ZTreeNode<null>[] {
    return files.map((file) => {
      const id = `${parent}/${file.name}`;
      return { id, label: file.name, icon: (file.children ? 'lucideFolder' : 'lucideFile') as ZIconName, children: file.children ? fromMock(file.children, id) : undefined };
    });
  }

  const nodes = $derived(inTauri() ? fromDisk('') : fromMock(mockFiles[project?.name ?? ''] ?? []));
  let treeHeight = $state(400);
  let box: HTMLDivElement | undefined = $state();

  // A file opens as a tab over the conversation; folders only expand.
  function onSelect(node: ZTreeNode<null>) {
    if (node.children || node.id.endsWith(PLACEHOLDER)) return;
    if (root) openFileTab(`${root}${node.id}`, root);
  }

  const parentOf = (id: string) => id.slice(0, id.lastIndexOf('/'));
  const nameOf = (id: string) => id.slice(id.lastIndexOf('/') + 1);

  // ---------- right click: rename, delete ----------
  let menu = $state<{ x: number; y: number; id: string } | null>(null);
  let menuOpen = $state(false);
  let menuKey = $state(0);

  function onContextMenu(event: MouseEvent) {
    event.preventDefault();
    if (!inTauri()) return;
    const id = (event.target as HTMLElement).closest('[role="treeitem"]')?.querySelector<HTMLElement>('[data-path]')?.dataset.path;
    if (!id || id.endsWith(PLACEHOLDER)) return;
    menuKey++;
    menu = { x: event.clientX, y: event.clientY, id };
    menuOpen = true;
  }

  // Reveal the folder (or the folder holding the file) in the system file manager, as the sidebar does for projects.
  async function reveal(id: string) {
    const folder = id.endsWith(PLACEHOLDER) ? '' : parentOf(id);
    try {
      await call('reveal_path', { path: `${root}${folder}` });
    } catch (error) {
      toast.error(String(error));
    }
  }

  const menuItems = $derived<ZDropdownMenuItem[]>([
    { label: 'Rename', icon: 'lucidePencil', iconSize: 12, class: 'text-xs! py-1', onClick: () => menu && askRename(menu.id) },
    { label: 'Reveal in File Explorer', icon: 'lucideFolder', iconSize: 12, class: 'text-xs! py-1', onClick: () => menu && void reveal(menu.id) },
    {
      label: 'Copy path',
      icon: 'lucideCopy',
      iconSize: 12,
      class: 'text-xs! py-1',
      onClick: () => {
        if (!menu) return;
        void navigator.clipboard.writeText(`${root}${menu.id}`);
        toast.success('Path copied');
      },
    },
    {
      label: 'Delete',
      icon: 'lucideTrash2',
      iconSize: 12,
      divide: 'before',
      class: 'text-xs! py-1 text-destructive! hover:bg-destructive/10! hover:text-destructive! [&_svg]:text-destructive!',
      onClick: () => menu && askDelete(menu.id),
    },
  ]);

  // ---------- undo (Ctrl+Z) ----------
  type Step = { kind: 'rename'; dir: string; from: string; to: string } | { kind: 'delete'; dir: string; name: string; trashed: string };
  const undo: Step[] = [];
  // A pill note at the bottom; a change that can be undone shows an Undo button on it.
  const say = (text: string, undoable = false) => toast.success(text, undoable ? { undo: () => void undoLast() } : undefined);

  async function undoLast() {
    const step = undo.pop();
    if (!step) return say('Nothing to undo.');
    try {
      if (step.kind === 'rename') {
        await call('path_rename', { root, path: `${root}${step.dir}/${step.to}`, name: step.from });
        refresh(step.dir, `${step.dir}/${step.to}`);
        say(`Renamed back to ${step.from}.`);
      } else {
        await call('path_restore', { root, trashed: step.trashed, to: `${root}${step.dir}/${step.name}` });
        refresh(step.dir);
        say(`Restored ${step.name}.`);
      }
    } catch (error) {
      say(String(error));
    }
  }

  // Ctrl+Z belongs to Explore while it was the last place you clicked.
  let armed = false;
  function onWindowPointer(event: PointerEvent) {
    armed = !!box && box.contains(event.target as Node);
  }
  function onWindowKey(event: KeyboardEvent) {
    if (!armed || renaming || !(event.ctrlKey || event.metaKey) || event.shiftKey || event.key.toLowerCase() !== 'z') return;
    event.preventDefault();
    void undoLast();
  }

  // ---------- rename ----------
  let renaming = $state(false);
  let target = $state('');
  let newName = $state('');
  let busy = $state(false);
  let renameError = $state('');
  let nameInput: { focus?: () => void } | undefined = $state();

  async function askRename(id: string) {
    target = id;
    newName = nameOf(id);
    renameError = '';
    renaming = true;
    await tick();
    nameInput?.focus?.();
  }

  async function doRename() {
    const from = nameOf(target);
    const to = newName.trim();
    if (!to || to === from) return void (renaming = false);
    busy = true;
    try {
      await call('path_rename', { root, path: `${root}${target}`, name: to });
      const dir = parentOf(target);
      undo.push({ kind: 'rename', dir, from, to });
      refresh(dir, target);
      renaming = false;
      say(`Renamed to ${to}`, true);
    } catch (error) {
      renameError = String(error);
    } finally {
      busy = false;
    }
  }

  // ---------- delete ----------
  function askDelete(id: string) {
    const name = nameOf(id);
    zModalService.danger({
      zTitle: `Delete ${name}?`,
      zContent: `It moves to ~/.agentdeck/trash/files. Ctrl+Z in Explore puts it back.`,
      zConfirmStyle: 'dialog',
      zOkText: 'Delete',
      zOkIcon: 'lucideTrash2',
      zCancelText: 'Cancel',
      zWidth: '400px',
      onOk: async () => {
        try {
          const trashed = await call<string>('path_trash', { root, path: `${root}${id}` });
          const dir = parentOf(id);
          undo.push({ kind: 'delete', dir, name, trashed });
          refresh(dir, id);
          say(`Deleted ${name}`, true);
        } catch (error) {
          zModalService.warning({ zTitle: 'Could not delete', zContent: String(error), zConfirmStyle: 'dialog', zCancelText: null, zOkText: 'OK' });
        }
      },
    });
  }
</script>

<svelte:window onpointerdown={onWindowPointer} onkeydown={onWindowKey} />

{#if nodes.length === 0}
  <div class="flex h-full min-h-0 flex-1 flex-col items-center justify-center gap-3 p-6 text-center text-muted-foreground" role="tabpanel" aria-label="Explore">
    {#if root && !failure && !listing[''] && inTauri()}
      <!-- A big folder takes a moment: the loader sits where the files will appear. -->
      <ZIcon zType={'lucideLoaderCircle' as ZIconName} class="size-5 animate-spin text-primary" />
      <p class="m-0 max-w-56 text-xs leading-normal" role="status">Reading the {project?.name} folder…</p>
    {:else}
      <ZIcon zType="lucideFolder" zSize="36" class="size-9 opacity-60" />
      <h2 class="text-sm font-medium text-foreground">Explore</h2>
      <p class="m-0 max-w-56 text-xs leading-normal">
        {#if failure}{failure}{:else if !root}Open a chat in a project to see its files.{:else}This folder is empty.{/if}
      </p>
    {/if}
  </div>
{:else}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    bind:this={box}
    bind:clientHeight={treeHeight}
    class="relative flex min-h-0 flex-1 flex-col pt-1.5 [&_.z-tree-row:focus-visible]:before:ring-0!"
    oncontextmenu={onContextMenu}
  >
    <!-- The list stops 12px above the panel's edge, so the last file never touches it. -->
    <ZTree {nodes} zSize="sm" zShowLine zExpandOnClick zMaxHeight={Math.max(0, treeHeight - 18)} zLabel="Project files" bind:expanded {onSelect}>
      {#snippet node(ctx)}
        <ZIcon zType={(ctx.node.icon ?? 'lucideFile') as ZIconName} class="size-3.5 shrink-0 text-muted-foreground {ctx.node.id.endsWith(PLACEHOLDER) ? 'animate-spin' : ''}" />
        <span data-path={ctx.node.id} class="min-w-0 flex-1 truncate {ctx.node.id.endsWith(PLACEHOLDER) ? 'text-muted-foreground' : ''}">{ctx.node.label}</span>
      {/snippet}
    </ZTree>
  </div>
{/if}

{#if menu}
  {#key menuKey}
    <div class="pointer-events-none fixed z-50 size-px" style:left="{menu.x}px" style:top="{menu.y}px">
      <ZDropdownMenu zItems={menuItems} bind:open={menuOpen} zPosition="bottom-left" zOffset={4} zMinWidth={160} zContentClass="p-1 text-xs">
        <span class="block size-px"></span>
      </ZDropdownMenu>
    </div>
  {/key}
{/if}

<ZModal bind:open={renaming} zTitle="Rename" zWidth="480px" zBodyClass="min-h-0! py-3" zOkText="Rename" zCancelText="Cancel" zLoading={busy} onOk={doRename}>
  <div class="flex flex-col gap-2">
    <ZInput bind:this={nameInput} bind:value={newName} zPlaceholder="New name" onEnter={doRename} />
    {#if renameError}<span class="text-xs text-destructive" role="alert">{renameError}</span>{/if}
  </div>
</ZModal>
