import { inTauri } from '../app/platform/history';

// The `/` palette. Two sources: AgentDeck's own commands (below), and the commands and skills the chat's agent
// already has on disk (read by Rust, `slash_commands.rs`). Choosing one puts `/name ` in the message box; when the
// message is sent, `expandSlash` swaps the command for its text, so the chat shows what was typed and the agent gets
// the full instructions. That works the same for every agent: nothing depends on the CLI knowing the command.

export type SlashAgent = 'claude' | 'codex' | 'antigravity' | 'opencode' | 'copilot' | 'custom';
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

/**
 * How a user guide is made: the agent itself works through the site in the Preview, marks each step on the page and
 * saves the pictures, then builds the requested file around them. Shared by `/docs` (when the request is a how-to)
 * and `/document`.
 */
const GUIDE_WORKFLOW = `HOW TO MAKE THE GUIDE (do every step yourself; never ask the user to click, take screenshots or record):

1. Format. Use the one asked for: Word (.docx), PDF (.pdf), PowerPoint (.pptx) or a video (.webm). None named: make a Word (.docx). Never switch formats without saying so.
2. Folder. Put everything under <project root>/docs/user-guides/<short-slug>/ (pictures in images/ inside it). Without a project folder, use the current working directory.
3. Open the site in the AgentDeck Preview: browser_open with the address (or browser_serve when it is this project and its dev server is not running). Check it with browser_screenshot.
4. Work through the task on the real page, one step at a time, with browser_elements, browser_click_element, browser_type, browser_key and browser_scroll. For each step:
   a. browser_annotate the control the user acts on: style "solid", the step number, and a short label (what to click or type). Mark a region the reader should look at with style "dashed". Use several marks when it helps.
   b. browser_screenshot with save_as = <folder>/images/step-NN.png.
   c. browser_annotate with clear true, then do the action and screenshot again to confirm it worked before the next step.
   Sign-in: use credentials the user gave or test values from the project. Never invent or guess real passwords. On a CAPTCHA, a code sent to a phone or an email, or missing credentials, stop and ask, and document the remaining steps from what the page shows.
   Never type real secrets into the document: write "your password" instead.
5. Build the file with the pictures embedded, a title, a short purpose line, numbered steps (action + expected result), and notes and troubleshooting at the end.
   - .docx: python-docx, or the docx npm package. .pptx: python-pptx, or pptxgenjs (one step per slide, picture large, caption under it). .pdf: build an HTML page with the pictures and print it with a headless browser (Playwright or Chrome --headless --print-to-pdf), or use reportlab.
   - Check which of these are available first. If none is, install one into a temporary folder or a virtual environment, not into the project's dependencies, and say what you installed.
   - Video: browser_record_start with the project root and a .webm name, do the steps (marks drawn with browser_annotate show in the recording; clear them after each step), then browser_record_stop.
6. Also write README.md in the same folder with the same steps as text and links to the pictures.
7. Verify: the file exists and opens (read it back with the same library, or count its pictures and pages). Report every output path, the number of steps and pictures, and anything you could not do.`;

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
    name: 'commit',
    description: 'Commit the changes and push them',
    kind: 'builtin',
    origin: 'AgentDeck',
    template:
      'Commit the changes in this project and push them. $ARGUMENTS\n\nRun git status and git diff to see exactly what changed. Stage only files that belong to this work: never a secret (.env, keys, tokens) or a generated file, and say so if you leave something out. Commit on the branch that is checked out, without creating one. Group the changes into one commit per purpose when they are unrelated. Write each message in the style of this repository (look at git log -5 first): one line that says what changed and why, no filler. Commit, then push the current branch (set the upstream if there is none). Never force-push, never skip hooks, never rewrite history that is already pushed. If a hook or the push fails, read the error, fix the cause and try again; do not give up at the first failure. End with the commit hashes, the branch, and where it was pushed.',
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
    name: 'security',
    description: 'Audit security across every language and app layer',
    kind: 'builtin',
    origin: 'AgentDeck',
    template:
      'Perform a read-only security audit of this application.\n\nFirst inventory every language, runtime, framework, build tool and deployment format actually present. Cover the whole repository, including mixed-language code, native modules, scripts, SQL and migrations, infrastructure/configuration, generated code where it affects runtime behavior, manifests and lockfiles. Do not skip a language because it is uncommon or lacks a preconfigured scanner. For each ecosystem, use its existing suitable static analyzer, linter, dependency auditor or security checks when available; state when a check is unavailable instead of implying coverage.\n\nTrace trust boundaries, entry points and sensitive data across layers. Check language-specific risks as relevant, including memory and concurrency safety, unsafe APIs, injection and output encoding, authentication and authorization, input validation, SSRF, path traversal, deserialization, file uploads, cryptography and secret handling. Also review desktop/mobile boundaries (IPC, filesystem/process access and update integrity) and DevOps/build/release configuration (exposed secrets, excessive CI permissions, unsafe workflows and insecure deployment defaults). Include dependency advisories only when a suitable audit command is already available.\n\nUse read-only inspection and safe, local checks already configured in the project. Do not attack external systems, access real secrets, install tools or dependencies, modify files, or claim a check passed unless you actually ran it. Report only actionable findings, ordered Critical / High / Medium / Low. For each include the severity, exact file:line evidence, a realistic attack path and impact, confidence, and a concrete remediation. Distinguish confirmed vulnerabilities from areas that need runtime verification. End with languages and layers inspected, checks actually run, and important coverage gaps. If no issue is confirmed, say so and summarize what you inspected.\n\nAudit focus: $ARGUMENTS (when provided)',
  },
  {
    name: 'release',
    description: 'Write notes, bump version, commit, push branch and tag for GitHub build',
    kind: 'builtin',
    origin: 'AgentDeck',
    template:
      'Prepare and publish a new release of this project. $ARGUMENTS\n\nStart by finding out how this repository releases: read the version files (package.json, Cargo.toml, tauri.conf.json, pyproject, etc.), the release notes or changelog folder and the latest notes as the format to copy, the CI workflows (a tag, a release event or a manual dispatch that builds and uploads assets), any npm/crates/PyPI publish step, and any docs about releasing or AGENTS.md rules. Follow what the project already does; do not invent a second process.\n\n1. Check the tree: git status, current branch, the last tag, and that nothing unrelated is uncommitted. If the tree is dirty with work that belongs in this release, say so before committing it.\n2. Decide the next version from the changes since the last tag (patch for fixes, minor for features, major for breaking changes) unless the arguments name one.\n3. Go through every commit since the last tag and sort them into what is new and what is fixed. Write the release notes in the project\'s own format and language: "What\'s new", then "Fixes" with what was wrong and what happens now, then any notes. Do not leave the fixes out.\n4. Bump the version in every file that carries it (use the project\'s script if it has one) and refresh the lockfile offline.\n5. Run the project\'s checks (type check, lint, tests) and report real results. Stop on a failure.\n6. Commit as "chore: release vX.Y.Z", create the tag, and push the branch and the tag. If CI builds on a tag, watch the run and report its result; if the project publishes to npm or another registry, or needs a zip/installer built locally, do that step only when the project documents it and the needed login already exists, and say what you did.\n\nPushing, tagging and publishing cannot be undone: show the version, the notes and the list of commits first, and wait for a yes unless the arguments already say to go ahead. Never force-push, move an existing tag, skip hooks, or publish with credentials you were not given. At the end give the version, the tag, the release link and anything left to do.',
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
    description: 'Write docs: a user guide with marked screenshots, or code docs',
    kind: 'builtin',
    origin: 'AgentDeck',
    template:
      'Write documentation for: $ARGUMENTS\n\nFirst decide which kind this is.\n- A user guide (how a person signs in, uses a screen, completes a task on a website or app, "hướng dẫn", "thao tác"): follow the guide workflow below, working the real site in the AgentDeck Preview yourself.\n- Developer documentation for code (an API, a module, setup): read the code first so every statement is true, match the tone and layout of the docs already here, and keep it short, with a runnable example where it helps.\n\n' +
      GUIDE_WORKFLOW,
  },
  {
    name: 'email',
    description: 'Write an email: clear, the right tone, ready to send',
    kind: 'builtin',
    origin: 'AgentDeck',
    template:
      'Write an email: $ARGUMENTS\n\nWork out who reads it, what they must know or do, and the tone that fits (a colleague, a client, a manager, support; Vietnamese or English as the request is written, unless it says otherwise). Then write it:\n- Subject line: specific, under 60 characters.\n- First sentence: the point or the request. No warm-up lines.\n- Body: short paragraphs or a short list; numbers, dates and names exact; one clear ask with a deadline when something is needed.\n- Close with a short sign-off, and leave [placeholders] for anything you do not know (a name, a date, an amount) instead of inventing it.\nGive the email in one block ready to copy, then one line with a shorter alternative subject. If the request leaves out something that changes the email (the reader, the goal), ask one short question first. Do not send anything.',
  },
  {
    name: 'layout',
    description: 'Suggest a UI layout that fits the project',
    kind: 'builtin',
    origin: 'AgentDeck',
    template:
      'Suggest a layout for: $ARGUMENTS\n\nFirst call the agentdeck tool project_context: use the project\'s own design tokens, components and rules, not a new look. Look at one or two existing screens of the app in the Preview (browser_open, browser_screenshot) so the new one belongs with them.\nThen give 2 or 3 layout options. For each: what the user does on the screen first, the structure as a small ASCII wireframe, which existing components it uses, how it behaves on a phone, and its trade-off. Avoid the usual AI look: no gradient hero, no card grid of equal boxes, no decorative icons in every heading, no centred everything; hierarchy comes from size, weight, spacing and alignment, with one accent colour. Recommend one option and say why in two lines. Do not write code until I pick one; when I do, build it, open it in the Preview, run ui_audit (desktop and with browser_device mobile) and run_checks, and fix what they report.',
  },
  {
    name: 'research',
    description: 'Research a question quickly, with sources',
    kind: 'builtin',
    origin: 'AgentDeck',
    template:
      'Research: $ARGUMENTS\n\nSearch the web (your web search tool, or open pages in the AgentDeck Preview with browser_open and read them with browser_text when you have none). Prefer primary sources: official docs, the project\'s repository and changelog, the vendor\'s own pricing page, papers. Check dates: say when a fact may have changed and prefer the newest source. Answer in this shape: the answer in 2-3 sentences; the key facts as a short list, each with its source link; where sources disagree, say so; what is still unknown. Keep it short. Do not change any file.',
  },
  {
    name: 'deep-research',
    description: 'Deep research: multiple AIs, sources cross-checked, written up',
    kind: 'builtin',
    origin: 'AgentDeck',
    template:
      'Deep research: $ARGUMENTS\n\n1. Plan: split the question into 3-6 independent sub-questions and list them briefly.\n2. Use multiple AIs by default. Call list_agents, then delegate independent research briefs to at least 3 available agents from different providers when possible, all in one round with delegate_task (wait:false). Assign distinct sub-questions or perspectives; ask each to search several queries and sources, open the best sources in full, and return claims with direct links, publisher and publication/update dates. Keep each brief self-contained. Send only public research questions to FREE TIER agents; never include private user/project data. If fewer than 3 providers are available or delegation is unavailable, use every suitable agent/tool available, continue the remaining research yourself, and state the limitation.\n3. While agents work, research the question yourself using several different queries and source types. Prefer primary sources (official docs, records, papers, vendor/project pages); use reputable independent sources to corroborate. Do not rely on search snippets alone.\n4. Compare the reports. Verify important claims against at least two independent sources, identify duplicate or weak sources, resolve conflicting dates/versions/regions, and mark claims that remain uncertain. Do not treat several AIs repeating one source as independent confirmation.\n5. Write one Markdown report at docs/research/<short-slug>.md in the current project (or the working folder): a five-line summary, findings by sub-question, a comparison table when useful, recommendation with reasons, open questions, and a numbered source list with direct links and dates. Keep the report grounded in sources actually opened. Do not change any other file.\nEnd your reply with the summary, recommendation, report path, number of agents used, and any research limits.',
  },
  {
    name: 'ui-review',
    description: 'Audit the UI in the Preview and fix what is wrong',
    kind: 'builtin',
    origin: 'AgentDeck',
    template:
      'Review the UI of: $ARGUMENTS\n\nCall project_context first to know the design tokens and components. Open the screen in the Preview (browser_serve or browser_open), then for desktop and for browser_device mobile: browser_screenshot, ui_audit, browser_console. List the problems by severity (broken layout, unreadable text, missing names for controls, off-token colours and sizes, inconsistent spacing), each with its selector and the file that renders it (use code_search). Fix them in the code with the project\'s tokens and components, reload, and run ui_audit and run_checks again until they are clean. End with before/after of what changed.',
  },
  {
    name: 'document',
    description: 'Create a Word, PDF, PowerPoint, or recorded video guide',
    kind: 'builtin',
    origin: 'AgentDeck',
    template:
      'Create this user guide: $ARGUMENTS\n\nWhen the workflow belongs to this project, read its source first so the steps are accurate. Use browser_zoom for a close-up of a small control, and set it back to 1 before the next step. Do not claim a file was created unless you saw it on disk.\n\n' +
      GUIDE_WORKFLOW,
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
