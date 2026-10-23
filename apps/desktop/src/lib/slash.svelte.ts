import { inTauri } from './history';

// The `/` palette. Two sources: AgentDeck's own commands (below), and the commands and skills the chat's agent
// already has on disk (read by Rust, `slash_commands.rs`). Choosing one puts `/name ` in the message box; when the
// message is sent, `expandSlash` swaps the command for its text, so the chat shows what was typed and the agent gets
// the full instructions. That works the same for every agent: nothing depends on the CLI knowing the command.

export type SlashAgent = 'claude' | 'codex' | 'antigravity' | 'custom';
export type SlashItem = {
  name: string;
  description: string;
  /** `builtin`: AgentDeck's own; `prompt`: a command file of the agent; `skill`: a SKILL.md of the agent. */
  kind: 'builtin' | 'prompt' | 'skill';
  /** For files: where the text is read from when the command is used. */
  path?: string;
  /** Text of a builtin, with `$ARGUMENTS` where what the user typed after the name goes. */
  template?: string;
  origin: string;
};

/** AgentDeck's own commands: a way of working, written once, usable with any agent. */
export const BUILTIN: SlashItem[] = [
  {
    name: 'feature',
    description: 'Build a feature end to end, with tests',
    kind: 'builtin',
    origin: 'AgentDeck',
    template:
      'Implement this feature: $ARGUMENTS\n\nFirst read the code around it and follow the patterns already used. Write a short plan, then make the smallest complete change that does the job. Add or update tests, run the project\'s checks (build, lint, tests), and fix what they report. Finish with what changed, what you ran, and what you could not verify.',
  },
  {
    name: 'fixbug',
    description: 'Find the root cause, fix it, prove it',
    kind: 'builtin',
    origin: 'AgentDeck',
    template:
      'Fix this bug: $ARGUMENTS\n\nReproduce it first and say how. Find the root cause, not just the symptom. Make the smallest fix at the layer where the fault is, add a test that fails without the fix and passes with it, and run the checks. Say what the cause was and how you confirmed the fix.',
  },
  {
    name: 'debug',
    description: 'Debug a page: open it, read console and network',
    kind: 'builtin',
    origin: 'AgentDeck',
    template:
      'Debug this: $ARGUMENTS\n\nUse the AgentDeck browser (the agentdeck tools: browser_serve or browser_open to open the page, browser_screenshot, browser_console, browser_network, browser_dom). Start the dev server with browser_serve if it is not running. Reproduce the problem on the page, then read the console errors and the failed network requests (with bodies) and work out the root cause. Fix it, reload, and check again that the console is clean and the requests succeed.',
  },
  {
    name: 'review',
    description: 'Review the current changes for bugs',
    kind: 'builtin',
    origin: 'AgentDeck',
    template:
      'Review the changes in this working tree (git diff, including staged and untracked files). $ARGUMENTS\n\nLook for real bugs first: wrong logic, missed cases, broken error handling, security problems, races. Then tests that are missing. Skip style. For each finding give the file and line, what goes wrong with a concrete input, and the fix. If there is nothing, say so.',
  },
  {
    name: 'refactor',
    description: 'Restructure without changing behaviour',
    kind: 'builtin',
    origin: 'AgentDeck',
    template:
      'Refactor: $ARGUMENTS\n\nKeep behaviour exactly the same. Run the tests before you start and after each step, and stop if they change. Prefer small steps that each leave the code working. Reuse what already exists instead of adding a second way. Summarise what moved and what you checked.',
  },
  {
    name: 'test',
    description: 'Write the missing tests',
    kind: 'builtin',
    origin: 'AgentDeck',
    template:
      'Write tests for: $ARGUMENTS\n\nFollow the test style already in the project. Cover the normal path, the edges and the failures that matter, and make sure each test can actually fail. Run them and report the result.',
  },
  {
    name: 'explain',
    description: 'Explain how something works',
    kind: 'builtin',
    origin: 'AgentDeck',
    template:
      'Explain how this works: $ARGUMENTS\n\nRead the code and answer from what it does, not from guesses. Start with the one-sentence answer, then the path through the code with file:line references, then anything surprising. Do not change any file.',
  },
  {
    name: 'plan',
    description: 'Plan only, change nothing',
    kind: 'builtin',
    origin: 'AgentDeck',
    template:
      'Make a plan for: $ARGUMENTS\n\nDo not edit any file. Read what you need, then give the steps in order, the files each one touches, the risks, and how each step is verified. Ask me about anything that changes the plan.',
  },
  {
    name: 'perf',
    description: 'Find and fix a slowdown',
    kind: 'builtin',
    origin: 'AgentDeck',
    template:
      'Something is slow: $ARGUMENTS\n\nMeasure before changing anything (timings, profile, network waterfall, query plan). Find the biggest cost, fix that one, measure again and report the before and after. Do not optimise what the numbers do not point at.',
  },
  {
    name: 'docs',
    description: 'Write or update documentation',
    kind: 'builtin',
    origin: 'AgentDeck',
    template:
      'Write or update the documentation for: $ARGUMENTS\n\nRead the code first so every statement is true. Match the tone and layout of the docs already here. Keep it short, with a runnable example where it helps.',
  },
];

