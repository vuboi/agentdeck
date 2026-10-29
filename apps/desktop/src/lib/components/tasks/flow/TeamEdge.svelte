<script lang="ts">
  import { BaseEdge, EdgeLabel, Position, getBezierPath, type EdgeProps } from '@xyflow/svelte';
  import type { TeamEdgeData } from './flow-graph';

  // The line two neighbouring members of a team talk over: it leaves a card's side and enters the next card's side level,
  // and bends smoothly when the cards are dragged to different heights (never a slanted straight line), an arrow at both
  // ends. While a message is waiting in a queue it glows; the chip says how many went each way.
  let { id, sourceX, sourceY, targetX, targetY, markerEnd, markerStart, data }: EdgeProps & { data: TeamEdgeData } = $props();
  const curve = $derived(getBezierPath({ sourceX, sourceY, sourcePosition: Position.Right, targetX, targetY, targetPosition: Position.Left, curvature: 0.35 }));
  const path = $derived(curve[0]);
</script>

{#if data.live}<path d={path} class="flow-edge-glow" fill="none" />{/if}
<BaseEdge {id} {path} {markerEnd} {markerStart} class="flow-edge flow-edge-{data.live ? 'active' : 'idle'} flow-edge-team" interactionWidth={14} />
{#if data.label}
  <EdgeLabel x={curve[1]} y={curve[2] - 16}><span class="flow-chip">{data.label}</span></EdgeLabel>
{/if}
