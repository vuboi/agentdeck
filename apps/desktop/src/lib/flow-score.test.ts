import assert from 'node:assert/strict';
import test from 'node:test';
import { scoreFlows, scoreRun, type ScoredNode, type ScoredRun } from './flow-score.ts';

const node = (over: Partial<ScoredNode> = {}): ScoredNode => ({ state: 'done', stats: { cost: 0.1, ms: 1000 }, ...over });
const run = (id: string, over: Partial<ScoredRun> = {}): ScoredRun => ({ id, preset: 'Plan → Implement → Review', startedAt: 0, endedAt: 60_000, handback: 'done', nodes: [node(), node(), node()], handbackStats: { cost: 0.05, ms: 500 }, ...over });

test('a run is right when the main agent got its answers back and every step was right', () => {
  const right = scoreRun(run('a'))!;
  assert.equal(right.ok, true);
  assert.equal(right.clean, true);
  assert.equal(right.firstTry, 3);
  assert.equal(Math.round(right.cost * 100), 35);
  assert.equal(right.ms, 60_000);
  assert.equal(scoreRun(run('b', { nodes: [node(), node({ state: 'failed' }), node()] }))!.ok, false);
  assert.equal(scoreRun(run('c', { handback: 'failed' }))!.ok, false);
});

test('a step the handoff calls blocked is wrong even when it finished', () => {
  const score = scoreRun(run('a', { nodes: [node(), node({ handoff: { status: 'blocked' } })] }))!;
  assert.equal(score.right, 1);
  assert.equal(score.ok, false);
  assert.equal(scoreRun(run('b', { nodes: [node({ handoff: { status: 'needs-plan-change' } })] }))!.ok, false);
  assert.equal(scoreRun(run('c', { nodes: [node({ handoff: { status: 'done' } })] }))!.ok, true);
});

test('a step run again is right but not on the first try', () => {
  const score = scoreRun(run('a', { nodes: [node(), node({ attempt: 2 }), node()] }))!;
  assert.equal(score.ok, true);
  assert.equal(score.clean, false);
  assert.equal(score.firstTry, 2);
  assert.equal(score.tries, 5);
});

test('a run that is not finished or was stopped has no score', () => {
  assert.equal(scoreRun(run('a', { endedAt: undefined })), null);
  assert.equal(scoreRun(run('b', { cancelled: true })), null);
});

test('flows are scored by the runs kept: rates, tries, cost and the cost of the right runs', () => {
  const runs = [
    run('1', { endedAt: 100_000 }),
    run('2', { endedAt: 200_000, nodes: [node(), node({ attempt: 1 }), node()] }),
    run('3', { endedAt: 300_000, nodes: [node(), node({ state: 'failed' }), node()] }),
    run('4', { preset: 'Bug fix', endedAt: 250_000, nodes: [node()] }),
  ];
  const flows = scoreFlows(runs);
  assert.deepEqual(flows.map((flow) => flow.name), ['Plan → Implement → Review', 'Bug fix']);
  const main = flows[0];
  assert.equal(main.runs, 3);
  assert.equal(Math.round(main.okRate * 100), 67);
  assert.equal(Math.round(main.cleanRate * 100), 33);
  assert.equal(Math.round(main.stepRate * 100), 89);
  assert.equal(main.series.map((item) => item.id).join(), '1,2,3');
  // Three runs cost 0.35 each; only two were right, so a right run costs 0.525.
  assert.equal(Math.round((main.costPerOk ?? 0) * 1000), 525);
  assert.equal(scoreFlows([run('x', { nodes: [node({ state: 'failed' })] })])[0].costPerOk, null);
});

test('hand-offs without a flow are scored together', () => {
  const flows = scoreFlows([run('1', { preset: undefined }), run('2', { preset: undefined, endedAt: 70_000 })]);
  assert.equal(flows.length, 1);
  assert.equal(flows[0].name, '@agent');
  assert.equal(flows[0].runs, 2);
});
