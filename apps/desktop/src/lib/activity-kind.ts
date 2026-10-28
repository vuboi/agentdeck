// What kind of step an Audit entry is: an icon, a plain label and a colour, from the tool's own name. The log keeps the
// name the CLI used (Write, Bash, mcp__server__tool, a Codex `server.tool`); this is only how a row reads.
// Free of stores and Tauri so plain node can test it. The names of each agent's tools match tool-kind.ts, the table the
// chat's steps use for their icons (this file cannot import it: node runs it with the extension, svelte-check without).

export type ActivityKind = { id: string; label: string; icon: string; tone: string };

const kind = (id: string, label: string, icon: string, tone: string): ActivityKind => ({ id, label, icon, tone });

const THINKING = kind('thinking', 'Thinking', 'lucideLightbulb', 'text-violet-500');
const CREATE = kind('create', 'Create file', 'lucideFilePlus', 'text-emerald-500');
const EDIT = kind('edit', 'Edit file', 'lucideFilePen', 'text-sky-500');
const DELETE = kind('delete', 'Delete file', 'lucideFileX', 'text-destructive');
const READ = kind('read', 'Read file', 'lucideFileText', 'text-muted-foreground');
const COMMAND = kind('command', 'Run command', 'lucideTerminal', 'text-amber-500');
const SEARCH = kind('search', 'Search code', 'lucideSearch', 'text-muted-foreground');
const WEB = kind('web', 'Web search', 'lucideGlobe', 'text-blue-500');
const FETCH = kind('fetch', 'Fetch page', 'lucideDownload', 'text-blue-500');
const SKILL = kind('skill', 'Use skill', 'lucideSparkles', 'text-fuchsia-500');
const MCP = kind('mcp', 'Use MCP', 'lucidePlug', 'text-teal-500');
const AGENT = kind('agent', 'Agent', 'lucideBot', 'text-orange-500');
const PLAN = kind('plan', 'Plan', 'lucideListChecks', 'text-muted-foreground');
const ERROR = kind('error', 'Chat error', 'lucideTriangleAlert', 'text-destructive');
const TALK = kind('talk', 'Discussion', 'lucideMessagesSquare', 'text-muted-foreground');
const RAW = kind('raw', 'Other step', 'lucideCode', 'text-muted-foreground');
const REQUEST = kind('request', 'Request', 'lucideSend', 'text-primary');
const OTHER = kind('other', '', 'lucideWrench', 'text-muted-foreground');

/** The kind of a step, from its tool name. An unknown tool keeps its own name as the label. */
export function kindOf(name: string): ActivityKind {
  const tool = name.trim();
  if (tool === 'Thinking') return THINKING;
  if (tool === 'Chat error') return ERROR;
  if (tool === 'Discussion') return TALK;
  if (tool === 'Request') return REQUEST;
  if (/^(Write|write_file|create(_file)?)$/i.test(tool)) return CREATE;
  if (/^(Edit|MultiEdit|NotebookEdit|apply_patch|str_replace\w*|replace|edit_file|insert)$/i.test(tool)) return EDIT;
  if (/^Delete$/i.test(tool)) return DELETE;
  if (/^(Read|view|read_file|read_many_files|open_file|cat)$/i.test(tool)) return READ;
  if (/^(Bash|BashOutput|KillShell|Terminal|Run project|shell|run_command|exec_command|powershell|pwsh|cmd|run_in_terminal|run_shell_command|(read|write)_(bash|powershell))$/i.test(tool)) return COMMAND;
  if (/^(Grep|Glob|LS|code_\w+|find|find_files|search|search_files|list_dir(ectory)?|file_search|codebase_search)$/i.test(tool)) return SEARCH;
  if (/^(WebSearch|web_search|google_web_search)$/i.test(tool)) return WEB;
  if (/^(WebFetch|web_fetch|fetch|fetch_url)$/i.test(tool)) return FETCH;
  if (/^Skill$/i.test(tool)) return SKILL;
  if (/^(Task|Agent|delegate_task|list_agents|task_status|task_stop)$/i.test(tool)) return AGENT;
  if (/^(TodoWrite|update_plan|ExitPlanMode)$/i.test(tool)) return PLAN;
  // An MCP tool: Claude writes `mcp__server__tool`, Codex `server.tool`, and the app's own browser tools are MCP too.
  if (/^mcp__/i.test(tool) || /^[\w-]+\.[\w.-]+$/.test(tool) || /^browser_\w+$/.test(tool)) return MCP;
  if (tool.includes(' · ')) return RAW;
  return { ...OTHER, label: tool || 'Step' };
}

/** What a row shows after the kind's label: the MCP server and tool, or the raw tool name when it says more. */
export function detailOf(name: string): string {
  const mcp = /^mcp__([^_].*?)__(.+)$/i.exec(name);
  if (mcp) return `${mcp[1]} · ${mcp[2]}`;
  if (kindOf(name).id === 'mcp') return name.replace('.', ' · ');
  return '';
}
