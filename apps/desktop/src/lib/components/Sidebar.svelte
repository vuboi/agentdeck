<script lang="ts">
  import { untrack } from 'svelte';
  import { ZButton, ZIcon, ZLoading, ZTree } from '@shival99/z-ui-svelte';
  import type { ZTreeNode } from '@shival99/z-ui-svelte';
  import AgentStack from './AgentStack.svelte';
  import { buildTree, forkSession, history, newSession, workspace, type TreeData } from '../sessions.svelte';

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
  </div>

  <div class="px-2 pt-2">
    <ZButton zWave={false} zType="ghost" zSize="sm" class="h-7 w-full justify-start gap-2 px-2 text-xs" onclick={() => newSession()}>
      <ZIcon zType="lucidePlus" class="size-3.5" />
      New session
    </ZButton>
  </div>

  <div class="px-4 pt-3 pb-1.5 font-mono text-[10px] tracking-widest text-muted-foreground">PROJECTS</div>
  {#if history.loading && workspace.sessions.length === 0}
    <div class="flex items-center gap-2 px-4 py-2 text-xs text-muted-foreground" role="status">
      <ZLoading zSpinner="orbit" zSize="xs" zColor="primary" />
      Loading your Claude and Codex history…
    </div>
  {/if}
  <!-- Full width, so the tree's own scrollbar sits flush against the sidebar border. -->
  <div bind:clientHeight={treeHeight} class="min-h-0 flex-1">
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
        <span class="relative flex min-w-0 flex-1 items-center gap-2">
          {#if data?.kind === 'session' && data.session}
            <!-- A fork hangs off its parent: an elbow drops from the parent's dot to this row. -->
            {#if data.session.parentId}
              <span class="pointer-events-none absolute bottom-1/2 -left-[11.5px] h-7 w-[9.5px] border-b border-l border-muted-foreground/40" aria-hidden="true"></span>
            {/if}
            <span class="size-1.5 shrink-0 rounded-full {statusColor[data.session.status]}" aria-label={data.session.status}></span>
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
              aria-label="Fork session"
              title="Fork session"
              onclick={(event) => {
                event.stopPropagation();
                fork(data.session!.id);
              }}
            >
              <ZIcon zType="lucideGitFork" class="size-3.5" />
            </button>
          {:else}
            <ZIcon zType={ctx.node.icon ?? 'lucideFolder'} class="size-3.5 shrink-0 text-muted-foreground" />
            <span title={data?.path} class="min-w-0 flex-1 truncate {data?.kind === 'project' ? 'font-medium' : 'text-muted-foreground'}">{ctx.node.label}</span>
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
</aside>
