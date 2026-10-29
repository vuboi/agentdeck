<script lang="ts">
  import { ZIcon } from '@shival99/z-ui-svelte';
  import { toolOf } from '../../chat/content/tools';
  import BrowserView from '../browser/BrowserView.svelte';
  import DevTools from '../browser/DevTools.svelte';
  import FileViewer from '../files/FileViewer.svelte';
  import TasksView from '../tasks/TasksView.svelte';
  import McpView from '../settings/McpView.svelte';
  import StoreView from '../store/StoreView.svelte';
  import WorkflowView from '../tasks/WorkflowView.svelte';
  import UsageView from '../settings/UsageView.svelte';
  import ImageGenerator from '../files/ImageGenerator.svelte';

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
{:else if tool === 'Store'}
  <StoreView />
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
