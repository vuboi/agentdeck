// The memory of a project: a small markdown file (`.agentdeck/memory.md`) shared by every agent that works in it, and a
// proposals file (`.agentdeck/proposals.md`) where agents suggest better ways that the user accepts or drops. This file
// builds the note an agent gets at the start of a conversation, and reads / writes the files for the Settings page.
// Pure apart from the invoke wrappers, so node can test the note.

export const MEMORY_FILE = '.agentdeck/memory.md';
export const PROPOSALS_FILE = '.agentdeck/proposals.md';
/** How much of the memory goes into the prompt: the rest stays in the file for the agent to open when it needs it. */
export const MEMORY_PROMPT_CHARS = 3000;

/** The memory file of a project folder, as the agents are told to open it (forward slashes work on every system). */
export const memoryPath = (root: string) => `${root.replace(/[\\/]+$/, '').replace(/\\/g, '/')}/${MEMORY_FILE}`;
export const proposalsPath = (root: string) => `${root.replace(/[\\/]+$/, '').replace(/\\/g, '/')}/${PROPOSALS_FILE}`;

/** The note for the start of a conversation, or '' when memory is off or there is no project. */
export function memoryHint(on: boolean, root: string | null, content: string): string {
  if (!on || !root) return '';
  const text = content.trim();
  const shown = text.length > MEMORY_PROMPT_CHARS ? `${text.slice(0, MEMORY_PROMPT_CHARS)}\n… (cut: open the file for the rest)` : text;
  return (
    `Project memory, shared by every agent in this project, kept in ${memoryPath(root)}. ` +
    (shown ? `Read it as background:\n${shown}\n` : 'It is empty so far. ') +
    'When you learn something durable and not obvious from the code (a convention, a command that works, a pitfall, a decision and why), ' +
    'add one short bullet to that file; fix a bullet that turned out wrong. Keep it under 40 lines; no secrets, no notes about the current task. ' +
    `If you find a clearly better way to work here, do not change the memory: append one bullet to ${proposalsPath(root)} with the change and why. ` +
    'The user decides whether it is kept.'
  );
}

/** Each proposal is a bullet starting with `- `; its indented lines belong to it. */
function bullets(text: string): string[] {
  const items: string[] = [];
  for (const line of text.split('\n')) {
    if (line.startsWith('- ')) items.push(line);
    else if (items.length && line.startsWith('  ')) items[items.length - 1] += `\n${line}`;
  }
  return items;
}

/** The proposals waiting for the user, as text. */
export function parseProposals(text: string): string[] {
  return bullets(text).map((item) => item.slice(2).trim());
}

/** The proposals file without the proposal at `index`. */
export function dropProposal(text: string, index: number): string {
  const kept = bullets(text).filter((_, position) => position !== index);
  return kept.length ? `${kept.join('\n')}\n` : '';
}

export async function readProjectMemory(root: string): Promise<string> {
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    return await invoke<string>('project_memory_read', { root });
  } catch {
    return '';
  }
}

export async function writeProjectMemory(root: string, text: string): Promise<void> {
  const { invoke } = await import('@tauri-apps/api/core');
  await invoke('project_memory_write', { root, text });
}

export async function readProposals(root: string): Promise<string> {
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    return await invoke<string>('project_proposals_read', { root });
  } catch {
    return '';
  }
}

export async function writeProposals(root: string, text: string): Promise<void> {
  const { invoke } = await import('@tauri-apps/api/core');
  await invoke('project_proposals_write', { root, text });
}
