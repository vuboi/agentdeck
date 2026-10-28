// The memory of a project: a small markdown file (`.agentdeck/memory.md`) shared by every agent that works in it. This
// file builds the note an agent gets at the start of a conversation, and reads / writes the file for the Settings page.
// Pure apart from the two invoke wrappers, so node can test the note.

export const MEMORY_FILE = '.agentdeck/memory.md';
/** How much of the memory goes into the prompt: the rest stays in the file for the agent to open when it needs it. */
export const MEMORY_PROMPT_CHARS = 3000;

/** The memory file of a project folder, as the agents are told to open it (forward slashes work on every system). */
export const memoryPath = (root: string) => `${root.replace(/[\\/]+$/, '').replace(/\\/g, '/')}/${MEMORY_FILE}`;

/** The note for the start of a conversation, or '' when memory is off or there is no project. */
export function memoryHint(on: boolean, root: string | null, content: string): string {
  if (!on || !root) return '';
  const text = content.trim();
  const shown = text.length > MEMORY_PROMPT_CHARS ? `${text.slice(0, MEMORY_PROMPT_CHARS)}\n… (cut: open the file for the rest)` : text;
  return (
    `Project memory, shared by every agent in this project, kept in ${memoryPath(root)}. ` +
    (shown ? `Read it as background:\n${shown}\n` : 'It is empty so far. ') +
    'When you learn something durable and not obvious from the code (a convention, a command that works, a pitfall, a decision and why), ' +
    'add one short bullet to that file; fix a bullet that turned out wrong. Keep it under 40 lines; no secrets, no notes about the current task.'
  );
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
