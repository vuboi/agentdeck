<script lang="ts">
  import { SvelteFlowProvider } from '@xyflow/svelte';
  import type { DelegateRun } from '../../../chat/sessions.svelte';
  import type { WorkflowStep } from '../../../tasks/workflows.svelte';
  import FlowInner from './FlowInner.svelte';

  // A flow drawn as a canvas (Svelte Flow): edited in the full-screen editor, watched live in the Workflow tab.
  let {
    steps = $bindable(),
    selected = $bindable(),
    mode = 'edit',
    run,
    minimap = false,
    onlayout,
    team = false,
    class: className = '',
  }: { steps: WorkflowStep[]; selected?: string | null; mode?: 'edit' | 'run'; run?: DelegateRun; minimap?: boolean; onlayout?: () => void; team?: boolean; class?: string } = $props();
</script>

<div class="relative min-h-0 {className}">
  <SvelteFlowProvider>
    <FlowInner bind:steps bind:selected {mode} {run} {minimap} {onlayout} {team} />
  </SvelteFlowProvider>
</div>
