<script lang="ts">
  import { ZIcon, ZPopover } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { inTauri } from '../history';
  import { activeSession, isRunning, isUnstarted, moveDraftToProject, NO_PROJECT, workspace } from '../sessions.svelte';
  import { toast } from '../toast';

  // Under the message box: the project the chat works in (switchable while the chat is new, "This computer" by default)
  // and the git branch of that project, which can be switched. A chat that has started keeps its folder.
  const session = $derived(activeSession());
  const project = $derived(workspace.projects.find((item) => item.id === session?.projectId));
  const canMove = $derived(isUnstarted(session));
  const running = $derived(session ? isRunning(session.id) : false);
  const projectName = (id: string) => (id === NO_PROJECT ? 'This computer' : (workspace.projects.find((item) => item.id === id)?.name ?? id));
  // "This computer" first, then the projects the sidebar shows.
  const choices = $derived(
    [...workspace.projects].sort((a, b) => Number(b.id === NO_PROJECT) - Number(a.id === NO_PROJECT)).map((item) => ({ id: item.id, name: projectName(item.id), path: item.path })),
  );

  const SEARCH_FROM = 6;
  const icon = (name: string) => name as ZIconName;

  // ---- project ----
  let projectTrigger = $state<HTMLElement>();
  let projectOpen = $state(false);
  let projectQuery = $state('');
  let projectSearch = $state<HTMLInputElement>();
  const shownProjects = $derived(choices.filter((item) => item.name.toLowerCase().includes(projectQuery.trim().toLowerCase())));
  $effect(() => {
    if (projectOpen) requestAnimationFrame(() => projectSearch?.focus());
  });
  function pickProject(id: string) {
    if (session) moveDraftToProject(session.id, id);
    projectOpen = false;
    projectQuery = '';
    requestAnimationFrame(() => document.querySelector<HTMLTextAreaElement>('main[aria-label="Conversation"] form textarea')?.focus());
  }

  // ---- git branch ----
  type Branches = { current: string | null; branches: string[] };
  let branches = $state<Branches | null>(null);
  let branchTrigger = $state<HTMLElement>();
  let branchOpen = $state(false);
  let branchQuery = $state('');
  let branchSearch = $state<HTMLInputElement>();
  let switching = $state(false);
  const shownBranches = $derived((branches?.branches ?? []).filter((name) => name.toLowerCase().includes(branchQuery.trim().toLowerCase())));
  $effect(() => {
    if (branchOpen) requestAnimationFrame(() => branchSearch?.focus());
  });

  async function readBranches(path: string | undefined) {
    if (!path || !inTauri()) {
      branches = null;
      return;
    }
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      const found = await invoke<Branches>('git_branches', { root: path });
      if (project?.path === path) branches = found;
    } catch {
      // Not a repository (or git is missing): there is no branch to show.
      if (project?.path === path) branches = null;
    }
  }
  // Read again whenever the project changes, and when a reply finishes (the agent may have switched branch itself).
  $effect(() => {
    const path = project?.path;
    void running;
    untrackRead(path);
  });
  function untrackRead(path: string | undefined) {
    void readBranches(path);
  }

  async function switchTo(name: string) {
    if (!project?.path || switching || name === branches?.current) {
      branchOpen = false;
      return;
    }
    switching = true;
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('git_switch', { root: project.path, branch: name });
      toast.success(`Switched to ${name}`);
      branchOpen = false;
      branchQuery = '';
    } catch (error) {
      // Git's own words: usually local changes that the other branch would overwrite.
      toast.error(String(error).replace(/^error:\s*/i, '').split('\n')[0]);
    } finally {
      switching = false;
      await readBranches(project?.path);
    }
  }
</script>

