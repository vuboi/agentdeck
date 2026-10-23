<script lang="ts">
  import { untrack } from 'svelte';
  import { ZButton, ZDropdownMenu, ZIcon, ZInput, ZLoading, ZModal, ZTree, zModalService } from '@shival99/z-ui-svelte';
  import type { ZDropdownMenuItem, ZIconName, ZTreeNode } from '@shival99/z-ui-svelte';
  import Logo from './Logo.svelte';
  import { toast } from '../toast';
  import AgentStack from './AgentStack.svelte';
  import { showSession } from '../content-tabs.svelte';
  import SearchPalette from './SearchPalette.svelte';
  import DockZone from './DockZone.svelte';
  import Splitter from './Splitter.svelte';
  import { layout } from '../layout.svelte';
  import {
    buildTree,
    deleteSession,
    forkSession,
    history,
    newChat,
    newSession,
    hideProject,
    NO_PROJECT,
    prefetchTranscript,
    renameSession,
    workspace,
    type TreeData,
  } from '../sessions.svelte';

  const nodes = $derived(buildTree());

  // The tree is the only scroller (wheel, keyboard and virtualisation all work on it), sized to the space left.
  let treeHeight = $state(400);

  // Everything starts open; a fork opens its parent so the new row is visible.
  let expanded = $state<string[]>([
    ...workspace.projects.map((project) => `project:${project.id}`),
    ...workspace.sessions.map((item) => `session:${item.id}`),
  ]);

  // Everything stays open: the tree is virtualised, so a long list costs nothing.
  $effect(() => {
    if (!history.imported) return;
    untrack(() => {
      expanded = [
        ...workspace.projects.map((project) => `project:${project.id}`),
        ...workspace.sessions.filter((item) => workspace.sessions.some((child) => child.parentId === item.id)).map((item) => `session:${item.id}`),
      ];
    });
  });

  // Whatever chat is open is visible: its parents (forks hang under them) are expanded.
  $effect(() => {
    const active = workspace.sessions.find((item) => item.id === workspace.activeId);
    if (!active?.parentId) return;
    untrack(() => {
      const ancestors: string[] = [];
      for (let cursor = active.parentId; cursor; ) {
        ancestors.push(`session:${cursor}`);
        cursor = workspace.sessions.find((item) => item.id === cursor)?.parentId;
        if (ancestors.length > 50) break;
      }
      const missing = ancestors.filter((id) => !expanded.includes(id));
      if (missing.length) expanded = [...expanded, ...missing];
    });
  });

  let selected = $state<string[]>([]);
  $effect(() => {
    selected = [`session:${workspace.activeId}`];
  });

  function onSelect(node: ZTreeNode<TreeData>) {
    // Projects only expand; only sessions switch the chat.
    // Opens the session's chat tab even when it is already the active session (its tab may have been closed).
    if (node.data?.kind === 'session' && node.data.session) showSession(node.data.session.id);
    else selected = [`session:${workspace.activeId}`];
  }

  function fork(sessionId: string) {
    forkSession(sessionId);
    expanded = [...new Set([...expanded, `session:${sessionId}`])];
  }

  // ---------- row menu: right click, or the three dots that replace the agent tag on hover ----------
  let menu = $state<{ x: number; y: number; id: string; align: 'bottom-left' | 'bottom-right'; kind: 'session' | 'project' } | null>(null);
  let menuOpen = $state(false);
  let menuKey = $state(0);
  let treeBox: HTMLDivElement | undefined = $state();

  // Right click anywhere on a row, not just on its text. Everywhere else in the tree the browser's own
  // menu (Inspect…) is suppressed too.
  function onTreeContextMenu(event: MouseEvent) {
    event.preventDefault();
    const row = (event.target as HTMLElement).closest('[role="treeitem"]');
    const id = row?.querySelector<HTMLElement>('[data-session]')?.dataset.session;
    const project = row?.querySelector<HTMLElement>('[data-project]')?.dataset.project;
    if (id) void openMenu(event, id);
    else if (project) void openMenu(event, project, undefined, 'project');
  }

  // The row the menu belongs to stays highlighted while the menu is open.
  $effect(() => {
    const id = menuOpen ? menu?.id : undefined;
    untrack(() => {
      for (const row of treeBox?.querySelectorAll('[data-menu-row]') ?? []) row.removeAttribute('data-menu-row');
      if (id) treeBox?.querySelector(`[data-session="${CSS.escape(id)}"]`)?.closest('[role="treeitem"]')?.setAttribute('data-menu-row', '');
    });
  });
  let searchOpen = $state(false);
  const menuSession = $derived(menu ? workspace.sessions.find((item) => item.id === menu!.id) : undefined);

  async function openMenu(event: MouseEvent, id: string, anchor?: HTMLElement, kind: 'session' | 'project' = 'session') {
    event.preventDefault();
    event.stopPropagation();
    menuKey++; // a fresh menu at the new spot: no close-then-open flicker
    if (anchor) {
      const rect = anchor.getBoundingClientRect();
      menu = { x: rect.right, y: rect.bottom, id, align: 'bottom-right', kind };
    } else {
      menu = { x: event.clientX, y: event.clientY, id, align: 'bottom-left', kind };
    }
    menuOpen = true;
  }

  // ---------- project menu: right click on a project row ----------
  const menuProject = $derived(menu?.kind === 'project' ? workspace.projects.find((item) => item.id === menu!.id) : undefined);

  async function revealProject() {
    const path = menuProject?.path;
    if (!path) return;
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('reveal_path', { path });
    } catch (error) {
      zModalService.warning({ zTitle: 'Could not open the folder', zContent: String(error), zConfirmStyle: 'dialog', zCancelText: null, zOkText: 'OK' });
    }
  }

  function askHideProject() {
    const project = menuProject;
    if (!project) return;
    zModalService.danger({
      zTitle: 'Remove project from the list?',
      zContent: `“${project.name}” leaves the sidebar. Its folder and its chats stay on disk, in Claude and Codex.`,
      zConfirmStyle: 'dialog',
      zOkText: 'Remove',
      zOkIcon: 'lucideTrash2',
      zCancelText: 'Cancel',
      zWidth: '400px',
      onOk: () => hideProject(project.id),
    });
  }

  const ITEM = 'text-xs! py-1';
  const projectItems = $derived<ZDropdownMenuItem[]>([
    { label: 'New session', icon: 'lucidePlus', iconSize: '12', class: ITEM, onClick: () => menu && addSession(menu.id) },
    { label: 'Reveal in File Explorer', icon: 'lucideFolder', iconSize: '12', class: ITEM, disabled: !menuProject?.path, onClick: () => void revealProject() },
    {
      label: 'Copy path',
      icon: 'lucideCopy',
      iconSize: '12',
      class: ITEM,
      disabled: !menuProject?.path,
      onClick: () => {
        if (!menuProject?.path) return;
        void navigator.clipboard.writeText(menuProject.path);
        toast.success('Path copied');
      },
    },
    {
      label: 'Remove from list',
      icon: 'lucideTrash2',
      iconSize: '12',
      divide: 'before',
      disabled: !menuProject?.path,
      class: `${ITEM} text-destructive! hover:bg-destructive/10! hover:text-destructive! [&_svg]:text-destructive!`,
      onClick: () => askHideProject(),
    },
  ]);

  const sessionItems = $derived<ZDropdownMenuItem[]>([
    { label: 'Rename', icon: 'lucidePencil', iconSize: '12', shortcut: 'R', class: 'text-xs! py-1', onClick: () => askRename() },
    { label: 'Fork', icon: 'lucideGitFork', iconSize: '12', shortcut: 'F', class: 'text-xs! py-1', onClick: () => menu && fork(menu.id) },
    { label: 'Delete', icon: 'lucideTrash2', iconSize: '12', shortcut: 'D', divide: 'before', class: 'text-xs! py-1 text-destructive! hover:bg-destructive/10! hover:text-destructive! [&_svg]:text-destructive! [&_.ml-auto]:text-destructive!', onClick: () => askDelete() },
  ]);
  const menuItems = $derived(menu?.kind === 'project' ? projectItems : sessionItems);

  // The letters on the right are real shortcuts while a chat's menu is open.
  $effect(() => {
    if (!menuOpen || menu?.kind !== 'session') return;
    const keys: Record<string, () => void> = {
      r: () => askRename(),
      f: () => menu && fork(menu.id),
      d: () => askDelete(),
    };
    const onKey = (event: KeyboardEvent) => {
      const action = event.ctrlKey || event.metaKey || event.altKey ? undefined : keys[event.key.toLowerCase()];
      if (!action) return;
      event.preventDefault();
      action();
      menuOpen = false;
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  });

  // ---------- rename / delete (also written into the original tool's own storage) ----------
  let renaming = $state(false);
  let nameInput: { focus: () => void } | undefined = $state();
  // Type straight away: focus the field once the modal is on screen.
  $effect(() => {
    if (!renaming) return;
    const timer = setTimeout(() => {
      nameInput?.focus();
      // Caret at the end of the old name, nothing selected.
      const field = document.activeElement as HTMLInputElement | null;
      field?.setSelectionRange?.(field.value.length, field.value.length);
    }, 80);
    return () => clearTimeout(timer);
  });
  let target = $state('');
  let title = $state('');
  let busy = $state(false);
  let failure = $state('');
  const targetSession = $derived(workspace.sessions.find((item) => item.id === target));
  const TITLE_MAX = 100;
  const titleError = $derived(
    !title.trim() ? 'The name cannot be empty.' : title.trim().length > TITLE_MAX ? `Keep it under ${TITLE_MAX} characters.` : '',
  );
  const synced = $derived(Boolean(targetSession?.path));
  const toolName = $derived(targetSession?.source === 'codex' ? 'Codex' : 'Claude');

  function askRename() {
    if (!menu) return;
    target = menu.id;
    title = menuSession?.title ?? '';
    failure = '';
    renaming = true;
  }

  /** Confirmation dialog (z-modal "dialog" style); the OK button stays loading until the files are moved. */
  function askDelete() {
    const session = menuSession;
    if (!session) return;
    const tool = session.source === 'codex' ? 'Codex' : 'Claude';
    zModalService.danger({
      zTitle: 'Delete chat?',
      zContent: session.path
        ? `“${session.title}” is also removed from ${tool}: the file moves to ~/.agentdeck/trash, so it can be restored by hand. A chat open in a running CLI cannot be deleted.`
        : `“${session.title}” only exists in AgentDeck and will be removed.`,
      zConfirmStyle: 'dialog',
      zOkText: 'Delete',
      zOkIcon: 'lucideTrash2',
      zCancelText: 'Cancel',
      zWidth: '400px',
      onOk: async () => {
        try {
          await deleteSession(session.id);
          toast.success(`Deleted “${session.title}”`);
        } catch (error) {
          zModalService.warning({ zTitle: 'Could not delete', zContent: String(error), zConfirmStyle: 'dialog', zCancelText: null, zOkText: 'OK' });
        }
        return true;
      },
    });
  }

  async function run(action: () => Promise<void>, close: () => void) {
    busy = true;
    failure = '';
    try {
      await action();
      close();
    } catch (error) {
      failure = String(error);
    } finally {
      busy = false;
    }
  }

  const doRename = () =>
    !titleError &&
    run(
      () => renameSession(target, title),
      () => {
        renaming = false;
        toast.success('Renamed');
      },
    );

  const projectId = (nodeId: string) => nodeId.replace(/^project:/, '');
  function addSession(id: string) {
    if (id === NO_PROJECT) newChat();
    else newSession(id);
  }

  const statusColor = { working: 'bg-success', waiting: 'bg-warning', idle: 'bg-muted-foreground/40' } as const;
  const agentLabel = (agent: string) => agent[0].toUpperCase() + agent.slice(1);
  // Each agent has its own tint so rows are readable at a glance.
  const agentTag = {
    claude: 'bg-orange-500/15 text-orange-500',
    codex: 'bg-emerald-500/15 text-emerald-500',
    gemini: 'bg-sky-500/15 text-sky-500',
  } as const;
</script>

<aside class="flex h-full min-w-0 flex-col bg-sidebar" aria-label="Sessions">
  <!-- Header row shared with the other panes: same height, one continuous bottom line. -->
  <div class="flex h-10 shrink-0 items-center gap-2 border-b border-border px-3" data-tauri-drag-region>
    <Logo class="size-5" />
    <span class="text-xs font-semibold" data-tauri-drag-region>AgentDeck</span>
  </div>

  <div class="border-b border-border px-2 py-2">
    <div class="flex gap-1">
      <ZButton zWave={false} zType="ghost" zSize="sm" class="h-7 min-w-0 flex-1 justify-start gap-2 px-2 text-xs" onclick={() => newSession()}>
        <ZIcon zType="lucidePlus" class="size-3.5" />
        New session
      </ZButton>
      <ZButton zWave={false} zType="ghost" zSize="sm" class="size-7! shrink-0 p-0" aria-label="Search chats (Ctrl+P)" title="Search chats (Ctrl+P)" onclick={() => (searchOpen = true)}>
        <ZIcon zType="lucideSearch" class="size-3.5" />
      </ZButton>
      <ZButton zWave={false} zType="ghost" zSize="sm" class="size-7! shrink-0 p-0" aria-label="New chat without a project" title="New chat without a project" onclick={() => newChat()}>
        <ZIcon zType="lucideMessageSquarePlus" class="size-3.5" />
      </ZButton>
    </div>
  </div>

  <!-- Full width, so the tree's own scrollbar sits flush against the sidebar border. -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div bind:clientHeight={treeHeight} bind:this={treeBox} class="relative min-h-0 flex-1" oncontextmenu={onTreeContextMenu}>
    {#if history.loading && workspace.sessions.length === 0}
      <!-- First load: centred in the list, where the chats will appear. -->
      <div class="absolute inset-0 z-10 flex flex-col items-center justify-center gap-3 px-6 text-center text-xs text-muted-foreground" role="status">
        <ZIcon zType={'lucideLoaderCircle' as ZIconName} class="size-5 animate-spin text-primary" />
        <span>Loading your Claude and Codex history…</span>
      </div>
    {/if}
    <ZTree
      {nodes}
      zSize="sm"
      zSelectable
      zExpandOnClick
      zMaxHeight={treeHeight}
      zLabel="Projects and sessions"
      bind:expanded
      bind:selected
      {onSelect}
    >
      {#snippet node(ctx)}
        {@const data = ctx.node.data}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <span
          class="relative flex min-w-0 flex-1 items-center gap-2 self-stretch"
          data-session={data?.kind === 'session' ? data.session?.id : undefined}
          data-project={data?.kind === 'project' ? projectId(ctx.node.id) : undefined}
          onpointerenter={() => data?.kind === 'session' && prefetchTranscript(data.session)}
        >
          {#if data?.kind === 'session' && data.session}
            <!-- Branch lines (the tree's own guides are off): the project's line, and for a fork the line of its chat,
                 with a tick into the fork's dot. The last fork's line turns the corner and stops. -->
            {#if data.session.parentId}
              <span class="pointer-events-none absolute top-0 bottom-0 -left-[43px] w-px bg-[var(--z-tree-guide)]" aria-hidden="true"></span>
              <span class="pointer-events-none absolute top-0 -left-[29px] w-px bg-[var(--z-tree-guide)] {data.last ? 'h-1/2' : 'bottom-0'}" aria-hidden="true"></span>
              <span class="pointer-events-none absolute top-1/2 -left-[29px] h-px w-[25px] bg-[var(--z-tree-guide)]" aria-hidden="true"></span>
            {:else}
              <span class="pointer-events-none absolute top-0 bottom-0 -left-[29px] w-px bg-[var(--z-tree-guide)]" aria-hidden="true"></span>
            {/if}
            {#if data.session.status === 'working'}
              <span class="grid size-3 shrink-0 place-items-center text-success" role="status" aria-label="Running">
                <ZLoading zLoading zSpinner="orbit" zSize="2xs" zColor="current" />
              </span>
            {:else}
              <span class="size-1.5 shrink-0 rounded-full {statusColor[data.session.status]}" aria-label={data.session.status}></span>
            {/if}
            <span class="min-w-0 flex-1 truncate {ctx.selected ? 'font-medium text-primary' : ''}">{ctx.node.label}</span>
            <span class="grid shrink-0 place-items-center">
              <span class="col-start-1 row-start-1">
                {#if (data.session.agents?.length ?? 1) > 1}
                  <AgentStack agents={data.session.agents!} />
                {:else}
                  <span class="block rounded px-1.5 py-px text-[10px] font-medium {agentTag[data.session.agent]}">{agentLabel(data.session.agent)}</span>
                {/if}
              </span>
            </span>
          {:else}
            <ZIcon zType={ctx.node.icon ?? 'lucideFolder'} class="size-3.5 shrink-0 text-muted-foreground" />
            <span title={data?.path} class="min-w-0 flex-1 truncate {data?.kind === 'project' ? 'font-medium' : 'text-muted-foreground'}">{ctx.node.label}</span>
            {#if data?.kind === 'project'}
              <button
                type="button"
                class="inline-flex shrink-0 cursor-pointer rounded p-0.5 text-muted-foreground opacity-0 transition-opacity outline-none group-hover/tree-row:opacity-100 group-focus-within/tree-row:opacity-100 hover:bg-accent hover:text-foreground"
                aria-label="New session in {ctx.node.label}"
                title="New session"
                onclick={(event) => {
                  event.stopPropagation();
                  addSession(projectId(ctx.node.id));
                }}
              >
                <ZIcon zType="lucidePlus" class="size-3.5" />
              </button>
            {/if}
          {/if}
        </span>
      {/snippet}
    </ZTree>
  </div>

  {#if layout.docked.sidebar.length > 0}
    <Splitter axis="y" label="Resize sidebar zone" onDelta={(delta) => (layout.sizes.sidebarZone = Math.min(Math.max(layout.sizes.sidebarZone - delta, 96), window.innerHeight - 320))} />
    <div class="flex shrink-0" style:height="{layout.sizes.sidebarZone}px"><DockZone zone="sidebar" /></div>
  {/if}

  <div class="flex h-11 shrink-0 items-center gap-2 px-3 text-xs">
    <span class="grid size-6 shrink-0 place-items-center rounded-full bg-muted text-[10px]">ZO</span>
    <span class="truncate">ZUI Organization</span>
    <ZButton zWave={false} zType="ghost" zSize="sm" class="ml-auto h-6 w-6" aria-label="Account menu">
      <ZIcon zType="lucideEllipsis" class="size-3.5" />
    </ZButton>
  </div>

  <!-- One shared menu, anchored to a 1px box at the click (or at the three dots). -->
  {#if menu}
    {#key menuKey}
    <div class="pointer-events-none fixed z-50 size-px" style:left="{menu.x}px" style:top="{menu.y}px">
      <ZDropdownMenu zItems={menuItems} bind:open={menuOpen} zPosition={menu.align} zOffset={4} zMinWidth={160} zContentClass="p-1 text-xs">
        <span class="block size-px"></span>
      </ZDropdownMenu>
    </div>
    {/key}
  {/if}
</aside>

<SearchPalette bind:open={searchOpen} />

<ZModal bind:open={renaming} zTitle="Rename chat" zWidth="480px" zBodyClass="min-h-0! py-3" zOkText="Save" zCancelText="Cancel" zLoading={busy} zOkDisabled={Boolean(titleError)} onOk={doRename}>
  <div class="flex flex-col gap-2">
    <ZInput bind:this={nameInput} bind:value={title} zPlaceholder="Chat name" onEnter={doRename} />
    <div class="flex items-center justify-between text-xs">
      <span class="text-destructive" role="alert">{title.length > 0 || title !== (targetSession?.title ?? '') ? titleError : ''}</span>
      <span class="text-muted-foreground tabular-nums">{title.trim().length}/{TITLE_MAX}</span>
    </div>
    {#if synced}
      <p class="text-xs text-muted-foreground">The new name is also written to the original {toolName} session.</p>
    {/if}
    {#if failure}<p class="text-xs text-destructive" role="alert">{failure}</p>{/if}
  </div>
</ZModal>
