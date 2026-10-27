// A flow as the nodes and edges of the canvas. Top to bottom: the task, then one row per stage (steps of a stage run
// side by side), then the orchestrator. A step the user moved keeps its place (`x`, `y`); the others are laid out.

import { MarkerType, Position, type Edge, type Node } from '@xyflow/svelte';
import type { DelegateRun, RunNode } from '../../sessions.svelte';
import { inputsOf, stepGroups, type WorkflowStep } from '../../workflows.svelte';

export const NODE_W = 256;
export const ROW = 240;
export const START_GAP = 160;
export const COL = 296;

export type FlowState = 'idle' | 'running' | 'done' | 'failed' | 'skipped';
export type FlowDir = 'down' | 'right';
export type StepData = { step: WorkflowStep; index: number; state: FlowState; mode: 'edit' | 'run'; node?: RunNode; dir?: FlowDir; /** A member of a team: it has handles for the lines between members. */ team?: boolean };
export type TeamEdgeData = { live: boolean; label: string; span: number };
/** What a team canvas needs: the messages sent so far, from the saved queue. */
export type TeamView = { sent: { from: string; to: string; waiting: boolean }[] };
export type EndData = { kind: 'start' | 'end'; state: FlowState; mode: 'edit' | 'run'; run?: DelegateRun; dir?: FlowDir };
export type EdgeData = { state: 'idle' | 'active' | 'done' | 'skipped' | 'failed' | 'stopped' | 'talking' | 'talk'; mode: 'edit' | 'run'; cuttable: boolean; label: string; oncut?: () => void };

const size = (text: string | undefined) => (!text ? '' : text.length > 999 ? `${(text.length / 1000).toFixed(1)}k chars` : `${text.length} chars`);

/** Where every step sits when it has not been moved: rows by stage, centred on 0. */
export function autoLayout(graph: WorkflowStep[]): Map<string, { x: number; y: number }> {
  const at = new Map<string, { x: number; y: number }>();
  stepGroups(graph).forEach((group, row) => {
    group.forEach((step, col) => at.set(step.id, { x: (col - (group.length - 1) / 2) * COL - NODE_W / 2, y: (row + 1) * ROW }));
  });
  return at;
}

/** The same layout left to right (a run only): a stage is a column, the steps of a stage are stacked. */
export const STAGE_W = 330;
export const STACK_H = 200;
function autoLayoutH(graph: WorkflowStep[]): Map<string, { x: number; y: number }> {
  const at = new Map<string, { x: number; y: number }>();
  stepGroups(graph).forEach((group, col) => {
    group.forEach((step, row) => at.set(step.id, { x: (col + 1) * STAGE_W, y: (row - (group.length - 1) / 2) * STACK_H - 75 }));
  });
  return at;
}

