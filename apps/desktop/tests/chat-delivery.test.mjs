import assert from 'node:assert/strict';
import test from 'node:test';
import { readFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import vm from 'node:vm';
import { enqueue, enqueueNext } from '../src/lib/chat/runs/queue.ts';
import { applyAgentEvent } from '../src/lib/chat/agent-reply.ts';

const ts = createRequire(import.meta.url)('typescript');
const providers = ['claude', 'codex', 'antigravity', 'opencode', 'copilot', 'custom'];
const flush = () => new Promise((resolve) => setImmediate(resolve));

/** Executes a real coordinator module with isolated native I/O; unconfigured imports fail on use. */
async function load(name, bindings) {
  const source = await readFile(new URL(`../src/lib/chat/session/${name}.svelte.ts`, import.meta.url), 'utf8');
  const ast = ts.createSourceFile(name, source, ts.ScriptTarget.Latest, true);
  const imports = new Map();
  for (const node of ast.statements) {
    if (!ts.isImportDeclaration(node) || node.importClause?.isTypeOnly) continue;
    const names = node.importClause?.namedBindings;
    if (!names || !ts.isNamedImports(names)) continue;
    imports.set(node.moduleSpecifier.text, [...new Set([...(imports.get(node.moduleSpecifier.text) ?? []), ...names.elements.filter((item) => !item.isTypeOnly).map((item) => (item.propertyName ?? item.name).text)])]);
  }
  const effect = Object.assign(() => {}, { root: () => {} });
  const context = vm.createContext({ $state: (value) => value, $effect: effect, Date, Map, Set, crypto, performance, console, setTimeout: () => 0, clearTimeout() {} });
  const code = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ESNext, module: ts.ModuleKind.ESNext } }).outputText;
  const module = new vm.SourceTextModule(code, { context });
  await module.link((path) => {
    const names = imports.get(path);
    return new vm.SyntheticModule(names, function () {
      for (const name of names) this.setExport(name, name in bindings ? bindings[name] : () => { throw new Error(`Unconfigured ${path}:${name}`); });
    }, { context });
  });
  await module.evaluate();
  return module.namespace;
}

/** Shared fixtures contain no credentials and no real endpoint. */
function fixture(agent) {
  let id = 0;
  const session = { id: 'chat', agent, model: 'model', status: 'idle', projectId: 'project' };
  const messages = [];
  const replies = {};
  const queued = {};
  const queuePaused = {};
  const starts = [];
  const stops = [];
  const bindings = {
    workspace: { activeId: 'chat', sessions: [session], projects: [] },
    activeSession: () => session, messagesOf: () => messages, replies, queued, queuePaused,
    getPref: async () => null, setPref() {}, persistLocal() {}, logFlow() {},
    clock: () => '', nextId: () => `id-${++id}`, inTauri: () => true,
    cancelAutoSwitch() {}, beginSessionWork() {}, credentialBusy: () => false,
    switchingAgent: () => false, canListenToSession: () => false,
    pendingInput: () => null, needsUserInput: () => false,
    hasTurnWork: () => Boolean(replies.chat), isRunning: () => Boolean(replies.chat),
    continueCurrentRun: () => 'group', enqueue, enqueueNext, sendNextQueued() {},
    resumes: {}, resumeAutoChoice: {}, resumeSince: {}, localMessages: {},
    startTurn: (...args) => { starts.push(args); replies.chat = 'reply'; session.status = 'working'; },
    stopTurn: (...args) => { stops.push(args); delete replies.chat; session.status = 'idle'; },
    presetById: () => undefined, presetOf: () => undefined, withElements: (value) => value,
    recoveries: {}, mentionContext: () => '', parseCustomModel: () => ({ providerId: 'fixture', model: 'model' }),
  };
  return { session, messages, replies, queued, queuePaused, starts, stops, bindings };
}

