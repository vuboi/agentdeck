import assert from 'node:assert/strict';
import test from 'node:test';
import { readFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import vm from 'node:vm';

const ts = createRequire(import.meta.url)('typescript');
// Run with `pnpm --filter desktop test:account-switch`.
// Fixtures use .invalid endpoints and fabricated keys. The VM cannot access real Tauri or fetch.
// These tests cover selection, native command order, rollback and cancellation; UI rendering,
// IndexedDB transactions and a provider's actual network/auth behavior require separate coverage.
const copy = (value) => structuredClone(value);
const account = (id, pct = 10) => ({ id, email: `${id}@example.invalid`, alias: null, plan: null, active: id === 'a', usage: [{ label: '5h', pct, resets_at: null }], observed_at: Date.now() / 1000, expired: false, status: null });
const attempt = (model = 'model', failedId = 'a') => ({ signal: new AbortController().signal, tried: new Set([failedId]), model, failedId });

/** Runs the actual TypeScript modules in isolation; only native I/O, runes and timers are stubbed.
 * No Svelte rendering is asserted here. Unknown native calls fail instead of reaching the network.
 */
async function harness({ provider = 'codex', rows = [account('a', 100), account('b', 20), account('c', 70)], onCall, quota, prefs = {} } = {}) {
  const calls = [];
  const waits = [];
  const epochs = [];
  let active = 'a';
  const state = Object.assign((value) => value, { snapshot: copy });
  const effect = Object.assign(() => {}, { root: () => {} });
  const context = vm.createContext({
    $state: state, $effect: effect, AbortController, DOMException, Date, Math, Error, console,
    setTimeout(fn, ms) {
      const timer = { cancelled: false };
      if (ms <= 10_000) { waits.push(ms); queueMicrotask(() => { if (!timer.cancelled) fn(); }); }
      return timer;
    },
    clearTimeout(timer) { timer.cancelled = true; },
    fetch() { throw new Error('Network forbidden in rotation tests'); },
  });
  const list = () => ({ manager: true, accounts: copy(rows).map((row) => ({ ...row, active: row.id === active })), auto_switch: null, error: null });
  const invoke = async (command, args = {}) => {
    calls.push({ command, args: copy(args) });
    const custom = await onCall?.(command, args, { active, setActive: (id) => { active = id; } });
    if (custom !== undefined) return custom;
    if (command === 'account_switch' || command === 'antigravity_account_switch') { active = args.id; return null; }
    if (command === 'accounts_list') return list();
    if (command === 'antigravity_accounts_list') return copy(rows).map((row) => ({ ...row, active: row.id === active }));
    if (command === 'antigravity_quota') return quota?.(active) ?? { observed_at: Date.now() / 1000, groups: [{ id: 'gemini', windows: [{ label: '5h', pct: 10, resets_at: null }] }] };
    if (command === 'provider_quota') return null;
    if (command === 'provider_probe') return { ok: true, models: ['model'], ms: 1, error: null };
    if (command === 'keep_awake_set' || command === 'connectors_detect') return null;
    throw new Error(`Unmocked native call: ${command}`);
  };
  const mocks = {
    '@tauri-apps/api/core': { invoke },
    '../app/platform/history': { inTauri: () => true },
    '../app/platform/prefs': { getPref: async (key) => copy(prefs[key] ?? null), setPref: async (key, value) => { prefs[key] = copy(value); } },
    '../app/platform/secrets': { loadSecret: async () => null, saveSecret: async () => true },
    '../app/platform/toast': { toast: { success() {}, error() {} } },
    '../chat/runs/account-epoch.svelte': { accountSwitched: (id) => epochs.push(id) },
    '../app/platform/system-notifications': { requestSystemNotificationPermission: async () => false, sendSystemNotification: async () => {} },
    '../app/platform/tauri-event': { listen: async () => () => {} },
    './models.svelte': { loadModels: async () => {}, reloadModels: async () => {} },
  };
  const cache = new Map();
  async function load(specifier) {
    if (cache.has(specifier)) return cache.get(specifier);
    const stub = mocks[specifier];
    let module;
    if (stub) {
      module = new vm.SyntheticModule(Object.keys(stub), function () { for (const [key, value] of Object.entries(stub)) this.setExport(key, value); }, { context });
    } else {
      assert.ok(['./connectors.svelte', './credential-switch'].includes(specifier), `Unexpected module: ${specifier}`);
      const source = await readFile(new URL(`${specifier}.ts`, import.meta.url), 'utf8');
      const code = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ESNext, module: ts.ModuleKind.ESNext } }).outputText;
      module = new vm.SourceTextModule(code, { context, identifier: specifier, importModuleDynamically: async (name) => {
        const found = await load(name);
        if (found.status === 'linked') await found.evaluate();
        return found;
      } });
    }
    cache.set(specifier, module);
    await module.link((name) => load(name));
    return module;
  }
  const module = await load('./connectors.svelte');
  await module.evaluate();
  await new Promise((resolve) => setImmediate(resolve));
  const api = module.namespace;
  api.connectors.autoSwitch[provider] = true;
  if (provider === 'agy') api.connectors.accounts.agy = list().accounts;
  else api.connectors.accounts[provider] = list();
  const native = () => calls.filter((call) => call.command === 'account_switch' || call.command === 'antigravity_account_switch').map((call) => call.args.id);
  return { api, calls, waits, epochs, prefs, native, active: () => active };
}

