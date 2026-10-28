import { activity, audit, type Activity } from './activity.svelte';
import { inTauri } from './history';
import { getPref, setPref } from './prefs';
import { toast } from './toast';

// The MCP tab: the servers the agent CLIs know (read by Rust, `mcp_manage.rs`), whether each one answers, and how
// much the agents used them. Usage is counted from the Tasks log (every tool call is in it), so it covers every
// server with no extra bookkeeping.

export type McpAgent = 'claude' | 'codex' | 'antigravity' | 'copilot';
export type McpEntry = {
  id: string;
  name: string;
  transport: string;
  target: string;
  env_keys: string[];
  agents: string[];
  scope: string;
  source: string;
  builtin: boolean;
  disabled: boolean;
};
export type McpOverview = { bridge: string | null; agy_link: string | null; agy_auto: boolean; entries: McpEntry[] };
export type TestResult = { ok: boolean; ms: number; server: string | null; tools: string[]; error: string | null };
export type AgentResult = { agent: string; ok: boolean; message: string };
export type AddSpec = { name: string; transport: 'stdio' | 'http'; target: string; args: string[]; env: string[]; headers: string[]; agents: McpAgent[] };

export type TestState = { state: 'testing' | 'ok' | 'failed'; result?: TestResult; at: number };

export const mcp = $state({
  overview: null as McpOverview | null,
  loading: false,
  error: '',
  tests: {} as Record<string, TestState>,
  /** Saving or removing right now. */
  busy: '',
});

const call = async <T>(command: string, args?: Record<string, unknown>): Promise<T> => {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<T>(command, args);
};
const message = (error: unknown) => (error instanceof Error ? error.message : String(error));

export async function loadMcp() {
  if (!inTauri() || mcp.loading) return;
  mcp.loading = true;
  mcp.error = '';
  try {
    mcp.overview = await call<McpOverview>('mcp_list');
  } catch (error) {
    mcp.error = message(error);
  } finally {
    mcp.loading = false;
  }
}

export async function testMcp(id: string) {
  if (!inTauri() || mcp.tests[id]?.state === 'testing') return;
  mcp.tests[id] = { state: 'testing', at: Date.now() };
  try {
    const result = await call<TestResult>('mcp_test', { id });
    mcp.tests[id] = { state: result.ok ? 'ok' : 'failed', result, at: Date.now() };
  } catch (error) {
    mcp.tests[id] = { state: 'failed', result: { ok: false, ms: 0, server: null, tools: [], error: message(error) }, at: Date.now() };
  }
}

export async function testAllMcp() {
  const ids = (mcp.overview?.entries ?? []).filter((entry) => !entry.disabled).map((entry) => entry.id);
  // A few at a time: each stdio server is a process.
  const queue = [...ids];
  const worker = async () => {
    for (let id = queue.shift(); id; id = queue.shift()) await testMcp(id);
  };
  await Promise.all([worker(), worker(), worker()]);
}

function report(results: AgentResult[], done: string) {
  const failed = results.filter((item) => !item.ok);
  if (failed.length === 0) toast.success(done);
  else toast.error(failed.map((item) => `${item.agent}: ${item.message}`).join(' · '));
}

/** Adds a server to the chosen agents. Returns whether every agent took it. */
export async function addMcp(spec: AddSpec): Promise<boolean> {
  mcp.busy = 'add';
  try {
    const results = await call<AgentResult[]>('mcp_add', { spec });
    report(results, `${spec.name} added`);
    await loadMcp();
    return results.every((item) => item.ok);
  } catch (error) {
    toast.error(message(error));
    return false;
  } finally {
    mcp.busy = '';
  }
}

export async function removeMcp(entry: McpEntry) {
  mcp.busy = `remove:${entry.id}`;
  try {
    report(await call<AgentResult[]>('mcp_remove', { name: entry.name, agents: entry.agents }), `${entry.name} removed`);
    delete mcp.tests[entry.id];
    await loadMcp();
  } catch (error) {
    toast.error(message(error));
  } finally {
    mcp.busy = '';
  }
}

