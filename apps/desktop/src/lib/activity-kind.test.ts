import assert from 'node:assert/strict';
import test from 'node:test';
import { detailOf, kindOf } from './activity-kind.ts';

test('each tool reads as a kind of step', () => {
  const expect: Record<string, string> = {
    Thinking: 'thinking', Write: 'create', Edit: 'edit', MultiEdit: 'edit', apply_patch: 'edit', Delete: 'delete', Read: 'read',
    Bash: 'command', 'Run project': 'command', Grep: 'search', code_search: 'search', WebSearch: 'web', WebFetch: 'fetch', Skill: 'skill',
    Task: 'agent', delegate_task: 'agent', TodoWrite: 'plan', 'Chat error': 'error', mcp__context7__query_docs: 'mcp', 'agentdeck.browser_open': 'mcp',
    browser_screenshot: 'mcp', 'codex · item.started': 'raw',
  };
  for (const [name, id] of Object.entries(expect)) assert.equal(kindOf(name).id, id, name);
});

test('an unknown tool keeps its own name', () => {
  assert.equal(kindOf('FrobnicateWidget').label, 'FrobnicateWidget');
});

test('an MCP step names its server and tool', () => {
  assert.equal(detailOf('mcp__context7__query_docs'), 'context7 · query_docs');
  assert.equal(detailOf('agentdeck.browser_open'), 'agentdeck · browser_open');
  assert.equal(detailOf('Bash'), '');
});

test('the tools of other agents read as the same kinds of step', () => {
  const expect: Record<string, string> = {
    powershell: 'command', run_shell_command: 'command', create: 'create', write_file: 'create', str_replace: 'edit', replace: 'edit',
    view: 'read', read_file: 'read', list_directory: 'search', google_web_search: 'web', fetch_url: 'fetch',
  };
  for (const [name, id] of Object.entries(expect)) assert.equal(kindOf(name).id, id, name);
});