for (const provider of ['claude', 'codex', 'copilot']) {
  test(`${provider}: chooses the greatest available quota and waits 5–10 seconds`, async () => {
    const h = await harness({ provider });
    const result = await h.api.autoSwitchConnectorAccount(provider, attempt());
    assert.equal(result.email, 'b@example.invalid');
    assert.equal(h.active(), 'b');
    assert.deepEqual(h.native(), ['b']);
    assert.equal(h.waits.length, 1);
    assert.ok(h.waits.every((ms) => ms >= 5000 && ms <= 10000));
    assert.deepEqual(h.epochs, [provider]);
  });
  test(`${provider}: failed validation rolls back before trying the next profile`, async () => {
    const h = await harness({ provider, onCall(command, _args, state) {
      if (command === 'accounts_list' && state.active === 'b') throw new Error('401 signed out');
    } });
    assert.equal((await h.api.autoSwitchConnectorAccount(provider, attempt())).email, 'c@example.invalid');
    assert.deepEqual(h.native(), ['b', 'a', 'c']);
    assert.equal(h.active(), 'c');
  });
  test(`${provider}: all candidates fail and original account is restored`, async () => {
    const h = await harness({ provider, onCall(command, _args, state) {
      if (command === 'accounts_list' && state.active !== 'a') throw new Error('Quota endpoint failed');
    } });
    assert.equal(await h.api.autoSwitchConnectorAccount(provider, attempt()), null);
    assert.equal(h.active(), 'a');
    assert.deepEqual(h.native().slice(0, 4), ['b', 'a', 'c', 'a']);
  });
  test(`${provider}: exhausted and signed-out profiles are never activated`, async () => {
    const h = await harness({ provider, rows: [account('a', 100), account('b', 100), { ...account('c'), expired: true }] });
    assert.equal(await h.api.autoSwitchConnectorAccount(provider, attempt()), null);
    assert.ok(h.native().every((id) => id === 'a'));
  });
}

test('connector recovery keeps failed identities across continuation errors', async () => {
  const h = await harness();
  const recovery = attempt();
  await h.api.autoSwitchConnectorAccount('codex', recovery);
  recovery.failedId = 'b'; recovery.tried.add('b');
  assert.equal((await h.api.autoSwitchConnectorAccount('codex', recovery)).email, 'c@example.invalid');
  recovery.failedId = 'c'; recovery.tried.add('c');
  assert.equal(await h.api.autoSwitchConnectorAccount('codex', recovery), null);
  assert.equal(h.active(), 'a');
  assert.equal(h.native().filter((id) => id === 'b').length, 1);
});

