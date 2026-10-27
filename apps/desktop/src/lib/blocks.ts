// Pure helpers for how a reply is laid out: commands folded into one group, status wording, durations.
import type { ZLoadingSpinner } from '@shival99/z-ui-svelte';
import { LOADER } from './loaders';
import { kindOfCommand } from './delegate-task';
import type { Block } from './mock';

export type CommandBlock = Extract<Block, { kind: 'command' }>;
export type ThinkingBlock = Extract<Block, { kind: 'thinking' }>;
export type PlanItem = { content: string; status: 'pending' | 'in_progress' | 'completed' };
export type Segment = { kind: 'plan'; items: PlanItem[] } | { kind: 'block'; block: Block } | /** Calls of delegate_task: each is a card of its own, not a line among the steps. */ { kind: 'tasks'; items: CommandBlock[] } | { kind: 'commands'; items: CommandBlock[]; /** Reasoning between the steps, folded into the group. */ thoughts: ThinkingBlock[] };

/** Consecutive commands become one group; everything else stays as it is. Reasoning with no text draws nothing,
 *  so it does not split a group either: the steps then count up on one line, as in Claude and Codex. Reasoning that
 *  has text and sits between steps (a local model thinks before nearly every one) joins the group too, as one
 *  "Thinking" entry inside it, instead of a box of its own between every two steps. */
/** The checklist of a TodoWrite / update_plan call, or null when the line is something else (or an old, cut-off one). */
export function planOf(command: string): PlanItem[] | null {
  if (!/^(TodoWrite|update_plan)\s/.test(command)) return null;
  try {
    const list: unknown = JSON.parse(argOf(command));
    if (!Array.isArray(list)) return null;
    return list.map((item: { content?: unknown; status?: unknown }) => ({
      content: String(item.content ?? ''),
      status: item.status === 'completed' || item.status === 'in_progress' ? item.status : 'pending',
    }));
  } catch {
    return null;
  }
}

export function segments(blocks: Block[]): Segment[] {
  const out: Segment[] = [];
  // Reasoning seen before its first step: it goes into the group that step starts.
  let pending: ThinkingBlock[] = [];
  // Reasoning with no step to join is one "Thinking" entry, however many parts it came in: Codex sends a dozen
  // reasoning items before an answer, and each used to be a box of its own.
  const flush = () => {
    if (pending.length === 1) out.push({ kind: 'block', block: pending[0] });
    else if (pending.length > 1)
      out.push({ kind: 'block', block: { kind: 'thinking', text: pending.map((thought) => thought.text.trim()).join('\n\n'), streaming: pending.some((thought) => thought.streaming) } });
    pending = [];
  };
  for (const block of blocks) {
    if (block.kind === 'loading') continue;
    if (block.kind === 'thinking') {
      if (!block.text.trim()) continue;
      const last = out.at(-1);
      if (last?.kind === 'commands') last.thoughts.push(block);
      else pending.push(block);
      continue;
    }
    if (block.kind !== 'command') {
      flush();
      out.push({ kind: 'block', block });
      continue;
    }
    if (kindOfCommand(block.command) === 'delegate_task') {
      flush();
      const shown = out.at(-1);
      if (shown?.kind === 'tasks') shown.items.push(block);
      else out.push({ kind: 'tasks', items: [block] });
      continue;
    }
    const plan = planOf(block.command);
    if (plan) {
      // One checklist per reply: a later call updates it where it first appeared.
      const shown = out.find((part) => part.kind === 'plan');
      if (shown?.kind === 'plan') shown.items = plan;
      else {
        flush();
        out.push({ kind: 'plan', items: plan });
      }
      continue;
    }
    const last = out.at(-1);
    if (last?.kind === 'commands') last.items.push(block);
    else {
      out.push({ kind: 'commands', items: [block], thoughts: pending });
      pending = [];
    }
  }
  flush();
  return out;
}

/** The tool name of a `Name  argument` command line; an MCP name (`mcp__agentdeck__browser_click`) is shown without its server. */
export const toolOf = (command: string) => (command.split(/\s+/, 1)[0] ?? '').replace(/^mcp__.*__/, '');
/** The argument part of the line. */
export const argOf = (command: string) => command.slice(toolOf(command).length).trim();

const RUN = /^(Bash|shell|exec_command)$/;
const EDIT = /^(Edit|MultiEdit|Write|NotebookEdit|apply_patch)$/;
const READ = /^Read$/;
const SEARCH = /^(Grep|Glob|LS|WebSearch)$/;
const FETCH = /^(WebFetch|Fetch)$/;
const AGENT = /^(Task|Agent)$/;
// The AgentDeck browser tools: what the agent did on the Preview page.
const PAGE_ACT = /^browser_(click|type|key|scroll|hover|fill|drag|upload)$/;
const PAGE_SCRIPT = /^browser_eval$/;
const PAGE_LOOK = /^browser_screenshot$/;
const PAGE_READ = /^browser_(dom|console|network|frames|page)$/;

