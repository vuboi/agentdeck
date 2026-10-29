<script lang="ts">
  import { fade, fly } from 'svelte/transition';
  import { ZButton } from '@shival99/z-ui-svelte';
  import { toast } from '../../app/platform/toast';
  import { copyMessage } from '../../chat/content/attachments';
  import { attachmentsOf, forkSession, workspace } from '../../chat/sessions.svelte';
  import { theme } from '../../app/ui/theme.svelte';
  import { openAttachment, openImages } from '../../browser/preview.svelte';
  import FileChip from './FileChip.svelte';
  import ChatBlock from './ChatBlock.svelte';
  import MessageFooter from './MessageFooter.svelte';
  import { edit, startEdit, cancelEdit, finishEdit, resizeEditTextarea } from './chat-edit.svelte';
  import type { Message } from '../../chat/mock';

  let { message }: { message: Message } = $props();

  // "Copy all" on a message with attachments: its text, pictures, notes and files go on the clipboard together, and
  // pasting into the message box restores them all (see attachments.ts).
  async function copyAll(message: Message) {
    const { images, elements, files } = attachmentsOf(message);
    const text = message.blocks.map((block) => (block.kind === 'response' ? block.text : '')).join('\n').trim();
    try {
      await copyMessage({ v: 1, text, images, elements, files });
      toast.success('Copied with attachments');
    } catch {
      toast.error('Could not copy');
    }
  }

  const pictures = $derived(message.blocks.flatMap((block) => (block.kind === 'image' ? [block] : [])));
  const words = $derived(message.blocks.filter((block) => block.kind !== 'image' && block.kind !== 'delegate' && block.kind !== 'file'));
  const documents = $derived(message.blocks.flatMap((block) => (block.kind === 'file' ? [block] : [])));
</script>

<!-- Pictures sit apart from the bubble as small tiles; click one to see it large. -->
{#if pictures.length > 0}
  <div class="mb-1.5 flex max-w-[85%] flex-wrap gap-1.5 {theme.chatLayout === 'split' ? 'justify-end' : ''}">
    {#each pictures as picture, index}
      <button
        type="button"
        class="size-[4.5rem] cursor-zoom-in overflow-hidden rounded-xl border border-border bg-muted outline-none transition hover:ring-2 hover:ring-primary/50 focus-visible:ring-2 focus-visible:ring-ring"
        aria-label="Preview {picture.name}"
        onclick={() => openImages(pictures, index)}
      >
        <img src={picture.src} alt={picture.name} class="size-full object-cover" />
      </button>
    {/each}
  </div>
{/if}
{#if documents.length > 0}
  <div class="mb-1.5 flex max-w-[85%] flex-wrap gap-1.5 {theme.chatLayout === 'split' ? 'justify-end' : ''}">
    {#each documents as file}
      <FileChip name={file.name} path={file.path} size={file.size} onclick={() => void openAttachment(file.path, file.name, file.size)} />
    {/each}
  </div>
{/if}
{#if edit.id === message.id}
  <!-- Editing: the message becomes a box; Send replaces it and everything after it. The text runs into the
       box's right padding, so a long message's scrollbar sits flush with the box's edge. -->
  <div in:fly={{ y: 4, duration: 220 }} class="flex w-full max-w-[85%] flex-col gap-2 rounded-2xl bg-muted px-4 pt-3 pb-2.5 text-sm">
    {#each message.blocks.filter((block) => block.kind === 'element') as attached, at (at)}
      <ChatBlock block={attached} />
    {/each}
    <textarea
      bind:value={edit.text}
      {@attach (node: HTMLTextAreaElement) => {
        node.focus();
        node.setSelectionRange(node.value.length, node.value.length);
        resizeEditTextarea(node);
      }}
      oninput={(event) => {
        resizeEditTextarea(event.currentTarget);
      }}
      onkeydown={(event) => {
        if (event.key === 'Escape') cancelEdit();
        if (event.key === 'Enter' && !event.shiftKey && !event.isComposing) {
          event.preventDefault();
          finishEdit(message.id);
        }
      }}
      aria-label="Edit message"
      rows="1"
      class="-mr-4 w-[calc(100%+1rem)] resize-none border-0 bg-transparent p-0 pr-3 text-sm leading-relaxed outline-none {edit.files.length ? 'min-h-40 overflow-y-auto' : ''}"
    ></textarea>
    <div class="flex justify-end gap-2">
      <ZButton zWave={false} zType="outline" zSize="sm" class="h-7 px-3 text-sm" onclick={cancelEdit}>Cancel</ZButton>
      <ZButton zWave={false} zSize="sm" class="h-7 px-3 text-sm" disabled={!edit.text.trim() || edit.loading} onclick={() => finishEdit(message.id)}>Send</ZButton>
    </div>
  </div>
{:else}
  {#if words.length > 0}
    <div in:fade={{ duration: 220 }} class="flex max-w-[85%] flex-col gap-2 rounded-2xl bg-muted p-3 text-sm {theme.chatLayout === 'split' ? 'rounded-br-md' : 'rounded-bl-md'}">
      {#each words as block}
        <ChatBlock {block} fade="var(--muted)" />
      {/each}
    </div>
  {/if}
  <MessageFooter {message} end={theme.chatLayout === 'split'} onfork={() => forkSession(workspace.activeId)} onedit={() => startEdit(message)} oncopyall={pictures.length + documents.length + message.blocks.filter((block) => block.kind === 'element').length > 0 ? () => copyAll(message) : undefined} />
{/if}