test('two chats share the verified account instead of rotating twice', async () => {
  const h = await harness();
  const results = await Promise.all([h.api.autoSwitchConnectorAccount('codex', attempt()), h.api.autoSwitchConnectorAccount('codex', attempt())]);
  assert.ok(results.every((result) => result.email === 'b@example.invalid'));
  assert.deepEqual(h.native(), ['b']);
});

test('cancellation during account verification rolls back without publishing the candidate', async () => {
  const controller = new AbortController();
  const h = await harness({ onCall(command, _args, state) {
    if (command === 'accounts_list' && state.active === 'b') controller.abort();
  } });
  await assert.rejects(h.api.autoSwitchConnectorAccount('codex', { ...attempt(), signal: controller.signal }), { name: 'AbortError' });
  assert.equal(h.active(), 'a');
  assert.deepEqual(h.epochs, []);
});

test('rollback failure stops recovery before activating another account', async () => {
  const h = await harness({ onCall(command, args, state) {
    if (command === 'accounts_list' && state.active === 'b') throw new Error('verification failed');
    if (command === 'account_switch' && args.id === 'a') throw new Error('restore failed');
  } });
  await assert.rejects(h.api.autoSwitchConnectorAccount('codex', attempt()));
  assert.ok(!h.native().includes('c'));
});

test('Antigravity rejects quota exhausted for the requested model family', async () => {
  const h = await harness({ provider: 'agy', quota(active) { return { groups: [{ id: 'gemini', windows: [{ label: 'daily', pct: active === 'b' ? 100 : 20, resets_at: null }] }, { id: '3p', windows: [{ label: 'daily', pct: 0, resets_at: null }] }] }; } });
  assert.equal((await h.api.autoSwitchConnectorAccount('agy', attempt('gemini-pro'))).email, 'c@example.invalid');
  assert.deepEqual(h.native(), ['b', 'a', 'c']);
});

test('Antigravity requires quota for the actual model family', async () => {
  const h = await harness({ provider: 'agy' });
  assert.equal(await h.api.autoSwitchConnectorAccount('agy', attempt('claude-sonnet')), null);
  assert.equal(h.active(), 'a');
});

const custom = () => ({ id: 'custom-test', name: 'Fixture provider', kind: 'openai', baseUrl: 'https://provider.invalid/v1', keyEnv: '', keys: ['a', 'b', 'c'].map((id) => ({ id, label: `Key ${id}`, apiKey: `fake-${id}` })), activeKeyId: 'a', autoSwitch: true, models: ['model'], defaultModel: 'model', enabled: true });
async function providerHarness(options) {
  const h = await harness(options);
  h.api.saveProvider(custom());
  return { ...h, provider: () => h.api.connectors.custom[0], probes: () => h.calls.filter((call) => call.command === 'provider_probe').map((call) => call.args.apiKey) };
}

for (const kind of ['openai', 'anthropic', 'ollama']) {
  test(`${kind}: custom provider activates a verified API key`, async () => {
    const h = await providerHarness();
    h.provider().kind = kind;
    assert.equal((await h.api.autoSwitchProviderKey(h.provider(), attempt())).label, 'Key b');
    assert.equal(h.provider().activeKeyId, 'b');
    assert.equal(h.calls.find((call) => call.command === 'provider_probe').args.kind, kind);
  });
}

test('API key authentication failure moves on to the next key', async () => {
  const h = await providerHarness({ onCall(command, args) {
    if (command === 'provider_probe' && args.apiKey === 'fake-b') return { ok: false, models: [], error: '401' };
  } });
  assert.equal((await h.api.autoSwitchProviderKey(h.provider(), attempt())).label, 'Key c');
  assert.deepEqual(h.probes(), ['fake-b', 'fake-c']);
});

test('API key with no remaining credit is skipped even if its model list would work', async () => {
  const h = await providerHarness({ onCall(command, args) {
    if (command === 'provider_quota' && args.apiKey === 'fake-b') return { used: 10, limit: 10, remaining: 0, free_tier: false };
  } });
  await h.api.autoSwitchProviderKey(h.provider(), attempt());
  assert.equal(h.provider().activeKeyId, 'c');
  assert.deepEqual(h.probes(), ['fake-c']);
});

