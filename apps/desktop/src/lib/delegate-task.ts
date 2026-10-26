// Pure pieces of the `delegate_task` tool: an agent hands a task to another agent during its own turn (Rust side:
// src-tauri/src/delegate.rs; state and chats: sessions.svelte.ts). Free of stores and Tauri so plain node can test it.
// The tool is named by each agent CLI its own way: `mcp__agentdeck__delegate_task` (Claude, Antigravity),
// `agentdeck.delegate_task` (Codex), bare `delegate_task` (a local model); `toolKindOf` reads all of them.

export type ToolKind = 'delegate_task' | 'list_agents' | 'task_status' | 'task_stop';

/** Which of the delegation tools a tool-call name is, whatever the CLI calls it; null for any other tool. */
export function toolKindOf(name: string): ToolKind | null {
  const bare = name.trim().replace(/^mcp__.*__/, '').replace(/^agentdeck[.:/]/, '');
  return bare === 'delegate_task' || bare === 'list_agents' || bare === 'task_status' || bare === 'task_stop' ? bare : null;
}

/** The tool name of a `Name  argument` command line of a reply (see agent-reply.ts). */
const nameOfCommand = (command: string) => command.split(/\s+/, 1)[0] ?? '';
export const kindOfCommand = (command: string) => toolKindOf(nameOfCommand(command));

// ---------- arguments ----------

export const DEFAULT_TIMEOUT_S = 1800;
export const MIN_TIMEOUT_S = 10;
export const MAX_TIMEOUT_S = 7200;
export const TASK_MAX_CHARS = 30_000;

export type DelegateArgs = { agent: string; model?: string; task: string; wait: boolean; timeoutS: number };

const text = (value: unknown) => (typeof value === 'string' && value.trim() ? value.trim() : undefined);

/** The arguments of `delegate_task` with their defaults (wait: true, 30 minutes); the same rules as the Rust side. */
export function parseDelegateArgs(raw: unknown): { ok: true; args: DelegateArgs } | { ok: false; error: string } {
  const input = (raw && typeof raw === 'object' ? raw : {}) as Record<string, unknown>;
  const agent = text(input.agent);
  if (!agent) return { ok: false, error: 'agent is required: claude, codex, antigravity or an id from list_agents' };
  const task = text(input.task);
  if (!task) return { ok: false, error: 'task is required: say what the worker must do and report back' };
  if (task.length > TASK_MAX_CHARS) return { ok: false, error: `task is longer than ${TASK_MAX_CHARS} characters: give the goal and the paths, not whole files` };
  let wait = true;
  if (input.wait !== undefined && input.wait !== null) {
    const flag = typeof input.wait === 'string' ? { true: true, yes: true, '1': true, false: false, no: false, '0': false }[input.wait.trim().toLowerCase()] : typeof input.wait === 'boolean' ? input.wait : typeof input.wait === 'number' ? input.wait !== 0 : undefined;
    if (flag === undefined) return { ok: false, error: 'wait must be true or false' };
    wait = flag;
  }
  let timeoutS = DEFAULT_TIMEOUT_S;
  if (input.timeout_s !== undefined && input.timeout_s !== null) {
    const seconds = typeof input.timeout_s === 'number' ? input.timeout_s : typeof input.timeout_s === 'string' ? Number(input.timeout_s.trim()) : Number.NaN;
    if (!Number.isFinite(seconds)) return { ok: false, error: 'timeout_s must be a number of seconds' };
    timeoutS = Math.min(MAX_TIMEOUT_S, Math.max(MIN_TIMEOUT_S, Math.round(seconds)));
  }
  return { ok: true, args: { agent, model: text(input.model), task, wait, timeoutS } };
}

/** A task id as the tools hand it out: `t-` and a few letters and digits. */
export const newTaskId = () => `t-${Math.random().toString(36).slice(2, 8)}`;
export const isTaskId = (id: unknown): id is string => typeof id === 'string' && /^[A-Za-z0-9_-]{1,64}$/.test(id);

// ---------- agents and models ----------

export type ModelRow = { value: string; label: string; description?: string };
/** `restricted`: a free tier that may keep and train on what it is sent (see `privateContentIn`). */
export type AgentRow = { id: string; label: string; models: ModelRow[]; restricted?: boolean };

const norm = (value: string) => value.trim().toLowerCase();

/**
 * The model a task runs on. `wanted` is what the caller wrote: a model id, its label, or a part of either (`haiku`).
 * Nothing wanted: the first model whose id contains `prefer`, else the agent's first. A name that matches nothing is an
 * error that lists the real ones, so the caller can try again.
 */
