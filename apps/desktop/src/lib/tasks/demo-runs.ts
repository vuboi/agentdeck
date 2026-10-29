// Made-up runs for the browser mock (never the desktop app), so the flow history can be looked at without running flows:
// three flows over the last two weeks, some right, some with a step redone, some wrong. The numbers are seeded, so the same
// page always shows the same history.
import type { DelegateRun, RunNode } from '../chat/sessions.svelte';

type Spec = { name: string; steps: { label: string; agent: RunNode['agent']; model: string }[]; runs: number; baseCost: number; baseMin: number; flaky: number };

const FLOWS: Spec[] = [
  {
    name: 'Plan → Implement → Review',
    steps: [
      { label: 'Plan', agent: 'claude', model: 'claude-opus-5-5' },
      { label: 'Implement', agent: 'codex', model: 'gpt-6.1-sol' },
      { label: 'Review', agent: 'claude', model: 'claude-sonnet-5-5' },
    ],
    runs: 14,
    baseCost: 0.3,
    baseMin: 9,
    flaky: 0.2,
  },
  {
    name: 'Bug fix',
    steps: [
      { label: 'Reproduce', agent: 'claude', model: 'claude-sonnet-5-5' },
      { label: 'Fix', agent: 'codex', model: 'gpt-6.1-luna' },
      { label: 'Review', agent: 'claude', model: 'claude-haiku-5-5' },
    ],
    runs: 9,
    baseCost: 0.1,
    baseMin: 4,
    flaky: 0.13,
  },
  {
    name: 'Docs update',
    steps: [
      { label: 'Write', agent: 'claude', model: 'claude-haiku-5-5' },
      { label: 'Check', agent: 'codex', model: 'gpt-6.1-luna' },
    ],
    runs: 6,
    baseCost: 0.03,
    baseMin: 2,
    flaky: 0.06,
  },
];

export function demoRuns(now = Date.now()): DelegateRun[] {
  let seed = 7;
  const random = () => {
    seed = (seed * 1664525 + 1013904223) % 4294967296;
    return seed / 4294967296;
  };
  const out: DelegateRun[] = [];
  FLOWS.forEach((flow, flowIndex) => {
    for (let index = 0; index < flow.runs; index++) {
      // Oldest first over about two weeks; the older runs go wrong more often, as a flow gets tuned.
      const age = (flow.runs - index) / flow.runs;
      const startedAt = now - age * 13.5 * 86_400_000 - flowIndex * 3_600_000 * 5;
      const ms = (flow.baseMin * (0.7 + random() * 0.9) + (age > 0.6 ? 3 : 0)) * 60_000;
      const wrongChance = flow.flaky * (0.4 + age);
      const nodes: RunNode[] = flow.steps.map((step, stepIndex) => {
        const roll = random();
        const failed = roll < wrongChance * 0.5;
        const redone = !failed && roll < wrongChance + 0.1;
        const cost = flow.baseCost * (0.25 + random() * 0.5) * (stepIndex === 1 ? 1.4 : 0.8) * (redone ? 1.8 : 1);
        const stepMs = (ms / flow.steps.length) * (0.7 + random() * 0.6);
        const blocked = !failed && random() < wrongChance * 0.25;
        return {
          childId: `demo-${flowIndex}-${index}-${stepIndex}`,
          agent: step.agent,
          model: step.model,
          label: step.label,
          startedAt: startedAt + stepIndex * (ms / flow.steps.length),
          endedAt: startedAt + (stepIndex + 1) * (ms / flow.steps.length),
          state: failed ? 'failed' : 'done',
          attempt: redone ? 1 + Math.floor(random() * 2) : undefined,
          handoff: failed ? null : { status: blocked ? 'blocked' : 'done', result: step.label, files: [], facts: [], verify: [], open: [] },
          stats: { ms: stepMs, cost, tools: 4 + Math.floor(random() * 9), last: '' },
        };
      });
      const right = nodes.every((node) => node.state === 'done' && node.handoff?.status !== 'blocked');
      out.push({
        id: `demo-run-${flowIndex}-${index}`,
        main: 's1',
        task: `${flow.name}: demo run ${index + 1}`,
        preset: flow.name,
        startedAt,
        nodes,
        handback: right || random() > 0.5 ? 'done' : 'failed',
        handbackStats: { ms: 20_000, cost: flow.baseCost * 0.12, tools: 0, last: '' },
        endedAt: startedAt + ms,
      });
    }
  });
  return out;
}