for (const agent of providers) {
  test(`${agent}: Send now prioritizes exactly once without cancelling existing work`, async () => {
    const f = fixture(agent);
    f.replies.chat = 'old';
    const first = { messageId: 'one', text: 'first', images: [], elements: [] };
    const second = { messageId: 'two', text: 'second', images: [], elements: [] };
    f.queued.chat = [first, second];
    f.queuePaused.chat = 'manual';
    const api = await load('sending', f.bindings);
    api.steerQueued('chat', first);
    api.steerQueued('chat', first);
    assert.equal(f.starts.length, 0);
    assert.equal(f.stops.length, 0);
    assert.equal(f.messages.length, 0);
    assert.deepEqual(Array.from(f.queued.chat, (item) => item.messageId), ['one', 'two']);
    assert.equal(f.queued.chat[0].priority, true);
    assert.equal(f.queuePaused.chat, undefined);
  });

  test(`${agent}: Send now cannot bypass a pending permission or question`, async () => {
    const f = fixture(agent);
    f.bindings.needsUserInput = () => true;
    const item = { text: 'first', images: [], elements: [] };
    f.queued.chat = [item];
    const api = await load('sending', f.bindings);
    api.steerQueued('chat', item);
    assert.equal(f.starts.length, 0);
    assert.deepEqual(f.queued.chat, [item]);
  });

  test(`${agent}: messages waking after an account switch retain FIFO order`, async () => {
    const f = fixture(agent);
    let release;
    const ready = new Promise((resolve) => { release = resolve; });
    let busy = true;
    f.bindings.credentialBusy = () => busy;
    f.bindings.waitForCredential = () => ready;
    const api = await load('sending', f.bindings);
    api.send('chat', 'first', [], []);
    api.send('chat', 'second', [], []);
    api.send('chat', 'third', [], []);
    busy = false;
    release();
    await flush();
    assert.equal(f.starts.length, 1);
    assert.deepEqual(Array.from(f.queued.chat, (item) => item.text), ['second', 'third']);
  });

  test(`${agent}: stopping during preparation prevents a later provider launch`, async () => {
    const f = fixture(agent);
    let release;
    const preparing = new Promise((resolve) => { release = resolve; });
    const calls = [];
    Object.assign(f.bindings, {
      modelFor: () => 'model', rememberLastChat() {}, pendingNew: new Set(),
      chat: { messages: f.messages }, currentCredentialId: () => 'fixture', missedTurns: () => '',
      sessionMark: () => undefined,
      turnOptions: { effort: 'medium' }, accountEpoch: {}, liveModel: () => true,
      liveReplies: {}, runEpochs: {}, turns: {}, advanceRun() {}, sessionCwd: () => null,
      prewarmAgent: async () => {}, isWorkerChat: () => true,
      lastPrompt: {}, lastEventAt: new Map(), lastEventKind: new Map(),
      saveAttachment: () => preparing, within: (promise) => promise,
      sendAgentTurn: async (...args) => calls.push(args), sendCustomTurn: async (...args) => calls.push(args),
      handleAgentEvent: (_id, event) => { throw new Error(`Unexpected error: ${event.message}`); },
    });
    const api = await load('run', f.bindings);
    f.messages.push({ id: 'ask', role: 'user', blocks: [] });
    api.startTurn(f.session, 'hello', [{ mime: 'image/png', src: 'data:image/png;base64,AA==' }]);
    assert.equal(api.runReplies.size, 1);
    // The real terminal-event handler deletes run ownership on Stop.
    api.runReplies.clear();
    release('fixture.png');
    await flush();
    assert.equal(calls.length, 0);
  });
}

test('Stop pauses the queue before returning from a foreground listener', async () => {
  const f = fixture('claude');
  const runReplies = new Map([['listener', { sessionId: 'chat', listener: true }]]);
  f.queued.chat = [{ text: 'later' }];
  Object.assign(f.bindings, {
    runReplies, stopAgentTurn: async () => {}, handleAgentEvent() {},
  });
  const api = await load('turns', f.bindings);
  // This module owns queue state, so arrange the queue through its public export.
  api.queued.chat = f.queued.chat;
  api.stopTurn('chat');
  assert.equal(api.queuePaused.chat, 'interrupted');
});

test('a background report keeps the originating request even when a newer prompt exists', async () => {
  const f = fixture('codex');
  const sent = [];
  Object.assign(f.bindings, { inTauri: () => false, hasTurnWork: () => false, send: (...args) => sent.push(args), handbackPrompt: (batch) => batch.task, NO_HANDOUT: {} });
  const api = await load('delegate-task', f.bindings);
  f.messages.push({ id: 'new', role: 'user', blocks: [{ kind: 'response', text: 'Unrelated new request' }] });
  api.delegateTasks.first = { id: 'first', state: 'done' };
  api.delegateTasks.second = { id: 'second', state: 'done' };
  api.backlog.set('chat', [
    { taskId: 'first', label: 'worker one', text: 'result', sourceMessageId: 'original', sourceTask: 'Original request' },
    { taskId: 'second', label: 'worker two', text: 'result two', sourceMessageId: 'other', sourceTask: 'Different request' },
  ]);
  api.flushReports('chat', 0);
  assert.equal(sent[0][5].prompt, 'Original request');
  assert.equal(sent[0][5].report.triggeredByMessageId, 'original');
  assert.deepEqual(Array.from(sent[0][5].report.taskIds), ['first']);
  assert.equal(api.backlog.get('chat').length, 1);
  assert.equal(api.backlog.get('chat')[0].sourceMessageId, 'other');
});