{#if session}
  <div class="mx-auto mt-1.5 flex min-w-0 max-w-full items-center gap-1 px-1 text-xs text-muted-foreground">
    <!-- Project -->
    {#if canMove}
      <button
        bind:this={projectTrigger}
        type="button"
        class="inline-flex h-6 min-w-0 max-w-48 cursor-pointer items-center gap-1.5 rounded-md px-1.5 outline-none hover:bg-accent hover:text-foreground {projectOpen ? 'bg-accent text-foreground' : ''}"
        aria-label="Project"
        aria-haspopup="listbox"
        aria-expanded={projectOpen}
        onclick={() => (projectOpen = !projectOpen)}
      >
        <ZIcon zType={icon(session.projectId === NO_PROJECT ? 'lucideMonitor' : 'lucideFolder')} class="size-3.5 shrink-0" />
        <span class="truncate">{projectName(session.projectId)}</span>
        <ZIcon zType={icon('lucideChevronDown')} class="size-3 shrink-0" />
      </button>
    {:else}
      <span class="inline-flex h-6 min-w-0 max-w-48 items-center gap-1.5 px-1.5" title="A chat stays in its folder once it has started">
        <ZIcon zType={icon(session.projectId === NO_PROJECT ? 'lucideMonitor' : 'lucideFolder')} class="size-3.5 shrink-0" />
        <span class="truncate">{projectName(session.projectId)}</span>
      </span>
    {/if}

    <!-- Git branch -->
    {#if branches}
      <button
        bind:this={branchTrigger}
        type="button"
        class="inline-flex h-6 min-w-0 max-w-48 cursor-pointer items-center gap-1.5 rounded-md px-1.5 outline-none hover:bg-accent hover:text-foreground disabled:cursor-not-allowed disabled:opacity-50 {branchOpen ? 'bg-accent text-foreground' : ''}"
        aria-label="Git branch"
        aria-haspopup="listbox"
        aria-expanded={branchOpen}
        title={running ? 'Wait for the reply to finish before switching branch' : 'Switch branch'}
        disabled={running}
        onclick={() => (branchOpen = !branchOpen)}
      >
        <ZIcon zType={icon('lucideGitBranch')} class="size-3.5 shrink-0" />
        <span class="truncate">{branches.current ?? 'detached HEAD'}</span>
        <ZIcon zType={icon(switching ? 'lucideLoaderCircle' : 'lucideChevronDown')} class="size-3 shrink-0 {switching ? 'animate-spin' : ''}" />
      </button>
    {/if}
  </div>

  <ZPopover anchor={projectTrigger} bind:open={projectOpen} zTrigger="manual" zPosition="top-left" zOffset={6} zWidth={200}>
    {#snippet children()}
      <div class="flex w-full flex-col text-xs" role="listbox" aria-label="Project">
        {#if choices.length >= SEARCH_FROM}
          <div class="flex h-9 items-center gap-2 border-b border-border px-3">
            <ZIcon zType={icon('lucideSearch')} class="size-3.5 shrink-0 text-muted-foreground" />
            <input class="min-w-0 flex-1 bg-transparent text-xs outline-none placeholder:text-muted-foreground" placeholder="Search projects…" aria-label="Search projects" bind:this={projectSearch} bind:value={projectQuery} />
          </div>
        {/if}
        <div class="flex max-h-44 flex-col gap-0.5 overflow-y-auto p-1">
          {#each shownProjects as item (item.id)}
            <button type="button" role="option" aria-selected={item.id === session.projectId} class="flex w-full cursor-pointer items-center gap-2 rounded-sm px-2 py-1 text-left outline-none hover:bg-accent {item.id === session.projectId ? 'bg-primary/15 text-primary' : ''}" onclick={() => pickProject(item.id)}>
              <ZIcon zType={icon(item.id === NO_PROJECT ? 'lucideMonitor' : 'lucideFolder')} class="size-3.5 shrink-0 text-muted-foreground" />
              <span class="min-w-0 flex-1 truncate">{item.name}</span>
              {#if item.id === session.projectId}<ZIcon zType={icon('lucideCheck')} class="size-3.5 shrink-0" />{/if}
            </button>
          {:else}
            <p class="m-0 px-2 py-2 text-center text-muted-foreground">No project matches</p>
          {/each}
        </div>
      </div>
    {/snippet}
  </ZPopover>

  <ZPopover anchor={branchTrigger} bind:open={branchOpen} zTrigger="manual" zPosition="top-left" zOffset={6} zWidth={180}>
    {#snippet children()}
      <div class="flex w-full flex-col text-xs" role="listbox" aria-label="Git branch">
        {#if (branches?.branches.length ?? 0) >= SEARCH_FROM}
          <div class="flex h-9 items-center gap-2 border-b border-border px-3">
            <ZIcon zType={icon('lucideSearch')} class="size-3.5 shrink-0 text-muted-foreground" />
            <input class="min-w-0 flex-1 bg-transparent text-xs outline-none placeholder:text-muted-foreground" placeholder="Search branches…" aria-label="Search branches" bind:this={branchSearch} bind:value={branchQuery} />
          </div>
        {/if}
        <!-- Fits its branches, and scrolls past a maximum height. -->
        <div class="flex max-h-44 flex-col gap-0.5 overflow-y-auto p-1">
          {#each shownBranches as name (name)}
            <button type="button" role="option" aria-selected={name === branches?.current} class="flex w-full cursor-pointer items-center gap-2 rounded-sm px-2 py-1 text-left outline-none hover:bg-accent {name === branches?.current ? 'bg-primary/15 text-primary' : ''}" disabled={switching} onclick={() => void switchTo(name)}>
              <ZIcon zType={icon('lucideGitBranch')} class="size-3.5 shrink-0 text-muted-foreground" />
              <span class="min-w-0 flex-1 truncate">{name}</span>
              {#if name === branches?.current}<ZIcon zType={icon('lucideCheck')} class="size-3.5 shrink-0" />{/if}
            </button>
          {:else}
            <p class="m-0 px-2 py-2 text-center text-muted-foreground">No branch matches</p>
          {/each}
        </div>
      </div>
    {/snippet}
  </ZPopover>
{/if}
