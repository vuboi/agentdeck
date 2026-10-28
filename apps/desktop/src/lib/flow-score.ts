// How well each flow did over the runs kept in the history: how often it was right, how many tries its steps needed,
// what a run cost and how long it took. Free of stores so plain node can test it.
//
// A step is right when it finished and its handoff does not say it is blocked or needs the plan changed. It is right the
// first time when it also never had to be run again (`attempt` counts the re-runs of a step). A run is right when the main
// agent got its answers back and every step was right.

/** The part of a step of a run that the score reads. */
export type ScoredNode = {
  state: 'running' | 'done' | 'failed';
  /** How many times the step was run again (0 or missing: the first run was the only one). */
  attempt?: number;
  handoff?: { status: string } | null;
  stats?: { cost: number | null; ms: number };
};
/** The part of a run that the score reads. */
export type ScoredRun = {
  id: string;
  preset?: string;
  startedAt: number;
  endedAt?: number;
  cancelled?: boolean;
  handback: 'waiting' | 'running' | 'done' | 'failed';
  nodes: ScoredNode[];
  handbackStats?: { cost: number | null; ms: number };
};

export const isRight = (node: ScoredNode) => node.state === 'done' && node.handoff?.status !== 'blocked' && node.handoff?.status !== 'needs-plan-change';
export const triesOf = (node: ScoredNode) => (node.attempt ?? 0) + 1;

export type RunScore = {
  id: string;
  /** When the run ended (ms), for the order and the chart. */
  at: number;
  steps: number;
  /** Steps that were right, and of those the ones right on the first try. */
  right: number;
  firstTry: number;
  /** Tries over all steps (a step run twice counts 2). */
  tries: number;
  ok: boolean;
  /** Right and no step needed a second try. */
  clean: boolean;
  cost: number;
  ms: number;
};

/** The score of one finished run, or null for one that is not finished or was stopped by the user. */
export function scoreRun(run: ScoredRun): RunScore | null {
  if (!run.endedAt || run.cancelled) return null;
  const right = run.nodes.filter(isRight);
  const firstTry = right.filter((node) => triesOf(node) === 1);
  const ok = run.handback === 'done' && right.length === run.nodes.length;
  const cost = [...run.nodes.map((node) => node.stats?.cost ?? 0), run.handbackStats?.cost ?? 0].reduce((sum, value) => sum + value, 0);
  return {
    id: run.id,
    at: run.endedAt,
    steps: run.nodes.length,
    right: right.length,
    firstTry: firstTry.length,
    tries: run.nodes.reduce((sum, node) => sum + triesOf(node), 0),
    ok,
    clean: ok && firstTry.length === run.nodes.length,
    cost,
    ms: run.endedAt - run.startedAt,
  };
}

export type FlowScore = {
  name: string;
  runs: number;
  /** Share of runs that were right (0 to 1), and right on the first try. */
  okRate: number;
  cleanRate: number;
  /** Share of steps right, and mean tries per step. */
  stepRate: number;
  avgTries: number;
  avgCost: number;
  avgMs: number;
  /** What the right runs cost on average: the money spent on wrong runs counts against the flow. */
  costPerOk: number | null;
  /** The runs oldest first, for the chart. */
  series: RunScore[];
  last: number;
};

const sum = (values: number[]) => values.reduce((total, value) => total + value, 0);

/** One score per flow (`@agent` hand-offs together), the most recently run first. */
export function scoreFlows(runs: ScoredRun[]): FlowScore[] {
  const groups = new Map<string, RunScore[]>();
  for (const run of runs) {
    const score = scoreRun(run);
    if (!score) continue;
    const key = run.preset ?? '@agent';
    groups.set(key, [...(groups.get(key) ?? []), score]);
  }
  return [...groups.entries()]
    .map(([name, list]): FlowScore => {
      const series = [...list].sort((a, b) => a.at - b.at);
      const steps = sum(series.map((run) => run.steps));
      const ok = series.filter((run) => run.ok).length;
      const cost = sum(series.map((run) => run.cost));
      return {
        name,
        runs: series.length,
        okRate: ok / series.length,
        cleanRate: series.filter((run) => run.clean).length / series.length,
        stepRate: steps ? sum(series.map((run) => run.right)) / steps : 0,
        avgTries: steps ? sum(series.map((run) => run.tries)) / steps : 0,
        avgCost: cost / series.length,
        avgMs: sum(series.map((run) => run.ms)) / series.length,
        costPerOk: ok ? cost / ok : null,
        series,
        last: series.at(-1)?.at ?? 0,
      };
    })
    .sort((a, b) => b.last - a.last);
}