export const slash = $state({ agent: '' as SlashAgent | '', cwd: '', items: [] as SlashItem[], loading: false });

type Listed = { name: string; description: string; kind: 'prompt' | 'skill'; path: string; origin: string };

let pending: Promise<void> | null = null;

/** Loads what `agent` has for the project at `cwd`. Builtins come first; a provider command of the same name is kept too. */
export async function loadSlash(agent: SlashAgent, cwd: string | null): Promise<void> {
  if (pending) await pending;
  if (`${slash.agent}|${slash.cwd}` === `${agent}|${cwd ?? ''}`) return;
  pending = (async () => {
    slash.loading = true;
    try {
      let listed: Listed[] = [];
      if (inTauri()) {
        const { invoke } = await import('@tauri-apps/api/core');
        listed = await invoke<Listed[]>('commands_list', { agent, cwd });
      }
      slash.items = [...BUILTIN, ...listed.map((item) => ({ ...item }))];
    } catch {
      slash.items = [...BUILTIN];
    } finally {
      slash.agent = agent;
      slash.cwd = cwd ?? '';
      slash.loading = false;
    }
  })();
  try {
    await pending;
  } finally {
    pending = null;
  }
}

/** Forces the next `loadSlash` to read the disk again (new files, another project). */
export const resetSlash = () => {
  slash.agent = '';
  slash.cwd = '';
};

const SLASH = /^\/([A-Za-z0-9_.:-]+)(?:[ \t]+([\s\S]*))?$/;

/** The words typed after the name: `$ARGUMENTS` is all of them, `$1`, `$2`… one by one. */
function fill(template: string, args: string): string {
  const words = args.match(/"[^"]*"|'[^']*'|\S+/g)?.map((word) => word.replace(/^["']|["']$/g, '')) ?? [];
  const used = template.includes('$ARGUMENTS') || /\$\d/.test(template);
  const text = template.replaceAll('$ARGUMENTS', args).replace(/\$(\d)/g, (_, index: string) => words[Number(index) - 1] ?? '');
  return used || !args ? text : `${text}\n\n${args}`;
}

/**
 * The text to send for a message that starts with a known `/command`; the message itself when it does not.
 * Unknown names are left alone, so a path or a CLI's own command typed with a slash still goes through.
 */
export async function expandSlash(text: string, agent: SlashAgent, cwd: string | null): Promise<string> {
  const match = SLASH.exec(text.trim());
  if (!match) return text;
  await loadSlash(agent, cwd);
  const [, name, rest = ''] = match;
  const args = rest.trim();
  // The agent's own command of that name wins over AgentDeck's: it is what the user set up.
  const item = slash.items.find((entry) => entry.name === name && entry.kind !== 'builtin') ?? slash.items.find((entry) => entry.name === name);
  if (!item) return text;
  if (item.kind === 'builtin') return fill(item.template ?? '', args);
  if (!item.path) return text;
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    const body = await invoke<string>('command_read', { path: item.path, cwd });
    if (item.kind === 'skill') {
      return `Use the "${item.name}" skill. Its instructions:\n\n${body.trim()}\n\n---\nTask: ${args || '(none given: ask what is needed)'}`;
    }
    return fill(body, args);
  } catch {
    return text;
  }
}