test('model list success without access to the requested model does not select the key', async () => {
  const h = await providerHarness({ onCall(command, args) {
    if (command === 'provider_probe' && args.apiKey === 'fake-b') return { ok: true, models: ['other-model'] };
  } });
  await h.api.autoSwitchProviderKey(h.provider(), attempt());
  assert.equal(h.provider().activeKeyId, 'c');
});

test('unknown provider quota cannot create an endless A/B key loop', async () => {
  const h = await providerHarness();
  const recovery = attempt();
  await h.api.autoSwitchProviderKey(h.provider(), recovery);
  recovery.failedId = 'b'; recovery.tried.add('b');
  await h.api.autoSwitchProviderKey(h.provider(), recovery);
  recovery.failedId = 'c'; recovery.tried.add('c');
  assert.equal(await h.api.autoSwitchProviderKey(h.provider(), recovery), null);
  assert.equal(h.provider().activeKeyId, 'a');
  assert.deepEqual(h.probes(), ['fake-b', 'fake-c']);
});

test('two provider chats reuse the new key after waiting for the same lock', async () => {
  const h = await providerHarness();
  const provider = h.provider();
  await Promise.all([h.api.autoSwitchProviderKey(provider, attempt()), h.api.autoSwitchProviderKey(provider, attempt())]);
  assert.equal(h.provider().activeKeyId, 'b');
  assert.deepEqual(h.probes(), ['fake-b']);
});

test('cancelling a provider probe cannot commit or publish its key', async () => {
  const controller = new AbortController();
  const h = await providerHarness({ onCall(command) { if (command === 'provider_probe') controller.abort(); } });
  await assert.rejects(h.api.autoSwitchProviderKey(h.provider(), { ...attempt(), signal: controller.signal }), { name: 'AbortError' });
  assert.equal(h.provider().activeKeyId, 'a');
});

test('legacy single API key migrates with stable identity and retained secret', async () => {
  const h = await harness();
  const provider = custom();
  delete provider.keys; delete provider.activeKeyId;
  provider.apiKey = 'fake-legacy';
  const migrated = h.api.normalizeCustomProvider(provider);
  assert.equal(migrated.keys[0].apiKey, 'fake-legacy');
  assert.equal(migrated.activeKeyId, migrated.keys[0].id);
  assert.equal(h.api.normalizeCustomProvider(migrated).activeKeyId, migrated.activeKeyId);
  assert.equal(migrated.apiKey, undefined);
});

test('disabled auto-switch never mutates a connector profile', async () => {
  const h = await harness();
  h.api.connectors.autoSwitch.codex = false;
  assert.equal(await h.api.autoSwitchConnectorAccount('codex', attempt()), null);
  assert.deepEqual(h.native(), []);
});

test('OpenCode does not attempt native account rotation', async () => {
  const h = await harness();
  h.api.connectors.autoSwitch.opencode = true;
  assert.equal(await h.api.autoSwitchConnectorAccount('opencode', attempt()), null);
  assert.deepEqual(h.native(), []);
});

test('unknown quota ranks behind a known usable account', async () => {
  const h = await harness({ rows: [account('a', 100), { ...account('b'), usage: [] }, account('c', 70)] });
  assert.equal((await h.api.autoSwitchConnectorAccount('codex', attempt())).email, 'c@example.invalid');
  assert.deepEqual(h.native(), ['c']);
});

test('fresh account cache avoids a listing read before the first switch', async () => {
  const rows = [account('a', 100), account('b', 20), account('c', 70)];
  const h = await harness({ rows, prefs: { 'accounts.quota-cache': { savedAt: Date.now(), refreshedAt: { codex: Date.now() }, accounts: { codex: { manager: true, accounts: rows, error: null } }, agyAccounts: [], agyQuota: null } } });
  await h.api.autoSwitchConnectorAccount('codex', attempt());
  const firstSwitch = h.calls.findIndex((call) => call.command === 'account_switch');
  assert.ok(firstSwitch >= 0);
  assert.ok(h.calls.slice(0, firstSwitch).every((call) => call.command !== 'accounts_list'));
});