export function resolveModel(models: ModelRow[], wanted?: string, prefer?: string): { model: string } | { error: string } {
  if (models.length === 0) return { error: 'that agent has no models available now' };
  const want = wanted ? norm(wanted) : '';
  if (!want) {
    const preferred = prefer ? models.find((item) => norm(item.value).includes(norm(prefer))) : undefined;
    return { model: (preferred ?? models[0]).value };
  }
  const found =
    models.find((item) => norm(item.value) === want) ??
    models.find((item) => norm(item.label) === want) ??
    models.find((item) => norm(item.value).includes(want) || norm(item.label).includes(want));
  if (found) return { model: found.value };
  return { error: `no model matches "${wanted}"; this agent offers: ${models.map((item) => item.label || item.value).join(', ')}` };
}

/** The agent a caller named: `claude`, `codex`, `antigravity` or the id (or name) of a local provider. */
export function findAgent(rows: AgentRow[], wanted: string): AgentRow | undefined {
  const want = norm(wanted);
  return rows.find((row) => norm(row.id) === want) ?? rows.find((row) => norm(row.label) === want);
}

/** The answer of `list_agents`. */
export function formatAgents(rows: AgentRow[]): string {
  if (rows.length === 0) return 'No agent is available to delegate to right now.';
  const lines = rows.map((row) => `- ${row.id}${row.label.toLowerCase() === row.id ? '' : ` (${row.label})`}: ${row.models.map((model) => (model.label && model.label !== model.value ? `${model.label} [${model.value}]` : model.value)).join(', ') || 'no models'}${row.restricted ? ` ${RESTRICTED_NOTE}` : ''}`);
  return `Agents you can pass as \`agent\` to delegate_task (a model name, or a part of it, goes in \`model\`):\n${lines.join('\n')}`;
}

// ---------- free-tier providers ----------

/** Said of a restricted provider in `list_agents`. */
export const RESTRICTED_NOTE =
  '[FREE TIER: may keep and train on what it is sent. Light public work only: web search, reading public pages, chat, emails, prompts, scripts and other writing, small text edits. It sees no file, project or tool of this machine: put everything it needs in `task`, with no source code, private data, secrets, passwords, environment variables, internal IPs or host names (AgentDeck blocks those).]';

/** Fenced code a task may carry to a free tier: a short public snippet, not a file of the project. */
const CODE_LINES_MAX = 30;

const SECRET_PATTERNS: [RegExp, string][] = [
  [/-----BEGIN [A-Z ]*PRIVATE KEY-----/, 'a private key'],
  [/\b(?:sk-(?:ant-|proj-)?[A-Za-z0-9_-]{20,}|gh[pousr]_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{30,}|glpat-[A-Za-z0-9_-]{20,}|xox[abposr]-[A-Za-z0-9-]{10,}|AKIA[0-9A-Z]{16}|AIza[0-9A-Za-z_-]{35})/, 'an API key or token'],
  [/\beyJ[A-Za-z0-9_-]{10,}\.eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}/, 'a signed token (JWT)'],
  [/\b[a-z][a-z0-9+.-]*:\/\/[^\s:/@]+:[^\s@/]+@/i, 'a URL with a password'],
  [/(?<![a-z])(?:password|passwd|pwd|secret|api[_-]?key|access[_-]?token|auth[_-]?token|client[_-]?secret)["']?\s*[:=]\s*["']?[^\s"']{6,}/i, 'a password or secret'],
  [/\bauthorization:\s*(?:bearer|basic)\s+\S{8,}/i, 'an authorization header'],
  [/^\s*(?:export\s+|set\s+|\$env:)?[A-Z][A-Z0-9_]*_(?:KEY|TOKEN|SECRET|PASSWORD|PASS|PWD|CREDENTIALS?|DSN|URL)\s*=\s*\S+/m, 'an environment variable'],
  [/\b(?:10(?:\.\d{1,3}){3}|127(?:\.\d{1,3}){3}|192\.168(?:\.\d{1,3}){2}|172\.(?:1[6-9]|2\d|3[01])(?:\.\d{1,3}){2}|169\.254(?:\.\d{1,3}){2})\b/, 'an internal IP address'],
  [/\b(?:localhost|[a-z0-9-]+(?:\.[a-z0-9-]+)*\.(?:internal|local|lan|corp|intranet|home\.arpa))(?::\d+)?\b/i, 'an internal host name'],
];