export function buildFlow(graph: WorkflowStep[], mode: 'edit' | 'run', run: DelegateRun | undefined, cut: (from: string, to: string) => void, dir: FlowDir = 'down', team?: TeamView): { nodes: Node[]; edges: Edge[] } {
  const links = inputsOf(graph);
  // The editor always reads top to bottom; only a run can be turned to read left to right.
  const across = mode === 'run' && dir === 'right';
  const auto = across ? autoLayoutH(graph) : autoLayout(graph);
  const outPos = across ? Position.Right : Position.Bottom;
  const inPos = across ? Position.Left : Position.Top;
  // A run's places are the automatic ones: the places the user gave in the editor are not used there.
  const place = (step: WorkflowStep) => (mode === 'edit' && step.x !== undefined && step.y !== undefined ? { x: step.x, y: step.y } : (auto.get(step.id) ?? (across ? { x: STAGE_W, y: -75 } : { x: 0, y: ROW })));

  // A step run again (its plan was sent back) shows its latest run.
  const nodeOf = (id: string) => run?.nodes.findLast((node) => node.stepId === id);
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
  const centreY = (list: WorkflowStep[]) => (list.length ? list.reduce((sum, step) => sum + place(step).y, 0) / list.length : -75);
  const top = Math.min(ROW, ...graph.map((step) => place(step).y));
  const bottom = Math.max(ROW, ...graph.map((step) => place(step).y));
  const last = Math.max(STAGE_W, ...graph.map((step) => place(step).x));
  const side: FlowDir = across ? 'right' : 'down';

  const nodes: Node[] = [
    { id: 'start', type: 'end', position: across ? { x: 0, y: centreY(starters) + 45 } : { x: centre(starters) + 16, y: top - START_GAP }, sourcePosition: outPos, data: { kind: 'start', state: stateOf('start'), mode, run, dir: side } satisfies EndData, deletable: false },
    ...graph.map((step, index) => ({ id: step.id, type: 'step', position: place(step), sourcePosition: outPos, targetPosition: inPos, data: { step, index, state: stateOf(step.id), mode, node: nodeOf(step.id), dir: side, team: team !== undefined && index > 0 } satisfies StepData })),
    { id: 'end', type: 'end', position: across ? { x: last + STAGE_W, y: centreY(sinks) + 45 } : { x: centre(sinks) + 16, y: bottom + ROW }, targetPosition: inPos, data: { kind: 'end', state: stateOf('end'), mode, run, dir: side } satisfies EndData, deletable: false },
  ];

  const edges: Edge[] = [];
  const link = (from: string, to: string, cuttable: boolean) => {
    const source = stateOf(from);
    const target = stateOf(to);
    const state: EdgeData['state'] = !run
      ? 'idle'
      : target === 'skipped' || source === 'skipped'
        ? 'skipped'
        : source === 'failed' || target === 'failed'
          ? 'failed'
          : run.cancelled && (target === 'running' || target === 'idle')
            ? 'stopped'
            : target === 'running'
              ? 'active'
              : source === 'done' && target === 'done'
                ? 'done'
                : 'idle';
    const text = to === 'end' ? nodeOf(from)?.output : from === 'start' ? run?.task : nodeOf(to)?.input;
    edges.push({
      id: `${from}>${to}`,
      source: from,
      target: to,
      type: 'flow',
      deletable: mode === 'edit' && cuttable,
      markerEnd: { type: MarkerType.ArrowClosed, width: 13, height: 13, strokeWidth: 2.2, color: state === 'failed' ? 'var(--destructive, #ef4444)' : state === 'stopped' ? 'rgb(245 158 11)' : state === 'idle' || state === 'skipped' ? 'var(--flow-edge, #94a3b8)' : 'color-mix(in srgb, var(--primary, #6366f1) 70%, var(--background, #fff))' },
      data: { state, mode, cuttable, label: state === 'done' || state === 'active' ? size(text) : '', oncut: () => cut(from, to) } satisfies EdgeData,
    });
  };
  for (const step of graph) {
    const from = links[step.id] ?? [];
    for (const id of from) link(id, step.id, !(from.length === 1 && id === 'start'));
  }
  if (sinks.length === 0) link('start', 'end', false);
  for (const sink of sinks) link(sink.id, 'end', false);
  // A team: neighbouring members (all but the planner, side by side) are joined by a short straight line in the gap
  // between their cards, an arrow at both ends: they send each other messages (team.ts). Members farther apart talk
  // too, but a line across the cards in between would only clutter the canvas. In a run the line says how many
  // messages went each way and glows while one waits in a queue.
  if (team && !across) {
    const crew = graph.slice(1);
    crew.slice(0, -1).forEach((one, k) => {
      const two = crew[k + 1];
      const go = team.sent.filter((message) => message.from === one.name && message.to === two.name);
      const back = team.sent.filter((message) => message.from === two.name && message.to === one.name);
      const live = [...go, ...back].some((message) => message.waiting);
      const colour = live ? 'color-mix(in srgb, var(--primary, #6366f1) 70%, var(--background, #fff))' : 'var(--flow-edge, #94a3b8)';
      edges.push({
        id: `team:${one.id}~${two.id}`,
        source: one.id,
        sourceHandle: 'team-out',
        target: two.id,
        targetHandle: 'team-in',
        type: 'team',
        deletable: false,
        selectable: false,
        markerStart: { type: MarkerType.ArrowClosed, width: 11, height: 11, color: colour },
        markerEnd: { type: MarkerType.ArrowClosed, width: 11, height: 11, color: colour },
        data: { live, span: 0, label: run && (go.length || back.length) ? `${go.length} → · ← ${back.length}` : '' } satisfies TeamEdgeData,
      });
    });
  }
  // A step that asked an earlier one: an arrow back up, from the asker to the one it asked, curving out at the side.
  for (const item of run?.nodes ?? []) {
    for (const talk of item.talks ?? []) {
      const target = graph.find((step) => step.name.toLowerCase() === talk.to.toLowerCase());
      if (!target || !item.stepId || target.id === item.stepId) continue;
      const live = talk.answer === undefined;
      const text = live ? talk.question : `${talk.question} → ${talk.answer}`;
      edges.push({
        id: `talk:${item.stepId}>${target.id}:${talk.at}`,
        source: item.stepId,
        sourceHandle: 'talk-out',
        target: target.id,
        targetHandle: 'talk-in',
        type: 'flow',
        deletable: false,
        selectable: false,
        markerEnd: { type: MarkerType.ArrowClosed, width: 16, height: 16, color: 'rgb(168 85 247)' },
        data: { state: live ? 'talking' : 'talk', mode, cuttable: false, label: text.length > 70 ? `${text.slice(0, 69)}…` : text } satisfies EdgeData,
      });
    }
  }
  return { nodes, edges };
}