test('pending input and a busy lane keep background reports without polling or dropping them', async () => {
  const f = fixture('codex');
  Object.assign(f.bindings, { inTauri: () => false, hasTurnWork: () => true, send: () => assert.fail('must wait') });
  const api = await load('delegate-task', f.bindings);
  api.backlog.set('chat', [{ taskId: 'first', label: 'worker', text: 'result', sourceTask: 'Original request' }]);
  api.flushReports('chat', 3601);
  assert.equal(api.backlog.get('chat').length, 1);
});

test('finishing a foreground lane settles only its own tools and request', async () => {
  const api = await load('../runs/activity', { inTauri: () => false });
  const call = { kind: 'tool_call', id: 'same-id', name: 'Bash', input: 'work', background: false, change: null };
  for (const turn of ['background-waiter', 'foreground']) {
    api.logRequest('chat', 'Title', '', 'Claude', 'model', 'Details', 'claude', turn);
    api.trackActivity('chat', 'Title', '', call, turn);
  }
  api.trackActivity('chat', 'Title', '', { kind: 'done', text: '', session_id: null, cost_usd: null }, 'foreground');
  const rows = api.activity;
  assert.equal(rows[0].status, 'running');
  assert.equal(rows[1].status, 'ok');
  assert.equal(api.audit.find((item) => item.name === 'Request' && item.turn === 'background-waiter').status, 'running');
});

test('a provider result preserves background work until the later task completion', async () => {
  const api = await load('../runs/activity', { inTauri: () => false });
  api.trackActivity('chat', '', '', { kind: 'tool_call', id: 'bg', name: 'Bash', input: 'background work', background: true, change: null }, 'primary');
  api.trackActivity('chat', '', '', { kind: 'tool_result', id: 'bg', output: 'Started', is_error: false }, 'primary');
  api.trackActivity('chat', '', '', { kind: 'done', text: '', session_id: null, cost_usd: null }, 'primary');
  assert.equal(api.activity[0].status, 'background');
  api.trackActivity('chat', '', '', { kind: 'task', id: 'task', tool_use_id: 'bg', status: 'done', label: null, summary: 'Finished', tools: null }, 'auto-turn');
  assert.equal(api.activity[0].status, 'ok');
  assert.equal(api.activity[0].output, 'Finished');
});

test('rendering a finished offscreen reply does not block delivery', async () => {
  const f = fixture('codex');
  Object.assign(f.bindings, { sessionLanes: {}, renderWaiters: { chat: { messageId: 'old' } } });
  const api = await load('turns', f.bindings);
  assert.equal(api.hasTurnWork('chat'), false);
});

test('a live listener and its prompt are reattached when a transcript is loaded again', async () => {
  const f = fixture('codex');
  const api = await load('run', f.bindings);
  const ask = { id: 'listener-ask', role: 'user', blocks: [] };
  const reply = { id: 'listener-reply', role: 'assistant', blocks: [] };
  api.runReplies.set('listener', { sessionId: 'chat', listener: true, ask, reply });
  const restored = api.withLive('chat', []);
  assert.deepEqual(Array.from(restored, (item) => item.id), ['listener-ask', 'listener-reply']);
  assert.equal(api.withLive('chat', restored).length, 2);
});

test('an automatic background report arriving during a newer prompt owns its own reply', async () => {
  const f = fixture('codex');
  const primary = { id: 'foreground', role: 'assistant', live: true, replyTo: 'new-request', blocks: [] };
  const runReplies = new Map();
  const liveReplies = { chat: primary };
  Object.assign(f.bindings, {
    inTauri: () => false, runReplies, liveReplies, turns: { chat: 'foreground-turn' },
    activity: [{ sessionId: 'chat', background: true, sourceMessageId: 'original-request', sourceTitle: 'Original work' }],
    updateLane() {}, advanceRun() {}, runEpochs: {}, continuationGroups: {},
    trackActivity() {}, markStep() {}, hangTraceOn: () => false, guide: { tasks: {} }, applyAgentEvent, addUsage() {}, liveAsks: {},
  });
  const api = await load('events', f.bindings);
  api.handleAgentEvent('chat', { kind: 'auto_turn', reason: 'background work finished' }, 'auto-report');
  api.handleAgentEvent('chat', { kind: 'delta', text: 'Old task result' }, 'auto-report');
  api.handleAgentEvent('chat', { kind: 'usage', input_tokens: 1, output_tokens: 1, cache_read_tokens: 0, cache_write_tokens: 0, new_call: false }, 'auto-report');
  assert.equal(liveReplies.chat, primary);
  assert.equal(primary.blocks.length, 0);
  const report = runReplies.get('auto-report');
  assert.equal(report.listener, true);
  assert.equal(report.reply.triggeredByMessageId, 'original-request');
  assert.equal(report.reply.blocks[0].text, 'Old task result');
});
