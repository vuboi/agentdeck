// One loader per kind of work, so a glance at the spinner says what the agent is doing.
// The scroll-to-latest button keeps `conic` for itself; nothing else uses it.
import type { ZLoadingSize, ZLoadingSpinner } from '@shival99/z-ui-svelte';

/** Keep the animated work icons crisp and consistent in the compact chat timeline. */
export const LOADER_SIZE: ZLoadingSize = '2xs';

export const LOADER = {
  /** Matrix Trace's nine-node progress motif while the model decides what to do. */
  thinking: 'matrix-grid',
  /** Reasoning text is streaming. */
  reasoning: 'matrix-orbit',
  /** A shell command is running: the same motif as thinking, so the timeline has one busy icon. */
  command: 'matrix-grid',
  /** The agent is changing files. */
  action: 'merge',
  /** The reply text is streaming: the merge motif, from the same set. */
  writing: 'merge',
  /** Reading, searching, checking. */
  check: 'scan',
  /** Fetching from the network. */
  fetch: 'comet',
  /** A sub-agent or background task. */
  background: 'matrix-orbit',
  /** A delegated task waiting for an available worker. */
  queued: 'lift',
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