/**
 * What in a task must not reach a free-tier provider: a secret, or source code longer than a short snippet. `null`:
 * nothing found. A guard behind the caller's own judgement, not a replacement for it: the caller still decides what is
 * private.
 */
export function privateContentIn(task: string): string | null {
  for (const [pattern, what] of SECRET_PATTERNS) if (pattern.test(task)) return what;
  if ((task.match(/^\s*(?:export\s+)?[A-Z][A-Z0-9_]{2,}\s*=\s*\S/gm)?.length ?? 0) >= 3) return 'an environment file';
  const fenced = [...task.matchAll(/```[^\n]*\n([\s\S]*?)(?:```|$)/g)].reduce((sum, match) => sum + match[1].split('\n').length, 0);
  if (fenced > CODE_LINES_MAX) return `${fenced} lines of code`;
  return null;
}

// ---------- the report ----------

/** Longest report handed to the caller: about 600 words. */
export const DIGEST_MAX = 4800;

export type HandoffStatus = 'done' | 'blocked' | 'needs-plan-change' | 'unknown';

export type HandoffReport = {
  status: HandoffStatus;
  result: string;
  files: string[];
  facts: string[];
  verify: string[];
  open: string[];
};

const HANDOFF_FIELDS = new Set(['status', 'result', 'files', 'facts', 'verify', 'open']);

function listValue(value: string): string[] {
  return value
    .split(/\s*(?:;|\n)\s*/)
    .map((item) => item.replace(/^[-*]\s*/, '').trim())
    .filter(Boolean);
}

