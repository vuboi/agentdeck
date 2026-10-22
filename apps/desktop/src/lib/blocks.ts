// Pure helpers for how a reply is laid out: commands folded into one group, status wording, durations.
import type { ZLoadingSpinner } from '@shival99/z-ui-svelte';
import { LOADER } from './loaders';
import type { Block } from './mock';

export type CommandBlock = Extract<Block, { kind: 'command' }>;
export type Segment = { kind: 'block'; block: Block } | { kind: 'commands'; items: CommandBlock[] };

/** Consecutive commands become one group; everything else stays as it is. */
export function segments(blocks: Block[]): Segment[] {
  const out: Segment[] = [];
  for (const block of blocks) {
    if (block.kind === 'loading') continue;
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

/** Row title of one command: "Ran a command", "Edited a file", … */
export function commandTitle(item: CommandBlock): string {
  const tool = toolOf(item.command);
  const running = item.status === 'running';
  if (RUN.test(tool)) return item.status === 'fail' ? 'Command failed' : running ? 'Running a command' : 'Ran a command';
  if (EDIT.test(tool)) return running ? 'Editing a file' : 'Edited a file';
  if (READ.test(tool)) return running ? 'Reading a file' : 'Read a file';
  if (SEARCH.test(tool)) return running ? 'Searching' : 'Searched';
  if (FETCH.test(tool)) return running ? 'Fetching a page' : 'Fetched a page';
  if (AGENT.test(tool)) return running ? 'Running an agent' : 'Ran an agent';
  return running ? `Using ${tool}` : `Used ${tool}`;
}

/** Group title: "Ran 2 commands", or "actions" when the tools are mixed. */
export function groupTitle(items: CommandBlock[]): string {
  const allCommands = items.every((item) => RUN.test(toolOf(item.command)));
  const noun = allCommands ? 'command' : 'action';
  return items.length === 1 ? `Ran ${allCommands ? 'a command' : 'an action'}` : `Ran ${items.length} ${noun}s`;
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
