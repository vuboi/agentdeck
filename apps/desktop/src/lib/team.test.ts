import { test } from 'node:test';
import assert from 'node:assert/strict';
import { inboxOf, inboxText, parseTeamReply, planFor, teamFinished, type TeamMember } from './team.ts';

const names = ['Frontend', 'Backend'];

test('a reply sends messages and says where its author stands', () => {
  const reply = 'Built the projects API.\n\nSEND @Frontend: GET /api/projects returns [{id,name}]\n- SEND @backend: to myself, dropped\nSEND @QA: nobody, dropped\nSTATUS: waiting @Frontend';
  const parsed = parseTeamReply(reply, 'Backend', names);
  assert.deepEqual(parsed.sends, [{ to: 'Frontend', text: 'GET /api/projects returns [{id,name}]' }]);
  assert.equal(parsed.status, 'waiting');
  assert.equal(parsed.waitingOn, 'Frontend');
  assert.equal(parseTeamReply('All done.\n**STATUS: done**', 'Frontend', names).status, 'done');
  assert.equal(parseTeamReply('No status line', 'Frontend', names).status, 'working');
});

test('each member gets its part of the plan with the shared part', () => {
  const plan = '# Plan\n\n## Shared\nPOST /api/login {email,password}\n\n## Frontend\nLogin page.\n\n## Backend\nLogin endpoint.';
  assert.equal(planFor(plan, 'Frontend'), '## Shared\nPOST /api/login {email,password}\n\n## Frontend\nLogin page.');
  assert.equal(planFor('no sections at all', 'Backend'), 'no sections at all');
});

test('the run ends only when nothing is queued and nobody works', () => {
  const member = (name: string, status: TeamMember['status'], busy = false): TeamMember => ({ stepId: name, name, childId: name, status, turns: 1, busy });
  const queue = [{ id: '1', from: 'Backend', to: 'Frontend', text: 'API ready', at: 1 }];
  assert.equal(teamFinished({ members: [member('Frontend', 'done'), member('Backend', 'done')], queue }), false);
  assert.equal(inboxText(inboxOf({ queue }, 'Frontend')), 'FROM @Backend: API ready');
  const taken = [{ ...queue[0], taken: 2 }];
  assert.equal(teamFinished({ members: [member('Frontend', 'done'), member('Backend', 'waiting')], queue: taken }), true);
  assert.equal(teamFinished({ members: [member('Frontend', 'working'), member('Backend', 'done')], queue: taken }), false);
  assert.equal(teamFinished({ members: [member('Frontend', 'done', true), member('Backend', 'done')], queue: taken }), false);
});