/** Gives Antigravity AgentDeck's browser tools, or takes them away. */
export async function linkAgy(enable: boolean) {
  mcp.busy = 'agy';
  try {
    const result = await call<AgentResult>('mcp_agy_link', { enable });
    if (result.ok) toast.success(enable ? 'Antigravity can use the AgentDeck browser' : 'Antigravity no longer has the AgentDeck browser');
    else toast.error(result.message);
    await loadMcp();
  } catch (error) {
    toast.error(message(error));
  } finally {
    mcp.busy = '';
  }
}

// --- Usage, counted from the Tasks log ---------------------------------------------------------------------

export type Usage = { calls: number; failed: number; tokens: number; lastAt: number; recentFailed: boolean; tools: { name: string; calls: number }[] };

/** The server and tool a call was made to: `mcp__server__tool` (Claude) or `server.tool` (Codex). */
export function mcpCall(name: string, known: Set<string>): { server: string; tool: string } | null {
  const claude = /^mcp__(.+?)__(.+)$/.exec(name);
  if (claude) return { server: claude[1], tool: claude[2] };
  const dot = name.indexOf('.');
  if (dot > 0 && known.has(name.slice(0, dot))) return { server: name.slice(0, dot), tool: name.slice(dot + 1) };
  return null;
}

/** About 4 characters a token: the CLIs do not report tokens per tool call, so this is an estimate. */
const estimate = (item: Activity) => Math.ceil((item.input.length + (item.output?.length ?? 0)) / 4);

export function usageByServer(known: Set<string>): Map<string, Usage> {
  const found = new Map<string, Usage & { list: Activity[] }>();
  const seen = new Set<string>();
  for (const item of [...activity, ...audit]) {
    if (seen.has(item.key)) continue;
    seen.add(item.key);
    const hit = mcpCall(item.name, known);
    if (!hit) continue;
    const entry = found.get(hit.server) ?? { calls: 0, failed: 0, tokens: 0, lastAt: 0, recentFailed: false, tools: [], list: [] };
    entry.calls += 1;
    entry.failed += item.status === 'fail' ? 1 : 0;
    entry.tokens += estimate(item);
    entry.lastAt = Math.max(entry.lastAt, item.startedAt);
    entry.list.push(item);
    const tool = entry.tools.find((candidate) => candidate.name === hit.tool);
    if (tool) tool.calls += 1;
    else entry.tools.push({ name: hit.tool, calls: 1 });
    found.set(hit.server, entry);
  }
  for (const entry of found.values()) {
    entry.tools.sort((a, b) => b.calls - a.calls);
    entry.recentFailed = entry.list.sort((a, b) => b.startedAt - a.startedAt).slice(0, 5).some((item) => item.status === 'fail');
  }
  return found;
}

export type Health = 'working' | 'failed' | 'failing' | 'testing' | 'untested';

/** One word for the row: a test wins while it is fresh; without one, the last few real calls decide. */
export function healthOf(entry: McpEntry, usage: Usage | undefined): Health {
  const test = mcp.tests[entry.id];
  if (test?.state === 'testing') return 'testing';
  if (test?.state === 'failed') return 'failed';
  if (usage?.recentFailed) return 'failing';
  if (test?.state === 'ok') return 'working';
  if (usage && usage.calls > 0) return 'working';
  return 'untested';
}

// Last test results survive a restart (so the tab does not look untested every time).
let hydrated = false;
void getPref<Record<string, TestState>>('mcp.tests').then((saved) => {
  if (saved) for (const [id, state] of Object.entries(saved)) if (state.state !== 'testing') mcp.tests[id] = state;
  hydrated = true;
});
$effect.root(() => {
  $effect(() => {
    const snapshot = $state.snapshot(mcp.tests);
    if (hydrated) setPref('mcp.tests', snapshot);
  });
});
