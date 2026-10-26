<script lang="ts">
  import { ZIcon } from '@shival99/z-ui-svelte';
  import { toolOf } from '../tools';
  import BrowserView from './BrowserView.svelte';
  import DevTools from './DevTools.svelte';
  import FileViewer from './FileViewer.svelte';
  import TasksView from './TasksView.svelte';
  import McpView from './McpView.svelte';
  import WorkflowView from './WorkflowView.svelte';
  import UsageView from './UsageView.svelte';
  import ImageGenerator from './ImageGenerator.svelte';

  // The body of a workspace tool. Same view in the right panel and in the detached modal.
  let { tool }: { tool: string } = $props();
  const current = $derived(toolOf(tool));
</script>

{#if tool === 'Explore' || tool === 'Files' || tool === 'Diff'}
  <FileViewer />
{:else if tool === 'Preview'}
  <BrowserView />
{:else if tool === 'Images'}
  <ImageGenerator />
{:else if tool === 'Tasks' || tool === 'Audit'}
  <TasksView />
{:else if tool === 'Workflow'}
  <WorkflowView />
{:else if tool === 'DevTools'}
  <DevTools embedded />
{:else if tool === 'MCP'}
  <McpView />
{:else if tool === 'Usage'}
  <UsageView />
{:else}

<div class="flex h-full min-h-0 flex-1 flex-col items-center justify-center gap-3 p-6 text-center text-muted-foreground" role="tabpanel" aria-label={current.label}>
  <ZIcon zType={current.icon} class="size-8 opacity-60" />
  <h2 class="text-sm font-medium text-foreground">{current.label}</h2>
  <p class="m-0 max-w-56 text-xs leading-normal">
    {current.value === 'Preview' ? 'Your browser preview will appear here.' : `No ${current.label.toLowerCase()} data yet.`}
  </p>
</div>
{/if}
