// One loader per kind of work, so a glance at the spinner says what the agent is doing.
// The scroll-to-latest button keeps `conic` for itself; nothing else uses it.
import type { ZLoadingSpinner } from '@shival99/z-ui-svelte';

export const LOADER = {
  /** Waiting for the first token. */
  thinking: 'orbit',
  /** Reasoning text is streaming. */
  reasoning: 'triple',
  /** A shell command is running. */
  command: 'matrix',
  /** The agent is changing files. */
  action: 'polygon',
  /** Reading, searching, checking. */
  check: 'scan',
  /** Fetching from the network. */
  fetch: 'comet',
  /** A sub-agent or background task. */
  background: 'bar',
  /** The scroll-to-latest button while a reply is running. */
  scroll: 'conic',
} as const satisfies Record<string, ZLoadingSpinner>;

const RULES: [RegExp, ZLoadingSpinner][] = [
  [/^(Bash|shell|exec_command)\b/, LOADER.command],
  [/^(Edit|MultiEdit|Write|NotebookEdit|apply_patch)\b/, LOADER.action],
  [/^(Read|Grep|Glob|LS|WebSearch)\b/, LOADER.check],
  [/^(WebFetch|Fetch)\b/, LOADER.fetch],
  [/^(Task|Agent|BashOutput|KillShell|Monitor)\b/, LOADER.background],
];

/** The loader for a tool call line (`Name  argument`). */
export function loaderFor(command: string): ZLoadingSpinner {
  return RULES.find(([pattern]) => pattern.test(command))?.[1] ?? LOADER.command;
}
