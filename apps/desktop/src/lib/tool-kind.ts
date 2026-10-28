// What kind of work a tool call is, from the tool's name, and the icon that goes with it. One table for every place that
// shows tool calls (the chat's steps, the Audit log), so a step has the same icon wherever it is seen.
// Free of stores and Tauri so plain node can test it.

// Tool names as each agent sends them: Claude (`Bash`, `Read`), Codex (`shell`, `exec_command`), Copilot (`powershell`,
// `view`, `create`, `str_replace`), OpenCode and Gemini (`read_file`, `write_file`, `run_shell_command`).
export const RUN = /^(bash|shell|exec_command|powershell|pwsh|cmd|run_in_terminal|run_shell_command|run_command|terminal|(read|write)_(bash|powershell))$/i;
export const EDIT = /^(Edit|MultiEdit|Write|NotebookEdit|apply_patch|str_replace(_editor|_based_edit_tool)?|replace|write_file|create(_file)?|edit_file|insert)$/i;
/** The edit tools that make a new file. */
export const CREATE = /^(Write|write_file|create(_file)?)$/i;
export const READ = /^(Read|view|read_file|read_many_files|open_file|cat)$/i;
export const SEARCH = /^(Grep|Glob|LS|list_dir(ectory)?|file_search|codebase_search|search_files|find_files|find|search|WebSearch|web_search|google_web_search)$/i;
export const WEB_SEARCH = /^(WebSearch|web_search|google_web_search)$/i;
export const FETCH = /^(WebFetch|Fetch|web_fetch|fetch_url)$/i;
export const AGENT = /^(Task|Agent)$/;
// The AgentDeck browser tools: what the agent did on the Preview page.
export const PAGE_ACT = /^browser_(click|type|key|scroll|hover|fill|drag|upload)$/;
export const PAGE_SCRIPT = /^browser_eval$/;
export const PAGE_LOOK = /^browser_screenshot$/;
export const PAGE_READ = /^browser_(dom|console|network|frames|page)$/;
export const PLAN = /^(Plan|TodoWrite|update_plan|ExitPlanMode)$/;
export const DELEGATE = /^delegate_task$/;
/** The agent asked the user something: its own `AskUserQuestion`, Codex's `request_user_input`, or the app's question block. */
export const ASK = /^(AskUser|AskUserQuestion|ask_user|ask_user_question|request_user_input)$/i;
export const TEAM = /^(list_agents|task_status|task_stop)$/;

export const ICON = {
  edit: 'lucideFilePen',
  create: 'lucideFilePlus',
  delete: 'lucideTrash2',
  run: 'lucideTerminal',
  read: 'lucideFileText',
  search: 'lucideSearch',
  web: 'lucideGlobe',
  agent: 'lucideBot',
  page: 'lucideMonitor',
  click: 'lucideMousePointerClick',
  plan: 'lucideListChecks',
  team: 'lucideUsers',
  mention: 'lucideAtSign',
  skill: 'lucideSparkles',
  code: 'lucideCode',
  ask: 'lucideMessageCircleQuestion',
  tool: 'lucidePlug',
} as const;

/** The tool's own name: without the MCP server (`mcp__agentdeck__browser_click`, Codex `agentdeck.browser_click`). */
export const bareTool = (name: string) => name.trim().replace(/^mcp__.*?__/i, '').split('.').at(-1) || name.trim();

/** The icon of a tool call by its name. Any tool not named here (an MCP tool of some server) is a plug. */
export function toolIcon(name: string): string {
  const tool = bareTool(name);
  if (/^Delete$/i.test(tool)) return ICON.delete;
  if (RUN.test(tool) || /^(Bash|BashOutput|KillShell|Run project)$/i.test(tool)) return ICON.run;
  if (CREATE.test(tool)) return ICON.create;
  if (EDIT.test(tool)) return ICON.edit;
  if (READ.test(tool)) return ICON.read;
  if (WEB_SEARCH.test(tool) || FETCH.test(tool)) return ICON.web;
  if (SEARCH.test(tool)) return ICON.search;
  if (AGENT.test(tool)) return ICON.agent;
  if (PAGE_ACT.test(tool)) return ICON.click;
  if (/^browser_/.test(tool)) return ICON.page;
  if (PLAN.test(tool)) return ICON.plan;
  if (ASK.test(tool)) return ICON.ask;
  if (DELEGATE.test(tool)) return ICON.mention;
  if (TEAM.test(tool)) return ICON.team;
  if (/skill/i.test(tool)) return ICON.skill;
  if (/^code_/.test(tool)) return ICON.code;
  return ICON.tool;
}

/** The command a shell step ran, without the shell around it: `"pwsh.exe" -Command "Get-Content 'x'"` is `Get-Content 'x'`. */
export function innerCommand(text: string): string {
  const inner = /^\S*(?:pwsh|powershell)(?:\.exe)?["']?\s.*?-Command\s+([\s\S]*)$/i.exec(text)?.[1];
  // Only the wrapper's own quotes come off; a plain `echo 'hi'` keeps its quotes.
  return inner === undefined ? text.trim() : inner.trim().replace(/^"([\s\S]*)"$/, '$1');
}

/** What an ask step is about: the first question, from the call's JSON (`{"questions":[{"question":"..."}]}`) or the plain text. */
export function askSubject(raw: string): string {
  if (!raw.startsWith('{')) return raw;
  try {
    const args = JSON.parse(raw) as { questions?: { question?: unknown }[]; question?: unknown };
    const first = args.questions?.[0]?.question ?? args.question;
    return typeof first === 'string' ? first.trim() : '';
  } catch {
    return '';
  }
}

/** The main argument of a tool call: from its JSON (`{"command":"load_skill word"}` is `load_skill word`), else the text as
 *  it is. Empty while the JSON is still streaming in. */
export function detailOf(raw: string): string {
  if (!raw.startsWith('{')) return raw;
  try {
    const args = JSON.parse(raw) as Record<string, unknown>;
    const value = ['command', 'query', 'url', 'path', 'file_path', 'pattern', 'name', 'action'].map((key) => args[key]).find((item) => typeof item === 'string' && item.trim());
    return typeof value === 'string' ? value.trim() : '';
  } catch {
    return '';
  }
}
