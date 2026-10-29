import { toast } from '../../app/platform/toast';
import { readFileBytes } from '../../chat/agent';
import { editMessage } from '../../chat/sessions.svelte';
import type { Message } from '../../chat/mock';

// Editing one of your messages in place (ChatGPT-style): Send replaces it and what came after it.
// Kept outside the row component: the virtual list unmounts rows that scroll away, the edit must survive that.
export const edit = $state<{ id: string | null; text: string; loading: boolean; files: string[] }>({ id: null, text: '', loading: false, files: [] });

export async function startEdit(message: Message) {
  const text = message.blocks.map((block) => (block.kind === 'response' ? block.text : '')).join('\n').trim();
  const files = message.blocks.filter((block) => block.kind === 'file');
  const hasOtherAttachments = message.blocks.some((block) => block.kind === 'image' || block.kind === 'element');
  const markdown = files.length === 1 && !hasOtherAttachments && !text && /\.(md|markdown|mdown|mkd)$/i.test(files[0].name)
    ? files[0]
    : undefined;
  edit.text = text;
  edit.files = [];
  edit.id = message.id;
  if (markdown) {
    edit.loading = true;
    try {
      edit.text = new TextDecoder().decode(await readFileBytes(markdown.path));
      edit.files = [markdown.path];
    } catch {
      toast.error('Could not read Markdown file');
    } finally {
      edit.loading = false;
    }
  }
}

export function cancelEdit() {
  edit.id = null;
  edit.files = [];
}

export function finishEdit(id: string) {
  if (!edit.text.trim() || edit.loading) return;
  editMessage(id, edit.text, edit.files);
  window.dispatchEvent(new Event('agentdeck:scrolltoend'));
  edit.id = null;
  edit.files = [];
}

export function resizeEditTextarea(node: HTMLTextAreaElement) {
  const markdown = edit.files.length > 0;
  const minHeight = markdown ? 160 : 0;
  const maxHeight = markdown ? 420 : 240;
  node.style.height = 'auto';
  node.style.height = `${Math.max(minHeight, Math.min(node.scrollHeight, maxHeight))}px`;
}