/** Parses the small machine-readable part of a worker's Handoff without constraining its prose. */
export function parseHandoff(answer: string): HandoffReport | null {
  const at = answer.search(/^#{1,3}\s*handoff\b/im);
  if (at < 0) return null;
  const body = answer.slice(at).replace(/^#{1,3}\s*handoff[^\n]*\n?/i, '');
  const fields = new Map<string, string>();
  let current = '';
  for (const line of body.split('\n')) {
    const match = /^\s*(STATUS|RESULT|FILES|FACTS|VERIFY|OPEN)\s*:\s*(.*)\s*$/i.exec(line);
    if (match) {
      current = match[1].toLowerCase();
      fields.set(current, match[2].trim());
      continue;
    }
    if (current && line.trim() && !/^\s*(?:[A-Z][A-Z -]+):/.test(line)) {
      fields.set(current, `${fields.get(current) ?? ''}\n${line.trim()}`.trim());
    }
  }
  if (![...fields.keys()].some((key) => HANDOFF_FIELDS.has(key))) return null;
  const rawStatus = fields.get('status')?.toLowerCase() ?? '';
  const status: HandoffStatus = rawStatus === 'done' || rawStatus === 'blocked' || rawStatus === 'needs-plan-change' ? rawStatus : 'unknown';
  return {
    status,
    result: fields.get('result') ?? '',
    files: listValue(fields.get('files') ?? ''),
    facts: listValue(fields.get('facts') ?? ''),
    verify: listValue(fields.get('verify') ?? ''),
    open: listValue(fields.get('open') ?? ''),
  };
}

/** What a worker is told to end with: the compact report that goes back to the agent that asked. */
export const REPORT_RULE =
  'End with "## Handoff" (<=600 words, fewer when the answer is short; only this goes back to the agent that asked, which answers the user from it; the user can read your full answer in your own chat): STATUS: done|blocked / RESULT: <the answer, telegraphic> / FILES: <paths> / FACTS: <<=8, "; "-separated, path:line> / OPEN: <<=3>. Telegraphic: no filler, no restating the task.';

/** The part of an answer meant for the next step: its "## Handoff" section, else its end. */
export function handoffOf(answer: string, max = 1600): string {
  const at = answer.search(/^#{1,3}\s*handoff\b/im);
  const part = (at >= 0 ? answer.slice(at).replace(/^#{1,3}\s*handoff[^\n]*\n?/i, '') : answer.slice(-1200)).trim();
  return part.length > max ? `${part.slice(0, max)}…` : part;
}

/** The report of a finished worker: its Handoff section, else its answer (its end when it is long), at most DIGEST_MAX. */
export function reportDigest(answer: string): string {
  const clean = answer.trim();
  if (/^#{1,3}\s*handoff\b/im.test(clean)) return handoffOf(clean, DIGEST_MAX);
  return clean.length > DIGEST_MAX ? `…${clean.slice(-DIGEST_MAX)}` : clean;
}

export type TaskState = 'running' | 'approval' | 'done' | 'failed' | 'stopped';
export type Verdict = 'done' | 'failed' | 'stopped' | 'timed out';

const oneLine = (reason: string, max = 300) => {
  const flat = reason.replace(/\s+/g, ' ').trim();
  return flat.length > max ? `${flat.slice(0, max - 1)}…` : flat;
};

/** The one-line reason for a task that gave no report. */
export function reasonLine(verdict: Verdict, detail = ''): string {
  const why = oneLine(detail);
  switch (verdict) {
    case 'stopped':
      return why ? `Stopped: ${why}` : 'Stopped by the user before it finished.';
    case 'timed out':
      return why || 'Timed out: the worker was stopped before it finished.';
    default:
      return `Failed: ${why || 'the worker gave no answer'}`;
  }
}

export type TaskHeader = { taskId: string; childId: string; label: string; verdict: Verdict | 'running'; took?: string };

/** What a tool returns for a task: a header line the card reads back after a reload, then the report or the reason. */
export function formatTaskResult(head: TaskHeader, body: string): string {
  return `[task ${head.taskId} · chat ${head.childId} · ${head.label} · ${head.verdict}${head.took ? ` · ${head.took}` : ''}]\n\n${body}`.trimEnd();
}

/** The header of `formatTaskResult`, read back; null for any other text. */
export function parseTaskResult(output: string): { taskId: string; childId: string; verdict: Verdict | 'running' } | null {
  const found = /^\[task (\S+) · chat (\S+) · .*? · (running|done|failed|stopped|timed out)(?: · [^\]]*)?\]/.exec(output.trimStart());
  return found ? { taskId: found[1], childId: found[2], verdict: found[3] as Verdict | 'running' } : null;
}

/** The immediate answer of a task started in the background. */
export function startedText(head: Omit<TaskHeader, 'verdict' | 'took'>): string {
  return formatTaskResult({ ...head, verdict: 'running' }, 'Started in the background: it keeps working while you carry on with other things. When it finishes its short report is delivered to you as a new message, so do not poll or wait for it; task_status and task_stop are there if you need them.');
}

// ---------- the card ----------

/** What the card needs from a delegate_task tool call: the arguments (read from the possibly cut-off input line). */
export type CallArgs = { agent?: string; model?: string; task?: string; wait?: boolean };

const unescape = (value: string) => value.replace(/\\(["\\/bnrt]|u[0-9a-fA-F]{4})/g, (_, code: string) => (code.startsWith('u') ? String.fromCharCode(parseInt(code.slice(1), 16)) : ({ b: '\b', n: '\n', r: '\r', t: '\t' } as Record<string, string>)[code] ?? code));

/**
 * The arguments of a delegate_task call from its input line. The CLIs cut a long input (200 to 400 characters), so a
 * line that is not whole JSON any more is read field by field: the task then ends where the line was cut.
 */
export function argsOfCall(input: string): CallArgs {
  const line = input.trim();
  try {
    const parsed: unknown = JSON.parse(line);
    if (parsed && typeof parsed === 'object') {
      const record = parsed as Record<string, unknown>;
      const wait = record.wait === undefined ? undefined : parseDelegateArgs({ agent: 'x', task: 'x', wait: record.wait });
      return { agent: text(record.agent), model: text(record.model), task: text(record.task), wait: wait?.ok ? wait.args.wait : undefined };
    }
  } catch {
    // Cut off: read what is there.
  }
  const field = (name: string) => {
    const found = new RegExp(`"${name}"\\s*:\\s*"((?:[^"\\\\]|\\\\.)*)("|$)`).exec(line);
    // A line the CLI cut ends with "…" and no closing quote: that mark is not the task's.
    return found ? text(unescape(found[2] ? found[1] : found[1].replace(/…$/, ''))) : undefined;
  };
  const wait = /"wait"\s*:\s*(false|"false")/i.test(line) ? false : /"wait"\s*:\s*(true|"true")/i.test(line) ? true : undefined;
  return { agent: field('agent'), model: field('model'), task: field('task'), wait };
}

/**
 * Which task a tool-call line belongs to. Several calls of one message reach the app in any order, so the call's own
 * arguments decide: the first free task of that agent whose text starts with the (possibly cut) task of the line, else
 * the first free one.
 */
export function pickTask(args: CallArgs, free: { id: string; agent: string; task: string }[]): string | undefined {
  const same = (item: { agent: string }) => !args.agent || norm(item.agent) === norm(args.agent);
  const exact = args.task ? free.find((item) => same(item) && item.task.startsWith(args.task!)) : undefined;
  return (exact ?? free.find(same) ?? free[0])?.id;
}

/** A task as the app keeps it while it runs and after (sessions.svelte.ts holds these). */
export type TaskRecord = {
  id: string;
  /** The chat that called delegate_task. */
  caller: string;
  childId: string;
  state: TaskState;
  wait: boolean;
  label: string;
  /** The kind of agent (`claude`, `codex`, `antigravity`, `custom`) and the id the caller named it by (`list_agents`). */
  agent: string;
  agentId: string;
  model: string;
  task: string;
  digest?: string;
  note?: string;
  startedAt: number;
  endedAt?: number;
};

export type CardState = 'running' | 'background' | 'approval' | 'done' | 'failed' | 'stopped';
export type TaskCard = {
  taskId?: string;
  childId?: string;
  state: CardState;
  agent?: string;
  model?: string;
  task?: string;
  wait: boolean;
  /** The report, or the reason there is none. */
  report: string;
  startedAt?: number;
  endedAt?: number;
};

/**
 * One delegate_task tool call of a reply as a card. While the app knows the task (`record`) it speaks for it; for a
 * chat read back from a file the call's own line and result say what they can: the arguments, the header of the result
 * (task id, the worker's chat, how it ended) and the report.
 */
export function cardOf(call: { command: string; output: string; status: 'running' | 'ok' | 'fail' }, record?: TaskRecord): TaskCard {
  const args = argsOfCall(call.command.replace(/^\S+\s*/, ''));
  const header = parseTaskResult(call.output);
  const wait = record?.wait ?? args.wait ?? true;
  if (record) {
    const state: CardState = record.state === 'running' && !record.wait ? 'background' : record.state;
    return { taskId: record.id, childId: record.childId, state, agent: record.agent, model: record.model, task: record.task, wait, report: record.digest ?? record.note ?? '', startedAt: record.startedAt, endedAt: record.endedAt };
  }
  const verdict = header?.verdict;
  const state: CardState =
    verdict === 'done' ? 'done' : verdict === 'failed' || verdict === 'timed out' ? 'failed' : verdict === 'stopped' ? 'stopped' : verdict === 'running' ? 'stopped' : call.status === 'running' ? (wait ? 'running' : 'background') : call.status === 'fail' ? 'failed' : 'done';
  const body = header ? call.output.slice(call.output.indexOf(']') + 1).trim() : call.output.trim();
  return { taskId: header?.taskId, childId: header?.childId, state, agent: args.agent, model: args.model, task: args.task, wait, report: body };
}

// ---------- what the agents are told ----------

/**
 * Said to an agent at the start of a conversation (see sessions.svelte.ts): when to hand work to another agent. The
 * tool descriptions say the same (src-tauri/src/delegate.rs). Kept short: it is sent with the first turn of every chat.
 */
export const DELEGATE_HINT =
  'Delegation (delegate_task, list_agents): choose the lightest useful shape for the request. Keep ordinary Q&A, a small edit, a quick command, and a simple artifact action in this chat; do them yourself and do not invent a plan for them. Delegate only when work is BIG or clearly separable; split by capability (investigator, implementer, verifier, artifact/browser specialist) only when the parts are genuinely independent. Give each worker a self-contained goal, paths/resources, constraints, and a compact evidence-first handoff. Independent tasks: wait:false, in parallel, and side by side; dependent tasks: wait (the default). Reuse the worker conversation for follow-up or correction, and replan only when a check or handoff shows a real gap. Never delegate just to relay the user\'s message or the final answer. Prefer 1-3 workers. One exception to "only big work": light public work (search, reading public info, chat, emails, prompts, scripts, small text edits) may go to an agent marked FREE TIER in list_agents, to save the paid agents\' usage. A free tier may train on what it is sent, so check the task yourself first: no source code of the project, no private or personal data, no keys, passwords, tokens, environment variables, internal IPs or host names, and no secret files (.env, keys, credentials). AgentDeck refuses such a task. Work that needs any of that goes to another agent or stays with you.';

/** The line the app puts before a prompt: its parts joined into one paragraph that starts with `[AgentDeck] `. */
export function agentdeckLead(...parts: string[]): string {
  const joined = parts.map((part) => part.replace(/^\[AgentDeck\]\s*/, '').trim()).filter(Boolean).join(' ');
  return joined ? `[AgentDeck] ${joined}\n\n` : '';
}