const basename = (path: string) => path.split(/[\\/]+/).filter(Boolean).at(-1) ?? path;

export type Step = {
  /** Small muted word: "Ran", "Created", "Read". */
  verb: string;
  /** What it was done to: the command, the file name, the pattern. */
  subject: string;
  /** What a file change did. Only the non-zero parts show. */
  counts?: Counts;
};

/** Lines added and removed, and files created and deleted. */
export type Counts = { added: number; removed: number; created: number; deleted: number };

/** One row of the activity list. Wording follows the tool; a running step says "Running", a failed one "Failed". */
export function describeStep(item: CommandBlock): Step {
  const tool = toolOf(item.command);
  const running = item.status === 'running';
  const verbs = (done: string, doing: string) => (item.status === 'fail' ? 'Failed' : running ? doing : done);
  if (item.change) {
    const removed = tool === 'Delete';
    const created = !removed && item.change.create === true;
    return {
      verb: verbs(removed ? 'Deleted' : created ? 'Created' : 'Edited', removed ? 'Deleting' : created ? 'Creating' : 'Editing'),
      subject: basename(item.change.path),
      counts: { added: item.change.added, removed: item.change.removed, created: created ? 1 : 0, deleted: removed ? 1 : 0 },
    };
  }
  const subject = argOf(item.command);
  if (RUN.test(tool)) return { verb: verbs('Ran', 'Running'), subject };
  if (EDIT.test(tool)) return { verb: verbs('Edited', 'Editing'), subject: basename(subject) };
  if (READ.test(tool)) return { verb: verbs('Read', 'Reading'), subject: basename(subject) };
  if (SEARCH.test(tool)) return { verb: verbs('Searched', 'Searching'), subject };
  if (FETCH.test(tool)) return { verb: verbs('Fetched', 'Fetching'), subject };
  if (AGENT.test(tool)) return { verb: verbs('Ran agent', 'Running agent'), subject };
  if (PAGE_ACT.test(tool)) return { verb: verbs('Interacted with page', 'Interacting with page'), subject: '' };
  if (PAGE_SCRIPT.test(tool)) return { verb: verbs('Ran page script', 'Running page script'), subject: '' };
  if (tool === 'browser_open') return { verb: verbs('Opened a page', 'Opening a page'), subject: '' };
  if (tool === 'browser_serve') return { verb: verbs('Started a server', 'Starting a server'), subject: '' };
  if (PAGE_LOOK.test(tool)) return { verb: verbs('Took a screenshot', 'Taking a screenshot'), subject: '' };
  if (PAGE_READ.test(tool)) return { verb: verbs('Found on page', 'Finding on page'), subject: '' };
  if (tool === 'Plan') return { verb: verbs('Planned', 'Planning'), subject: '' };
  const delegation = kindOfCommand(item.command);
  if (delegation === 'delegate_task') return { verb: verbs('Messaged an agent', 'Messaging an agent'), subject: '' };
  if (delegation === 'list_agents') return { verb: verbs('Listed the agents', 'Listing the agents'), subject: '' };
  if (delegation === 'task_status') return { verb: verbs('Checked a task', 'Checking a task'), subject: '' };
  if (delegation === 'task_stop') return { verb: verbs('Stopped a task', 'Stopping a task'), subject: '' };
  if (tool === 'PackageInfo') return { verb: verbs('Checked the package', 'Checking the package'), subject };
  if (/^code_/.test(tool)) return { verb: verbs('Looked up in the code index', 'Looking up in the code index'), subject: `${tool.slice(5)} ${subject}`.trim() };
  return { verb: verbs('Used', 'Using'), subject: `${tool} ${subject}`.trim() };
}

const plural = (count: number, one: string, many = `${one}s`) => `${count} ${count === 1 ? one : many}`;

