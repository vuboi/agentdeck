import assert from 'node:assert/strict';
import test from 'node:test';
import { askSubject, bareTool, detailOf, innerCommand, toolIcon } from './tool-kind.ts';

test('every agent\'s tool names get the icon of their kind', () => {
  const expect: Record<string, string> = {
    // Claude
    Bash: 'lucideTerminal', Read: 'lucideFileText', Write: 'lucideFilePlus', Edit: 'lucideFilePen', MultiEdit: 'lucideFilePen', Grep: 'lucideSearch', Glob: 'lucideSearch',
    WebSearch: 'lucideGlobe', WebFetch: 'lucideGlobe', Task: 'lucideBot', TodoWrite: 'lucideListChecks', Skill: 'lucideSparkles', Delete: 'lucideTrash2',
    // Codex
    shell: 'lucideTerminal', exec_command: 'lucideTerminal', apply_patch: 'lucideFilePen', update_plan: 'lucideListChecks',
    // Copilot
    powershell: 'lucideTerminal', view: 'lucideFileText', create: 'lucideFilePlus', str_replace: 'lucideFilePen', report_intent: 'lucidePlug',
    // OpenCode, Gemini
    read_file: 'lucideFileText', write_file: 'lucideFilePlus', run_shell_command: 'lucideTerminal', google_web_search: 'lucideGlobe', list_directory: 'lucideSearch',
    AskUserQuestion: 'lucideMessageCircleQuestion', ask_user: 'lucideMessageCircleQuestion', request_user_input: 'lucideMessageCircleQuestion', AskUser: 'lucideMessageCircleQuestion',
    // The app's own tools and any MCP tool
    delegate_task: 'lucideAtSign', list_agents: 'lucideUsers', browser_click: 'lucideMousePointerClick', browser_dom: 'lucideMonitor', code_search: 'lucideCode',
    mcp__agentdeck__browser_click: 'lucideMousePointerClick', 'agentdeck.browser_open': 'lucideMonitor', 'officecli.officecli': 'lucidePlug', FrobnicateWidget: 'lucidePlug',
  };
  for (const [name, icon] of Object.entries(expect)) assert.equal(toolIcon(name), icon, name);
});

test('an MCP tool is named without its server', () => {
  assert.equal(bareTool('mcp__agentdeck__browser_click'), 'browser_click');
  assert.equal(bareTool('officecli.officecli'), 'officecli');
  assert.equal(bareTool('Bash'), 'Bash');
});

test('a shell step shows its command, not the shell around it', () => {
  assert.equal(innerCommand('"pwsh.exe" -Command "Get-Content \'a.txt\' -TotalCount 5"'), "Get-Content 'a.txt' -TotalCount 5");
  assert.equal(innerCommand('powershell -NoProfile -Command Get-ChildItem'), 'Get-ChildItem');
  // Only the wrapper's own quotes come off.
  assert.equal(innerCommand("echo 'hi'"), "echo 'hi'");
  assert.equal(innerCommand('git commit -m "x"'), 'git commit -m "x"');
});

test('a call\'s main argument is read from its JSON', () => {
  assert.equal(detailOf('{"command":"load_skill word","args":{"x":1}}'), 'load_skill word');
  assert.equal(detailOf('{"path":"E:/a/b.ts"}'), 'E:/a/b.ts');
  assert.equal(detailOf('{"unknown":1}'), '');
  // Still streaming in, not valid JSON yet: nothing to show.
  assert.equal(detailOf('{"command":"load_sk'), '');
  assert.equal(detailOf('plain text'), 'plain text');
});

test('an ask step is about its first question', () => {
  assert.equal(askSubject('{"questions":[{"question":"Which fix?","options":[]},{"question":"Second?"}]}'), 'Which fix?');
  assert.equal(askSubject('Which fix?'), 'Which fix?');
  assert.equal(askSubject('{"questions":[{"quest'), '');
});
