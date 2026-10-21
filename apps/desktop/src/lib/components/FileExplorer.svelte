<script lang="ts">
  import { ZIcon, ZTree } from '@shival99/z-ui-svelte';
  import type { ZIconName, ZTreeNode } from '@shival99/z-ui-svelte';
  import { activeSession, workspace } from '../sessions.svelte';
  import { openFileTab } from '../content-tabs.svelte';

  // Files of the active session's project. Mock listing until agentd reads the real folder.
  type File = { name: string; children?: File[] };
  const mockFiles: Record<string, File[]> = {
    agentdeck: [
      { name: 'apps', children: [{ name: 'desktop', children: [{ name: 'src', children: [{ name: 'lib', children: [{ name: 'components' }, { name: 'sessions.svelte.ts' }] }, { name: 'routes' }] }, { name: 'package.json' }] }] },
      { name: 'crates', children: [{ name: 'protocol' }, { name: 'store' }, { name: 'core' }] },
      { name: 'AGENTS.md' },
      { name: 'PLAN.md' },
      { name: 'README.md' },
    ],
  };

  const projectName = $derived(workspace.projects.find((project) => project.id === activeSession()?.projectId)?.name ?? '');

  // Same shape as the sidebar tree: folders expand, files are leaves.
  function toNodes(files: File[], parent = ''): ZTreeNode<null>[] {
    return files.map((file) => {
      const id = `${parent}/${file.name}`;
      return {
        id,
        label: file.name,
        icon: (file.children ? 'lucideFolder' : 'lucideFile') as ZIconName,
        children: file.children ? toNodes(file.children, id) : undefined,
      };
    });
  }

  const files = $derived(mockFiles[projectName] ?? []);
  const nodes = $derived(toNodes(files));
  let treeHeight = $state(400);
  let treeBox: HTMLDivElement | undefined = $state();
  let expanded = $state<string[]>([]);

  // A file opens as a tab over the conversation; folders only expand.
  function onSelect(node: ZTreeNode<null>) {
    if (node.children) return;
    // Tree ids are paths inside the project, e.g. /apps/desktop/package.json.
    const root = workspace.projects.find((project) => project.id === activeSession()?.projectId)?.path ?? '';
    if (root) openFileTab(`${root}${node.id}`, root);
  }
</script>

{#if nodes.length === 0}
  <div class="flex h-full min-h-0 flex-1 flex-col items-center justify-center gap-3 p-6 text-center text-muted-foreground" role="tabpanel" aria-label="Explore">
    <ZIcon zType="lucideFolder" class="size-8 opacity-60" />
    <h2 class="text-sm font-medium text-foreground">Explore</h2>
    <p class="m-0 max-w-56 text-xs leading-normal">No files yet.</p>
  </div>
{:else}
  <div bind:clientHeight={treeHeight} bind:this={treeBox} class="min-h-0 flex-1 py-1.5">
    <ZTree {nodes} zSize="sm" zShowLine zExpandOnClick zMaxHeight={treeHeight} zLabel="Project files" bind:expanded {onSelect}>
      {#snippet node(ctx)}
        <ZIcon zType={(ctx.node.icon ?? 'lucideFile') as ZIconName} class="size-3.5 shrink-0 text-muted-foreground" />
        <span class="min-w-0 flex-1 truncate">{ctx.node.label}</span>
      {/snippet}
    </ZTree>
  </div>
{/if}