/** "Created 2 files, ran 8 commands": what a group of steps did, files first. */
export function summarize(items: CommandBlock[]): string {
  const isDelete = (item: CommandBlock) => toolOf(item.command) === 'Delete';
  const paths = (keep: (item: CommandBlock) => boolean) => new Set(items.filter((item) => item.change && keep(item)).map((item) => item.change!.path)).size;
  const created = paths((item) => !isDelete(item) && item.change!.create === true);
  const edited = paths((item) => !isDelete(item) && item.change!.create !== true);
  const deleted = paths(isDelete);
  const ran = items.filter((item) => !item.change && RUN.test(toolOf(item.command))).length;
  // Steps that only read or look something up: each kind says what it did, the rest is a plain tool call.
  const kinds = items.filter((item) => !item.change && !RUN.test(toolOf(item.command))).map((item) => toolOf(item.command));
  const read = kinds.filter((tool) => READ.test(tool)).length;
  const searched = kinds.filter((tool) => SEARCH.test(tool)).length;
  const fetched = kinds.filter((tool) => FETCH.test(tool)).length;
  const other = kinds.length - read - searched - fetched;
  const parts = [
    created && `created ${plural(created, 'file')}`,
    edited && `edited ${plural(edited, 'file')}`,
    deleted && `deleted ${plural(deleted, 'file')}`,
    ran && `ran ${plural(ran, 'command')}`,
    read && `read ${plural(read, 'file')}`,
    searched && `searched ${plural(searched, 'time', 'times')}`,
    fetched && `fetched ${plural(fetched, 'page')}`,
    other && `used ${plural(other, 'tool')}`,
  ].filter(Boolean) as string[];
  const text = parts.join(', ');
  return text.charAt(0).toUpperCase() + text.slice(1);
}

/** Lines and files a group changed, or null when it changed no files. */
export function totalChange(items: CommandBlock[]): Counts | null {
  const changes = items.filter((item) => item.change);
  if (changes.length === 0) return null;
  const isDelete = (item: CommandBlock) => toolOf(item.command) === 'Delete';
  return {
    added: changes.reduce((sum, item) => sum + item.change!.added, 0),
    removed: changes.reduce((sum, item) => sum + item.change!.removed, 0),
    created: changes.filter((item) => !isDelete(item) && item.change!.create === true).length,
    deleted: changes.filter(isDelete).length,
  };
}

/** What the agent is doing right now, and the loader that goes with it. */
export function liveState(blocks: Block[]): { label: string; spinner: ZLoadingSpinner } {
  const last = blocks.filter((block) => block.kind !== 'loading').at(-1);
  if (last?.kind === 'thinking' && last.streaming) return { label: 'Reasoning', spinner: LOADER.reasoning };
  if (last?.kind === 'response' && last.streaming) return { label: 'Writing', spinner: LOADER.thinking };
  if (last?.kind === 'command' && last.status === 'running') {
    const tool = toolOf(last.command);
    if (tool === 'Write') return { label: 'Creating', spinner: LOADER.action };
    if (EDIT.test(tool)) return { label: 'Editing', spinner: LOADER.action };
    if (READ.test(tool)) return { label: 'Reading', spinner: LOADER.check };
    if (SEARCH.test(tool)) return { label: 'Searching', spinner: LOADER.check };
    if (FETCH.test(tool)) return { label: 'Fetching', spinner: LOADER.fetch };
    if (AGENT.test(tool)) return { label: 'Running agent', spinner: LOADER.background };
    if (kindOfCommand(last.command) === 'delegate_task') return { label: 'Waiting for an agent', spinner: LOADER.background };
    if (PAGE_ACT.test(tool)) return { label: 'Interacting with page', spinner: LOADER.action };
    if (PAGE_SCRIPT.test(tool)) return { label: 'Running page script', spinner: LOADER.command };
    if (PAGE_LOOK.test(tool) || PAGE_READ.test(tool)) return { label: 'Reading the page', spinner: LOADER.check };
    return { label: 'Running command', spinner: LOADER.command };
  }
  return { label: 'Thinking', spinner: LOADER.thinking };
}

/** `24s`, `1m 05s`. */
export function elapsed(ms: number): string {
  const seconds = Math.max(0, Math.round(ms / 1000));
  if (seconds < 60) return `${seconds}s`;
  return `${Math.floor(seconds / 60)}m ${String(seconds % 60).padStart(2, '0')}s`;
}

/** The text of the answer, for the copy button. */
export function answerText(blocks: Block[]): string {
  return blocks
    .filter((block) => block.kind === 'response')
    .map((block) => block.text)
    .join('\n\n');
}

/** `4 minutes ago`. */
export function ago(iso: string, now: number): string {
  const seconds = Math.round((new Date(iso).getTime() - now) / 1000);
  if (Number.isNaN(seconds)) return '';
  const rtf = new Intl.RelativeTimeFormat('en', { numeric: 'auto' });
  const steps: [Intl.RelativeTimeFormatUnit, number][] = [['day', 86400], ['hour', 3600], ['minute', 60]];
  for (const [unit, size] of steps) {
    if (Math.abs(seconds) >= size) return rtf.format(Math.round(seconds / size), unit);
  }
  return 'just now';
}

/** `Oct 22, 2026, 11:17 AM`. */
export const fullDate = (iso: string) =>
  new Date(iso).toLocaleString('en-US', { dateStyle: 'medium', timeStyle: 'short' });
