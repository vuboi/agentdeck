<script lang="ts">
  import { untrack } from 'svelte';
  import { ZButton, ZDropdownMenu, ZIcon, ZInput, ZLoading, ZModal, ZTree } from '@shival99/z-ui-svelte';
  import type { ZDropdownMenuItem, ZTreeNode } from '@shival99/z-ui-svelte';
  import AgentStack from './AgentStack.svelte';
  import SearchPalette from './SearchPalette.svelte';
  import {
    buildTree,
    deleteSession,
    forkSession,
    history,
    newChat,
    newSession,
    NO_PROJECT,
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

  let selected = $state<string[]>([]);
  $effect(() => {
    selected = [`session:${workspace.activeId}`];
  });

  function onSelect(node: ZTreeNode<TreeData>) {
    // Projects only expand; only sessions switch the chat.
    if (node.data?.kind === 'session' && node.data.session) workspace.activeId = node.data.session.id;
    else selected = [`session:${workspace.activeId}`];
  }

  function fork(sessionId: string) {
    forkSession(sessionId);
    expanded = [...new Set([...expanded, `session:${sessionId}`])];
  }

  // ---------- row menu: right click, or the three dots that replace the agent tag on hover ----------
  let menu = $state<{ x: number; y: number; id: string; align: 'bottom-left' | 'bottom-right' } | null>(null);
  let menuOpen = $state(false);
  let menuKey = $state(0);
  let treeBox: HTMLDivElement | undefined = $state();

  // Right click anywhere on a row, not just on its text. Everywhere else in the tree the browser's own
  // menu (Inspect…) is suppressed too.
  function onTreeContextMenu(event: MouseEvent) {
    event.preventDefault();
    const row = (event.target as HTMLElement).closest('[role="treeitem"]');
    const id = row?.querySelector<HTMLElement>('[data-session]')?.dataset.session;
    if (id) void openMenu(event, id);
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

  async function openMenu(event: MouseEvent, id: string, anchor?: HTMLElement) {
    event.preventDefault();
    event.stopPropagation();
    menuKey++; // a fresh menu at the new spot: no close-then-open flicker
    if (anchor) {
      const rect = anchor.getBoundingClientRect();
      menu = { x: rect.right, y: rect.bottom, id, align: 'bottom-right' };
    } else {
      menu = { x: event.clientX, y: event.clientY, id, align: 'bottom-left' };
    }
    menuOpen = true;
  }

  const menuItems = $derived<ZDropdownMenuItem[]>([
    { label: 'Rename', icon: 'lucidePencil', iconSize: '12', shortcut: 'R', class: 'text-xs! py-1', onClick: () => askRename() },
    { label: 'Fork', icon: 'lucideGitFork', iconSize: '12', shortcut: 'F', class: 'text-xs! py-1', onClick: () => menu && fork(menu.id) },
    { label: 'Delete', icon: 'lucideTrash2', iconSize: '12', shortcut: 'D', divide: 'before', class: 'text-xs! py-1 text-destructive', onClick: () => askDelete() },
  ]);

  // The letters on the right are real shortcuts while the menu is open.
  $effect(() => {
    if (!menuOpen) return;
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
  let deleting = $state(false);
  let target = $state('');
  let title = $state('');
  let busy = $state(false);
  let failure = $state('');
  const targetSession = $derived(workspace.sessions.find((item) => item.id === target));
  const synced = $derived(Boolean(targetSession?.path));
  const toolName = $derived(targetSession?.source === 'codex' ? 'Codex' : 'Claude');

  function askRename() {
    if (!menu) return;
    target = menu.id;
    title = menuSession?.title ?? '';
    failure = '';
    renaming = true;
  }

  function askDelete() {
    if (!menu) return;
    target = menu.id;
    failure = '';
    deleting = true;
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

  const doRename = () => run(() => renameSession(target, title), () => (renaming = false));
  const doDelete = () => run(() => deleteSession(target), () => (deleting = false));

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
    <span class="grid size-5 place-items-center rounded-md bg-primary text-[11px] font-bold text-primary-foreground" data-tauri-drag-region>A</span>
    <span class="text-xs font-semibold" data-tauri-drag-region>AgentDeck</span>
    <ZButton zWave={false} zType="ghost" zSize="sm" class="ml-auto h-6 w-6 px-0" aria-label="Search chats (Ctrl+K)" title="Search chats (Ctrl+K)" onclick={() => (searchOpen = true)}>
      <ZIcon zType="lucideSearch" class="size-3.5" />
    </ZButton>
  </div>

  <div class="border-b border-border px-2 py-2">
    <div class="flex gap-1">
      <ZButton zWave={false} zType="ghost" zSize="sm" class="h-7 min-w-0 flex-1 justify-start gap-2 px-2 text-xs" onclick={() => newSession()}>
        <ZIcon zType="lucidePlus" class="size-3.5" />
        New session
      </ZButton>
      <ZButton zWave={false} zType="ghost" zSize="sm" class="size-7! shrink-0 p-0" aria-label="New chat without a project" title="New chat without a project" onclick={() => newChat()}>
        <ZIcon zType="lucideMessageSquarePlus" class="size-3.5" />
      </ZButton>
    </div>
  </div>

  {#if history.loading && workspace.sessions.length === 0}
    <div class="flex items-center gap-2 px-4 py-2 text-xs text-muted-foreground" role="status">
      <ZLoading zSpinner="orbit" zSize="xs" zColor="primary" />
      Loading your Claude and Codex history…
    </div>
  {/if}
  <!-- Full width, so the tree's own scrollbar sits flush against the sidebar border. -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div bind:clientHeight={treeHeight} bind:this={treeBox} class="min-h-0 flex-1" oncontextmenu={onTreeContextMenu}>
    <ZTree
      {nodes}
      zSize="sm"
      zShowLine
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
          class="relative flex min-w-0 flex-1 items-center gap-2"
          data-session={data?.kind === 'session' ? data.session?.id : undefined}
        >
          {#if data?.kind === 'session' && data.session}
            <!-- A fork hangs off its parent: an elbow drops from the parent's dot to this row. -->
            {#if data.session.parentId}
              <span class="pointer-events-none absolute bottom-1/2 -left-[11.5px] h-[23px] w-[9.5px] border-b border-l border-muted-foreground/40" aria-hidden="true"></span>
            {/if}
            {#if data.session.status === 'working'}
              <span class="grid size-3 shrink-0 place-items-center text-success" role="status" aria-label="Running">
                <ZLoading zSpinner="orbit" zSize="2xs" zColor="current" />
              </span>
            {:else}
              <span class="size-1.5 shrink-0 rounded-full {statusColor[data.session.status]}" aria-label={data.session.status}></span>
            {/if}
            <span class="min-w-0 flex-1 truncate {ctx.selected ? 'font-medium text-primary' : ''}">{ctx.node.label}</span>
            <!-- One agent: a coloured name. Several agents in one chat: overlapping logos. -->
            {#if (data.session.agents?.length ?? 1) > 1}
              <span class="group-hover/row:hidden"><AgentStack agents={data.session.agents!} /></span>
            {:else}
              <span class="shrink-0 rounded px-1.5 py-px text-[10px] font-medium group-hover/row:hidden {agentTag[data.session.agent]}">{agentLabel(data.session.agent)}</span>
            {/if}
            <button
              type="button"
              class="hidden shrink-0 cursor-pointer rounded p-0.5 text-muted-foreground hover:bg-accent hover:text-foreground group-hover/row:inline-flex"
              aria-label="Session actions"
              aria-haspopup="menu"
              onclick={(event) => openMenu(event, data.session!.id, event.currentTarget)}
            >
              <ZIcon zType="lucideEllipsis" class="size-3.5" />
            </button>
          {:else}
            <ZIcon zType={ctx.node.icon ?? 'lucideFolder'} class="size-3.5 shrink-0 text-muted-foreground" />
            <span title={data?.path} class="min-w-0 flex-1 truncate {data?.kind === 'project' ? 'font-medium' : 'text-muted-foreground'}">{ctx.node.label}</span>
            {#if data?.kind === 'project'}
              <button
                type="button"
                class="hidden shrink-0 cursor-pointer rounded p-0.5 text-muted-foreground hover:bg-accent hover:text-foreground group-hover/row:inline-flex"
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

  <div class="flex h-11 shrink-0 items-center gap-2 px-3 text-xs">
    <span class="grid size-6 shrink-0 place-items-center rounded-full bg-muted text-[10px]">JD</span>
    <span class="truncate">Jane Developer</span>
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

<ZModal bind:open={renaming} zTitle="Rename chat" zWidth="420px" zOkText="Save" zCancelText="Cancel" zLoading={busy} zOkDisabled={!title.trim()} onOk={doRename}>
  <div class="flex flex-col gap-2">
    <ZInput bind:value={title} zPlaceholder="Chat name" onEnter={doRename} />
    {#if synced}
      <p class="text-xs text-muted-foreground">The new name is also written to the original {toolName} session.</p>
    {/if}
    {#if failure}<p class="text-xs text-destructive" role="alert">{failure}</p>{/if}
  </div>
</ZModal>

<ZModal bind:open={deleting} zTitle="Delete chat" zWidth="440px" zOkText="Delete" zOkDestructive zCancelText="Cancel" zLoading={busy} onOk={doDelete}>
  <div class="flex flex-col gap-2 text-sm">
    <p>Delete “{targetSession?.title}”?</p>
    <p class="text-xs text-muted-foreground">
      {#if synced}
        It is removed from {toolName} too: the file is moved to <code>~/.agentdeck/trash</code>, so it can still be restored by hand. A chat open in a running CLI cannot be deleted.
      {:else}
        This chat only exists in AgentDeck.
      {/if}
    </p>
    {#if failure}<p class="text-xs text-destructive" role="alert">{failure}</p>{/if}
  </div>
</ZModal>
