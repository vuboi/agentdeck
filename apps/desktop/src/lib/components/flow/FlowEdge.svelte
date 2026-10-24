<script lang="ts">
  import { BaseEdge, EdgeLabel, EdgeReconnectAnchor, getBezierPath, type EdgeProps } from '@xyflow/svelte';
  import type { EdgeData } from './flow-graph';

  // A hand-over between two cards: a soft curve with an arrow. While the next card works, light runs down it.
  let { id, sourceX, sourceY, targetX, targetY, sourcePosition, targetPosition, markerEnd, data, selected }: EdgeProps & { data: EdgeData } = $props();
  let reconnecting = $state(false);
  const path = $derived(getBezierPath({ sourceX, sourceY, sourcePosition, targetX, targetY, targetPosition, curvature: data.state === 'talk' || data.state === 'talking' ? 0.9 : 0.18 }));
</script>

{#if data.state === 'active' || data.state === 'talking'}<path d={path[0]} class="flow-edge-glow" fill="none" />{/if}
{#if !reconnecting}<BaseEdge {id} path={path[0]} {markerEnd} class="flow-edge flow-edge-{data.state} {selected ? 'is-selected' : ''}" interactionWidth={22} />{/if}
{#if data.state === 'active' || data.state === 'talking'}
  {#each [0, 1, 2] as n (n)}
    <circle r="3.4" class="flow-dot">
      <animateMotion dur="1.4s" begin="{-n * 0.46}s" repeatCount="indefinite" path={path[0]} />
    </circle>
  {/each}
{:else if data.state === 'done'}
  <circle r="2.6" class="flow-dot flow-dot-calm">
    <animateMotion dur="3.8s" repeatCount="indefinite" path={path[0]} />
  </circle>
{/if}
{#if data.mode === 'edit' && selected && data.cuttable}
  <EdgeLabel x={path[1]} y={path[2]}>
    <button type="button" class="flow-cut nodrag nopan" aria-label="Remove this link" title="Remove this link" onclick={() => data.oncut?.()}>
      <svg viewBox="0 0 12 12" width="10" height="10" aria-hidden="true"><path d="M2.5 2.5l7 7M9.5 2.5l-7 7" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" /></svg>
    </button>
  </EdgeLabel>
{:else if data.label}
  <EdgeLabel x={path[1]} y={path[2]}>
    <span class="flow-chip">{data.label}</span>
  </EdgeLabel>
{/if}
{#if data.mode === 'edit' && data.cuttable && selected}
  <!-- Drag an end of a picked link onto another card to move it, or onto empty space to take it off. -->
  <EdgeReconnectAnchor bind:reconnecting type="source" position={{ x: sourceX, y: sourceY }} class="flow-anchor" size={24} />
  <EdgeReconnectAnchor bind:reconnecting type="target" position={{ x: targetX, y: targetY }} class="flow-anchor" size={24} />
{/if}