test('Antigravity commits a verified account once', async () => {
  const h = await harness({ provider: 'agy' });
  assert.equal((await h.api.autoSwitchConnectorAccount('agy', attempt('gemini-pro'))).email, 'b@example.invalid');
  assert.equal(h.active(), 'b');
  assert.deepEqual(h.epochs, ['antigravity']);
});

test('manual switch during auto verification waits for the original profile to be restored', async () => {
  let manual;
  let h;
  h = await harness({ onCall(command, _args, state) {
    if (command === 'accounts_list' && state.active === 'b' && !manual) {
      manual = h.api.switchAccount('codex', account('c'));
    }
  } });
  await assert.rejects(h.api.autoSwitchConnectorAccount('codex', attempt()), { name: 'AbortError' });
  assert.equal(await manual, true);
  assert.equal(h.active(), 'c');
  assert.deepEqual(h.native(), ['b', 'a', 'c']);
});

test('fresh key quota ranks the key with the most available credit first', async () => {
  const at = Date.now() / 1000;
  const h = await providerHarness({ prefs: { 'providers.quota-cache': { 'custom-test:b': { used: 9, limit: 10, remaining: 1, free_tier: false, at }, 'custom-test:c': { used: 0, limit: 10, remaining: 10, free_tier: false, at } } } });
  await h.api.autoSwitchProviderKey(h.provider(), attempt());
  assert.equal(h.provider().activeKeyId, 'c');
  assert.deepEqual(h.probes(), ['fake-c']);
  assert.equal(h.calls.filter((call) => call.command === 'provider_quota').length, 0);
});

test('stale exhausted quota is refreshed before a key is rejected', async () => {
  const h = await providerHarness({ prefs: { 'providers.quota-cache': { 'custom-test:b': { used: 10, limit: 10, remaining: 0, at: 1 } } }, onCall(command, args) {
    if (command === 'provider_quota' && args.apiKey === 'fake-b') return { used: 0, limit: 10, remaining: 10, free_tier: false };
  } });
  await h.api.autoSwitchProviderKey(h.provider(), attempt());
  assert.equal(h.provider().activeKeyId, 'b');
  assert.equal(h.prefs['providers.quota-cache']['custom-test:b'].remaining, 10);
});

test('all API keys fail: original key stays selected', async () => {
  const h = await providerHarness({ onCall(command) {
    if (command === 'provider_probe') return { ok: false, models: [], error: '401' };
  } });
  assert.equal(await h.api.autoSwitchProviderKey(h.provider(), attempt()), null);
  assert.equal(h.provider().activeKeyId, 'a');
  assert.deepEqual(h.probes(), ['fake-b', 'fake-c']);
});

test('a key removed while its probe is pending cannot be selected', async () => {
  let h;
  h = await providerHarness({ onCall(command, args) {
    if (command === 'provider_probe' && args.apiKey === 'fake-b') h.provider().keys = h.provider().keys.filter((key) => key.id !== 'b');
  } });
  await h.api.autoSwitchProviderKey(h.provider(), attempt());
  assert.equal(h.provider().activeKeyId, 'c');
});

test('provider disabled during connection verification cannot select a key', async () => {
  let h;
  h = await providerHarness({ onCall(command) {
    if (command === 'provider_probe') h.provider().autoSwitch = false;
  } });
  assert.equal(await h.api.autoSwitchProviderKey(h.provider(), attempt()), null);
  assert.equal(h.provider().activeKeyId, 'a');
});

test('account list carrying a native verification error cannot commit stale quota', async () => {
  const h = await harness({ onCall(command, _args, state) {
    if (command === 'accounts_list' && state.active === 'b') return { manager: true, accounts: [account('a', 100), { ...account('b', 10), active: true }, account('c', 70)].map((row) => ({ ...row, active: row.id === 'b' })), auto_switch: null, error: 'Could not verify connection' };
  } });
  assert.equal((await h.api.autoSwitchConnectorAccount('codex', attempt())).email, 'c@example.invalid');
  assert.deepEqual(h.native(), ['b', 'a', 'c']);
});
