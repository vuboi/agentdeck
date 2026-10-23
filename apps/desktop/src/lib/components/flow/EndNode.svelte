<script lang="ts">
  import { Handle, Position, type NodeProps } from '@xyflow/svelte';
  import { ZIcon } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import type { EndData } from './flow-graph';

  // The two ends of a flow: the task it is given (top) and the orchestrator, the chat's own agent, that writes the
  // final answer from the steps' handoffs (bottom).
  let { data, selected }: NodeProps & { data: EndData } = $props();
  const icon = (name: string) => name as ZIconName;
  const start = $derived(data.kind === 'start');
  const sub = $derived.by(() => {
    if (start) return data.run ? data.run.task.split('\n')[0] : 'The message you send';
    if (data.mode === 'edit') return 'Combines the handoffs';
    return data.state === 'running' ? 'Writing the final answer' : data.state === 'done' ? 'Answered' : data.state === 'failed' ? 'Failed' : 'Waits for every step';
  });
</script>

<div class="flow-node flow-end state-{data.state} {selected ? 'is-selected' : ''}" style="width:224px">
  {#if !start}<Handle type="target" position={Position.Top} class="flow-handle" isConnectable={false} />{/if}
  <div class="flex items-center gap-2.5 px-3 py-2.5">
    <span class="grid size-8 shrink-0 place-items-center rounded-full {start ? 'bg-primary/12 text-primary' : 'bg-violet-500/12 text-violet-600 dark:text-violet-400'}">
      <ZIcon zType={icon(start ? 'lucideMessageSquare' : 'lucideBot')} class="size-4" />
    </span>
    <span class="min-w-0 flex-1">
      <span class="block text-[13px] leading-tight font-semibold text-foreground">{start ? 'Task' : 'Orchestrator'}</span>
      <span class="block truncate text-[11px] text-muted-foreground">{sub}</span>
    </span>
    {#if data.state === 'running'}
      <ZIcon zType={icon('lucideLoaderCircle')} class="size-3.5 shrink-0 animate-spin text-primary" />
    {:else if data.state === 'done' && !start}
      <ZIcon zType={icon('lucideCircleCheck')} class="size-3.5 shrink-0 text-emerald-500" />
    {/if}
  </div>
  {#if start}<Handle type="source" position={Position.Bottom} class="flow-handle" />{/if}
</div>
