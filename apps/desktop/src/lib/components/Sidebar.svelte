<script lang="ts">
  import { ZButton, ZIcon, ZTree } from '@shival99/z-ui-svelte';
  import type { ZTreeNode } from '@shival99/z-ui-svelte';
  import { buildTree, forkSession, newSession, workspace, type TreeData } from '../sessions.svelte';

  const nodes = $derived(buildTree());

  // Everything starts open; a fork opens its parent so the new row is visible.
  let expanded = $state<string[]>([
    ...workspace.projects.map((project) => `project:${project.id}`),
    ...workspace.sessions.map((item) => `session:${item.id}`),
  ]);

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
  <!-- The scroll container spans the whole sidebar so its scrollbar sits flush against the border; padding lives inside. -->
  <div class="min-h-0 flex-1 overflow-y-auto">
    <div class="px-1.5 pb-2">
    <ZTree
      {nodes}
      zSize="sm"
      zShowLine
      zSelectable
      zExpandOnClick
      zMaxHeight="none"
      zLabel="Projects and sessions"
      bind:expanded
      bind:selected
      {onSelect}
    >
      {#snippet node(ctx)}
        {@const data = ctx.node.data}
        <span class="flex min-w-0 flex-1 items-center gap-2">
          {#if data?.kind === 'session' && data.session}
            <span class="size-1.5 shrink-0 rounded-full {statusColor[data.session.status]}" aria-label={data.session.status}></span>
            <span class="min-w-0 flex-1 truncate {ctx.selected ? 'font-medium text-primary' : ''}">{ctx.node.label}</span>
            <span class="shrink-0 text-[10px] text-muted-foreground group-hover/row:hidden">{agentLabel(data.session.agent)}</span>
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
            <span class="min-w-0 flex-1 truncate {data?.kind === 'project' ? 'font-medium' : 'text-muted-foreground'}">{ctx.node.label}</span>
          {/if}
        </span>
      {/snippet}
    </ZTree>
    </div>
  </div>

  <div class="flex h-11 shrink-0 items-center gap-2 px-3 text-xs">
    <span class="grid size-6 shrink-0 place-items-center rounded-full bg-muted text-[10px]">JD</span>
    <span class="truncate">Jane Developer</span>
    <ZButton zWave={false} zType="ghost" zSize="sm" class="ml-auto h-6 w-6" aria-label="Account menu">
      <ZIcon zType="lucideEllipsis" class="size-3.5" />
    </ZButton>
  </div>
</aside>
