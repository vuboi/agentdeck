// Pure helpers for how a reply is laid out: commands folded into one group, status wording, durations.
import type { ZLoadingSpinner } from '@shival99/z-ui-svelte';
import { LOADER } from './loaders';
import type { Block } from './mock';

export type CommandBlock = Extract<Block, { kind: 'command' }>;
export type Segment = { kind: 'block'; block: Block } | { kind: 'commands'; items: CommandBlock[] };

/** Consecutive commands become one group; everything else stays as it is. Reasoning with no text draws nothing,
 *  so it does not split a group either: the steps then count up on one line, as in Claude and Codex. */
export function segments(blocks: Block[]): Segment[] {
  const out: Segment[] = [];
  for (const block of blocks) {
    if (block.kind === 'loading') continue;
    if (block.kind === 'thinking' && !block.text.trim()) continue;
    const last = out.at(-1);
    if (block.kind !== 'command') out.push({ kind: 'block', block });
    else if (last?.kind === 'commands') last.items.push(block);
    else out.push({ kind: 'commands', items: [block] });
  }
  return out;
}

/** The tool name of a `Name  argument` command line. */
export const toolOf = (command: string) => command.split(/\s+/, 1)[0] ?? '';
/** The argument part of the line. */
export const argOf = (command: string) => command.slice(toolOf(command).length).trim();

const RUN = /^(Bash|shell|exec_command)$/;
const EDIT = /^(Edit|MultiEdit|Write|NotebookEdit|apply_patch)$/;
const READ = /^Read$/;
const SEARCH = /^(Grep|Glob|LS|WebSearch)$/;
const FETCH = /^(WebFetch|Fetch)$/;
const AGENT = /^(Task|Agent)$/;

const basename = (path: string) => path.split(/[\\/]+/).filter(Boolean).at(-1) ?? path;

export type Step = {
  /** Small muted word: "Ran", "Created", "Read". */
  verb: string;
  /** What it was done to: the command, the file name, the pattern. */
  subject: string;
  /** Lines added and removed, for file changes. */
  change?: { added: number; removed: number };
};

/** One row of the activity list. Wording follows the tool; a running step says "Running", a failed one "Failed". */
export function describeStep(item: CommandBlock): Step {
  const tool = toolOf(item.command);
  const running = item.status === 'running';
  const verbs = (done: string, doing: string) => (item.status === 'fail' ? 'Failed' : running ? doing : done);
  if (item.change) {
    const created = item.change.create === true;
    return {
      verb: verbs(created ? 'Created' : 'Edited', created ? 'Creating' : 'Editing'),
      subject: basename(item.change.path),
      change: { added: item.change.added, removed: item.change.removed },
    };
  }
  const subject = argOf(item.command);
  if (RUN.test(tool)) return { verb: verbs('Ran', 'Running'), subject };
  if (EDIT.test(tool)) return { verb: verbs('Edited', 'Editing'), subject: basename(subject) };
  if (READ.test(tool)) return { verb: verbs('Read', 'Reading'), subject: basename(subject) };
  if (SEARCH.test(tool)) return { verb: verbs('Searched', 'Searching'), subject };
  if (FETCH.test(tool)) return { verb: verbs('Fetched', 'Fetching'), subject };
  if (AGENT.test(tool)) return { verb: verbs('Ran agent', 'Running agent'), subject };
  return { verb: verbs('Used', 'Using'), subject: `${tool} ${subject}`.trim() };
}

const plural = (count: number, one: string, many = `${one}s`) => `${count} ${count === 1 ? one : many}`;

/** "Created 2 files, ran 8 commands": what a group of steps did, files first. */
export function summarize(items: CommandBlock[]): string {
  const files = (create: boolean) => new Set(items.filter((item) => item.change && (item.change.create === true) === create).map((item) => item.change!.path)).size;
  const created = files(true);
  const edited = files(false);
  const ran = items.filter((item) => !item.change && RUN.test(toolOf(item.command))).length;
  const looked = items.filter((item) => !item.change && !RUN.test(toolOf(item.command))).length;
  const parts = [
    created && `created ${plural(created, 'file')}`,
    edited && `edited ${plural(edited, 'file')}`,
    ran && `ran ${plural(ran, 'command')}`,
    looked && `looked at ${plural(looked, 'thing')}`,
  ].filter(Boolean) as string[];
  const text = parts.join(', ');
  return text.charAt(0).toUpperCase() + text.slice(1);
}

/** Total lines added and removed by a group, or null when it wrote no files. */
export function totalChange(items: CommandBlock[]): { added: number; removed: number } | null {
  const changes = items.flatMap((item) => (item.change ? [item.change] : []));
  if (changes.length === 0) return null;
  return { added: changes.reduce((sum, c) => sum + c.added, 0), removed: changes.reduce((sum, c) => sum + c.removed, 0) };
}

/** What the agent is doing right now, and the loader that goes with it. */
export function liveState(blocks: Block[]): { label: string; spinner: ZLoadingSpinner } {
  const last = blocks.filter((block) => block.kind !== 'loading').at(-1);
  if (last?.kind === 'thinking' && last.streaming) return { label: 'Reasoning', spinner: LOADER.reasoning };
  if (last?.kind === 'response' && last.streaming) return { label: 'Writing', spinner: LOADER.thinking };
  if (last?.kind === 'command' && last.status === 'running') {
    const tool = toolOf(last.command);
    if (EDIT.test(tool)) return { label: 'Editing', spinner: LOADER.action };
    if (READ.test(tool) || SEARCH.test(tool)) return { label: 'Checking', spinner: LOADER.check };
    if (FETCH.test(tool)) return { label: 'Fetching', spinner: LOADER.fetch };
    if (AGENT.test(tool)) return { label: 'Running agent', spinner: LOADER.background };
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
