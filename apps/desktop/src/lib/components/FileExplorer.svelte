<script lang="ts">
  import { closeOnScroll } from '../close-on-scroll';
  import { ZDropdownMenu, ZIcon, ZTree } from '@shival99/z-ui-svelte';
  import type { ZDropdownMenuItem, ZIconName, ZTreeNode } from '@shival99/z-ui-svelte';
  import { tick, untrack } from 'svelte';
  import { activeSession, workspace } from '../sessions.svelte';
  import { openFileTab } from '../content-tabs.svelte';
  import { inTauri } from '../history';
  import { toast } from '../toast';
  import { fileWrites } from '../activity.svelte';
  import FileSearchPalette from './FileSearchPalette.svelte';

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
  const DRAFT = '/::new-entry::';
  function fromDisk(dir: string): ZTreeNode<null>[] {
    const draft: ZTreeNode<null>[] =
      creating && creating.dir === dir && listing[dir] ? [{ id: `${dir}${DRAFT}`, label: '', icon: (creating.folder ? 'lucideFolder' : 'lucideFile') as ZIconName }] : [];
    return draft.concat((listing[dir] ?? []).map((entry) => {
      const id = `${dir}/${entry.name}`;
      return {
        id,
        label: entry.name,
        icon: (entry.dir ? 'lucideFolder' : 'lucideFile') as ZIconName,
        // A folder not read yet shows one "Loading…" row, so it still has an arrow to open it.
        children: entry.dir ? (listing[id] ? fromDisk(id) : [{ id: `${id}${PLACEHOLDER}`, label: 'Loading…', icon: 'lucideLoaderCircle' as ZIconName }]) : undefined,
      };
    }));
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
    if (node.children || node.id.endsWith(PLACEHOLDER) || node.id.endsWith(DRAFT)) return;
    if (root) openFileTab(`${root}${node.id}`, root);
  }

  const parentOf = (id: string) => id.slice(0, id.lastIndexOf('/'));
  const nameOf = (id: string) => id.slice(id.lastIndexOf('/') + 1);

  // ---------- right click: rename, delete ----------
  let menu = $state<{ x: number; y: number; id: string } | null>(null);
  let menuOpen = $state(false);
  $effect(() => closeOnScroll(() => menuOpen, () => (menuOpen = false)));
  let menuKey = $state(0);
  let selected = $state<string[]>([]);

  function onContextMenu(event: MouseEvent) {
    event.preventDefault();
    if (!inTauri()) return;
    // Right click on the empty part of the list works on the project folder itself ('').
    const id = (event.target as HTMLElement).closest('[role="treeitem"]')?.querySelector<HTMLElement>('[data-path]')?.dataset.path ?? '';
    if (id.endsWith(PLACEHOLDER) || id.endsWith(DRAFT)) return;
    // The row under the pointer becomes the active one, so the menu is plainly about it.
    if (id) selected = [id];
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

  /** The folder a new file goes in: the folder itself, or the folder that holds the file ('' is the project folder). */
  function folderFor(id: string) {
    if (id === '') return '';
    const entry = listing[parentOf(id)]?.find((item) => item.name === nameOf(id));
    return entry?.dir ? id : parentOf(id);
  }

  const menuItems = $derived<ZDropdownMenuItem[]>(menu?.id === '' ? [
    { label: 'New file', icon: 'lucideFile', iconSize: 12, class: 'text-xs! py-1', onClick: () => askCreate('', false) },
    { label: 'New folder', icon: 'lucideFolderPlus', iconSize: 12, class: 'text-xs! py-1', onClick: () => askCreate('', true) },
    { label: 'Reveal in File Explorer', icon: 'lucideFolder', iconSize: 12, class: 'text-xs! py-1', divide: 'before', onClick: () => void reveal('/.') },
  ] : [
    { label: 'New file', icon: 'lucideFile', iconSize: 12, class: 'text-xs! py-1', onClick: () => menu && askCreate(folderFor(menu.id), false) },
    { label: 'New folder', icon: 'lucideFolderPlus', iconSize: 12, class: 'text-xs! py-1', onClick: () => menu && askCreate(folderFor(menu.id), true) },
    { label: 'Rename', icon: 'lucidePencil', iconSize: 12, class: 'text-xs! py-1', divide: 'before', onClick: () => menu && askRename(menu.id) },
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
    if (!armed || renaming !== null || creating || !(event.ctrlKey || event.metaKey) || event.shiftKey || event.key.toLowerCase() !== 'z') return;
    event.preventDefault();
    void undoLast();
  }

  // ---------- new file / new folder ----------
  // An input row opens at the top of the folder, as in VS Code: Enter makes it, Esc (or an empty name) drops it.
  let creating = $state<{ dir: string; folder: boolean } | null>(null);
  let createName = $state('');
  let createError = $state('');
  let committing = false;

  async function askCreate(dir: string, folder: boolean) {
    if (!root || !inTauri()) return;
    if (dir && !expanded.includes(dir)) expanded = [...expanded, dir];
    await load(dir);
    createName = '';
    createError = '';
    creating = { dir, folder };
  }

  function cancelCreate() {
    creating = null;
    createName = '';
    createError = '';
  }

  async function doCreate() {
    if (!creating || committing) return;
    const { dir, folder } = creating;
    const name = createName.trim();
    if (!name) return cancelCreate();
    committing = true;
    try {
      await call<string>('path_create', { root, dir: `${root}${dir}`, name, folder });
      cancelCreate();
      refresh(dir);
      if (!folder) openFileTab(`${root}${dir}/${name}`, root);
    } catch (error) {
      createError = String(error).replace(/^(Error:\s*)+/i, '');
    } finally {
      committing = false;
    }
  }

  function onCreateKey(event: KeyboardEvent) {
    event.stopPropagation();
    if (event.key === 'Enter') {
      event.preventDefault();
      void doCreate();
    } else if (event.key === 'Escape') {
      event.preventDefault();
      cancelCreate();
    }
  }
  // The menu that opened the row and the tree itself both take focus back while it settles: until then a blur
  // only puts focus on the input again, it does not drop the row.
  let armed_at = 0;
  function onCreateBlur(event: FocusEvent) {
    if (committing) return;
    if (Date.now() < armed_at) {
      const input = event.currentTarget as HTMLInputElement;
      setTimeout(() => input.isConnected && input.focus(), 0);
      return;
    }
    if (createName.trim() && !createError) void doCreate();
    else cancelCreate();
  }
  function focusNow(node: HTMLInputElement) {
    armed_at = Date.now() + 400;
    node.focus();
    for (const delay of [60, 150, 300]) setTimeout(() => node.isConnected && document.activeElement !== node && node.focus(), delay);
  }

  // ---------- find a file by name ----------
  let findOpen = $state(false);
  const openFound = (relative: string) => openFileTab(`${root}/${relative}`, root);

  // ---------- rename ----------
  // The row's name turns into an input: Enter renames, Esc leaves it as it was.
  let renaming = $state<string | null>(null);
  let newName = $state('');
  let renameError = $state('');
  let renameBusy = false;

  async function askRename(id: string) {
    newName = nameOf(id);
    renameError = '';
    renaming = id;
  }

  function cancelRename() {
    renaming = null;
    renameError = '';
  }

  async function doRename() {
    if (!renaming || renameBusy) return;
    const target = renaming;
    const from = nameOf(target);
    const to = newName.trim();
    if (!to || to === from) return cancelRename();
    renameBusy = true;
    try {
      await call('path_rename', { root, path: `${root}${target}`, name: to });
      const dir = parentOf(target);
      undo.push({ kind: 'rename', dir, from, to });
      cancelRename();
      refresh(dir, target);
      say(`Renamed to ${to}`, true);
    } catch (error) {
      renameError = String(error).replace(/^(Error:\s*)+/i, '');
    } finally {
      renameBusy = false;
    }
  }

  function onRenameKey(event: KeyboardEvent) {
    event.stopPropagation();
    if (event.key === 'Enter') {
      event.preventDefault();
      void doRename();
    } else if (event.key === 'Escape') {
      event.preventDefault();
      cancelRename();
    }
  }
  function onRenameBlur(event: FocusEvent) {
    if (renameBusy) return;
    if (Date.now() < armed_at) {
      const input = event.currentTarget as HTMLInputElement;
      setTimeout(() => input.isConnected && input.focus(), 0);
      return;
    }
    if (newName.trim() && newName.trim() !== nameOf(renaming ?? '') && !renameError) void doRename();
    else cancelRename();
  }
  // Focus the input with the name selected, without its extension (as VS Code does).
  function focusRename(node: HTMLInputElement) {
    armed_at = Date.now() + 400;
    node.focus();
    const dot = node.value.lastIndexOf('.');
    node.setSelectionRange(0, dot > 0 ? dot : node.value.length);
    for (const delay of [60, 150, 300]) setTimeout(() => node.isConnected && document.activeElement !== node && node.focus(), delay);
  }

  // ---------- delete ----------
  // No question: the file goes to the trash and the toast offers Undo (Ctrl+Z works too).
  async function askDelete(id: string) {
    const name = nameOf(id);
    try {
      const trashed = await call<string>('path_trash', { root, path: `${root}${id}` });
      const dir = parentOf(id);
      undo.push({ kind: 'delete', dir, name, trashed });
      refresh(dir, id);
      say(`Deleted ${name}`, true);
    } catch (error) {
      toast.error(String(error));
    }
  }
</script>

<svelte:window onpointerdown={onWindowPointer} onkeydown={onWindowKey} />

{#if root && inTauri()}
  <!-- Find a file by name, or make a new file or folder in the project folder. -->
  <div class="flex h-9 shrink-0 items-center gap-1 border-b border-border p-1 text-xs">
    <button type="button" class="inline-flex h-7 min-w-0 flex-1 cursor-pointer items-center gap-1.5 rounded px-2 text-left text-muted-foreground outline-none hover:bg-primary/10 hover:text-primary" title="Find a file by name" onclick={() => (findOpen = true)}>
      <ZIcon zType={'lucideSearch' as ZIconName} zSize="15" /> <span class="truncate">Search files</span>
    </button>
    <button type="button" class="inline-flex size-7 cursor-pointer items-center justify-center rounded text-muted-foreground outline-none hover:bg-primary/10 hover:text-primary" title="New file" aria-label="New file" onclick={() => askCreate('', false)}>
      <ZIcon zType={'lucidePlus' as ZIconName} zSize="15" />
    </button>
    <button type="button" class="inline-flex size-7 cursor-pointer items-center justify-center rounded text-muted-foreground outline-none hover:bg-primary/10 hover:text-primary" title="New folder" aria-label="New folder" onclick={() => askCreate('', true)}>
      <ZIcon zType={'lucideFolderPlus' as ZIconName} zSize="15" />
    </button>
  </div>
{/if}

{#if nodes.length === 0}
  <div class="flex h-full min-h-0 flex-1 flex-col items-center justify-center gap-3 p-6 text-center text-muted-foreground" role="tabpanel" aria-label="Explore">
    {#if root && !failure && !listing[''] && inTauri()}
      <!-- A big folder takes a moment: the loader sits where the files will appear. -->
      <ZIcon zType={'lucideLoaderCircle' as ZIconName} class="size-5 animate-spin text-primary" />
      <p class="m-0 max-w-56 text-xs leading-normal" role="status">Reading the {project?.name} folder…</p>
    {:else}
      <ZIcon zType="lucideFolder" zSize="28" class="size-7 opacity-60" />
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
    <ZTree {nodes} zSize="sm" zShowLine zExpandOnClick zMaxHeight={Math.max(0, treeHeight - 18)} zLabel="Project files" bind:expanded bind:selected {onSelect}>
      {#snippet node(ctx)}
        {#if ctx.node.id.endsWith(DRAFT)}
          <ZIcon zType={(ctx.node.icon ?? 'lucideFile') as ZIconName} class="size-3.5 shrink-0 text-muted-foreground" />
          <input
            use:focusNow
            bind:value={createName}
            class="h-5 min-w-0 flex-1 bg-transparent p-0 text-xs outline-none {createError ? 'text-destructive' : 'text-foreground'}"
            spellcheck="false"
            autocomplete="off"
            aria-label={creating?.folder ? 'Folder name' : 'File name'}
            aria-invalid={createError ? 'true' : undefined}
            title={createError}
            oninput={() => (createError = '')}
            onkeydown={onCreateKey}
            onblur={onCreateBlur}
            onclick={(event) => event.stopPropagation()}
            onpointerdown={(event) => event.stopPropagation()}
          />
        {:else if renaming === ctx.node.id}
          <ZIcon zType={(ctx.node.icon ?? 'lucideFile') as ZIconName} class="size-3.5 shrink-0 text-muted-foreground" />
          <input
            use:focusRename
            bind:value={newName}
            class="h-5 min-w-0 flex-1 bg-transparent p-0 text-xs outline-none {renameError ? 'text-destructive' : 'text-foreground'}"
            spellcheck="false"
            autocomplete="off"
            aria-label="New name"
            aria-invalid={renameError ? 'true' : undefined}
            title={renameError}
            oninput={() => (renameError = '')}
            onkeydown={onRenameKey}
            onblur={onRenameBlur}
            onclick={(event) => event.stopPropagation()}
            onpointerdown={(event) => event.stopPropagation()}
          />
        {:else}
        <ZIcon zType={(ctx.node.icon ?? 'lucideFile') as ZIconName} class="size-3.5 shrink-0 text-muted-foreground {ctx.node.id.endsWith(PLACEHOLDER) ? 'animate-spin' : ''}" />
        <span data-path={ctx.node.id} class="min-w-0 flex-1 truncate {ctx.node.id.endsWith(PLACEHOLDER) ? 'text-muted-foreground' : ''}">{ctx.node.label}</span>
        {/if}
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

<FileSearchPalette bind:open={findOpen} {root} onpick={openFound} />
