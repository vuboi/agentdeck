// A flow as the nodes and edges of the canvas. Top to bottom: the task, then one row per stage (steps of a stage run
// side by side), then the orchestrator. A step the user moved keeps its place (`x`, `y`); the others are laid out.

import { MarkerType, type Edge, type Node } from '@xyflow/svelte';
import type { DelegateRun, RunNode } from '../../sessions.svelte';
import { inputsOf, stepGroups, type WorkflowStep } from '../../workflows.svelte';

export const NODE_W = 256;
export const ROW = 190;
export const COL = 296;

export type FlowState = 'idle' | 'running' | 'done' | 'failed' | 'skipped';
export type StepData = { step: WorkflowStep; index: number; state: FlowState; mode: 'edit' | 'run'; node?: RunNode };
export type EndData = { kind: 'start' | 'end'; state: FlowState; mode: 'edit' | 'run'; run?: DelegateRun };
export type EdgeData = { state: 'idle' | 'active' | 'done' | 'skipped'; mode: 'edit' | 'run'; cuttable: boolean; label: string; oncut?: () => void };

const size = (text: string | undefined) => (!text ? '' : text.length > 999 ? `${(text.length / 1000).toFixed(1)}k chars` : `${text.length} chars`);

/** Where every step sits when it has not been moved: rows by stage, centred on 0. */
export function autoLayout(graph: WorkflowStep[]): Map<string, { x: number; y: number }> {
  const at = new Map<string, { x: number; y: number }>();
  stepGroups(graph).forEach((group, row) => {
    group.forEach((step, col) => at.set(step.id, { x: (col - (group.length - 1) / 2) * COL - NODE_W / 2, y: (row + 1) * ROW }));
  });
  return at;
}

export function buildFlow(graph: WorkflowStep[], mode: 'edit' | 'run', run: DelegateRun | undefined, cut: (from: string, to: string) => void): { nodes: Node[]; edges: Edge[] } {
  const links = inputsOf(graph);
  const auto = autoLayout(graph);
  const place = (step: WorkflowStep) => (step.x !== undefined && step.y !== undefined ? { x: step.x, y: step.y } : (auto.get(step.id) ?? { x: 0, y: ROW }));

  const nodeOf = (id: string) => run?.nodes.find((node) => node.stepId === id);
  const stateOf = (id: string): FlowState => {
    if (!run) return 'idle';
    if (id === 'start') return 'done';
    if (id === 'end') return run.handback === 'running' ? 'running' : run.handback === 'done' ? 'done' : run.handback === 'failed' ? 'failed' : 'idle';
    if (run.skipped?.includes(id)) return 'skipped';
    return nodeOf(id)?.state ?? 'idle';
  };

  const sinks = graph.filter((step) => !graph.some((other) => links[other.id]?.includes(step.id)));
  const starters = graph.filter((step) => links[step.id]?.includes('start'));
  const centre = (list: WorkflowStep[]) => (list.length ? list.reduce((sum, step) => sum + place(step).x, 0) / list.length : -NODE_W / 2);
  const top = Math.min(ROW, ...graph.map((step) => place(step).y));
  const bottom = Math.max(ROW, ...graph.map((step) => place(step).y));

  const nodes: Node[] = [
    { id: 'start', type: 'end', position: { x: centre(starters) + 16, y: top - ROW }, data: { kind: 'start', state: stateOf('start'), mode, run } satisfies EndData, deletable: false },
    ...graph.map((step, index) => ({ id: step.id, type: 'step', position: place(step), data: { step, index, state: stateOf(step.id), mode, node: nodeOf(step.id) } satisfies StepData })),
    { id: 'end', type: 'end', position: { x: centre(sinks) + 16, y: bottom + ROW }, data: { kind: 'end', state: stateOf('end'), mode, run } satisfies EndData, deletable: false },
  ];

  const edges: Edge[] = [];
  const link = (from: string, to: string, cuttable: boolean) => {
    const source = stateOf(from);
    const target = stateOf(to);
    const state: EdgeData['state'] = !run
      ? 'idle'
      : target === 'skipped' || source === 'skipped'
        ? 'skipped'
        : target === 'running'
          ? 'active'
          : source === 'done' && (target === 'done' || target === 'failed')
            ? 'done'
            : 'idle';
    const text = to === 'end' ? nodeOf(from)?.output : from === 'start' ? run?.task : nodeOf(to)?.input;
    edges.push({
      id: `${from}>${to}`,
      source: from,
      target: to,
      type: 'flow',
      deletable: mode === 'edit' && cuttable,
      markerEnd: { type: MarkerType.ArrowClosed, width: 18, height: 18, color: state === 'idle' || state === 'skipped' ? 'var(--flow-edge, #94a3b8)' : 'var(--primary, #6366f1)' },
      data: { state, mode, cuttable, label: state === 'done' || state === 'active' ? size(text) : '', oncut: () => cut(from, to) } satisfies EdgeData,
    });
  };
  for (const step of graph) {
    const from = links[step.id] ?? [];
    for (const id of from) link(id, step.id, !(from.length === 1 && id === 'start'));
  }
  if (sinks.length === 0) link('start', 'end', false);
  for (const sink of sinks) link(sink.id, 'end', false);
  return { nodes, edges };
}
