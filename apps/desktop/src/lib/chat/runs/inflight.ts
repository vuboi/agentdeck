// What was still running when the page went away (a reload, or a hot update in dev): the Rust side keeps the shells and
// the dev servers going, so the Tasks panel and the background list must find them again. The records are small and
// live in localStorage; the decisions about them are pure functions, tested in inflight.test.ts.

const KEY = 'agentdeck.inflight.v1';

/** A command a Terminal tab runs. */
export type InflightTerminal = { tabId: string; command: string; cwd?: string; startedAt: number };
/** A project started from the Preview. `settled`: its end is already in the Audit log (it came up or failed). */
export type InflightRun = { sessionId: string; title: string; project: string; input: string; port?: number; startedAt: number; settled: boolean };
export type Inflight = { terminals: InflightTerminal[]; runs: InflightRun[] };

export const emptyInflight = (): Inflight => ({ terminals: [], runs: [] });

const isText = (value: unknown): value is string => typeof value === 'string';
const isTime = (value: unknown): value is number => typeof value === 'number' && Number.isFinite(value);

/** Reads what was saved; anything malformed (or of another version) is dropped, never thrown. */
export function parseInflight(raw: string | null): Inflight {
  const out = emptyInflight();
  if (!raw) return out;
  try {
    const data = JSON.parse(raw) as { terminals?: unknown; runs?: unknown };
    if (Array.isArray(data.terminals)) {
      for (const item of data.terminals as Partial<InflightTerminal>[]) {
        if (item && isText(item.tabId) && isText(item.command) && isTime(item.startedAt)) out.terminals.push({ tabId: item.tabId, command: item.command, cwd: isText(item.cwd) ? item.cwd : undefined, startedAt: item.startedAt });
      }
    }
    if (Array.isArray(data.runs)) {
      for (const item of data.runs as Partial<InflightRun>[]) {
        if (item && isText(item.input) && isTime(item.startedAt)) {
          out.runs.push({ sessionId: isText(item.sessionId) ? item.sessionId : '', title: isText(item.title) ? item.title : '', project: isText(item.project) ? item.project : '', input: item.input, port: isTime(item.port) ? item.port : undefined, startedAt: item.startedAt, settled: item.settled === true });
        }
      }
    }
  } catch {
    // A damaged save is the same as none.
  }
  return out;
}

export function loadInflight(): Inflight {
  try {
    return parseInflight(globalThis.localStorage?.getItem(KEY) ?? null);
  } catch {
    return emptyInflight();
  }
}

export function saveInflight(state: Inflight) {
  try {
    if (state.terminals.length === 0 && state.runs.length === 0) globalThis.localStorage?.removeItem(KEY);
    else globalThis.localStorage?.setItem(KEY, JSON.stringify(state));
  } catch {
    // Storage is full or blocked: the entries are just not kept across a reload.
  }
}

/** Changes the saved state in one read-modify-write. */
export function updateInflight(change: (state: Inflight) => void) {
  const state = loadInflight();
  change(state);
  saveInflight(state);
}

/** A tab ran a command: kept (one per tab). */
export function withTerminal(state: Inflight, record: InflightTerminal): Inflight {
  return { ...state, terminals: [...state.terminals.filter((item) => item.tabId !== record.tabId), record] };
}
export function withoutTerminal(state: Inflight, tabId: string): Inflight {
  return { ...state, terminals: state.terminals.filter((item) => item.tabId !== tabId) };
}

/** What became of a command that a tab was running before the reload: its tab, and whether its shell is back at a prompt. */
export function terminalVerdict(alive: boolean, atPrompt: boolean): 'running' | 'ok' | 'stopped' {
  if (!alive) return 'stopped';
  return atPrompt ? 'ok' : 'running';
}

/** What a server of the Rust side's list looks like to the plan. */
export type ServerLike = { key: number; command: string; cwd: string; running: boolean; age: number };

const inputOf = (server: ServerLike) => `${server.command}\n${server.cwd}`;
const same = (a: string, b: string) => {
  const [ac, ad] = a.split('\n');
  const [bc, bd] = b.split('\n');
  return ac?.trim() === bc?.trim() && ad === bd;
};

export type RunPlan = {
  /** Servers that still run and get a running entry again, with the record that belongs to them (if any) and its start. */
  resume: { server: ServerLike; record?: InflightRun; startedAt: number }[];
  /** Records that were still starting and whose server is gone: the Audit log gets a stopped entry. */
  stopped: InflightRun[];
  /** Records of a server that is gone but whose end the log already has: just forgotten. */
  forget: InflightRun[];
};

/**
 * Matches the saved Run project records with the servers the Rust side still lists. `known` are the entries the
 * panel already has (a second restore, a hot update that kept them): they are not made twice.
 */
export function planRunRestore(records: InflightRun[], servers: ServerLike[], known: { ports: Set<number>; inputs: string[] }, now: number): RunPlan {
  const plan: RunPlan = { resume: [], stopped: [], forget: [] };
  const used = new Set<InflightRun>();
  for (const server of servers) {
    const record = records.find((item) => !used.has(item) && (item.port !== undefined ? item.port === server.key : same(item.input, inputOf(server))));
    if (record && server.running) used.add(record);
    if (!server.running) continue;
    if (known.ports.has(server.key) || known.inputs.some((input) => same(input, inputOf(server)))) continue;
    plan.resume.push({ server, record, startedAt: record?.startedAt ?? Math.max(0, now - server.age * 1000) });
  }
  for (const record of records) {
    if (used.has(record)) continue;
    (record.settled ? plan.forget : plan.stopped).push(record);
  }
  return plan;
}

/** What `trackTerminal` does with a report: every run has its own id, so the same command run again is a new run. */
export function terminalStep(currentRunId: number | undefined, run: { runId: number } | null): 'none' | 'start' | 'finish' | 'restart' {
  if (currentRunId === undefined) return run ? 'start' : 'none';
  if (!run) return 'finish';
  return run.runId === currentRunId ? 'none' : 'restart';
}

const SET_LOCATION = /^Set-Location -LiteralPath '((?:[^']|'')*)'; ([\s\S]+)$/;
const CD = /^cd '((?:[^']|'\\'')*)' && ([\s\S]+)$/;

/** The Run button puts a move into the project's folder before the command (`terminalCommand`): the card shows only the command. */
export function stripLocationPrefix(command: string): { command: string; cwd?: string } {
  const text = command.trim();
  const windows = SET_LOCATION.exec(text);
  if (windows) return { command: windows[2].trim(), cwd: windows[1].replaceAll("''", "'") };
  const posix = CD.exec(text);
  if (posix) return { command: posix[2].trim(), cwd: posix[1].replaceAll("'\\''", "'") };
  return { command: text };
}
