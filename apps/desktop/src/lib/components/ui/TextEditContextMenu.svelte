<script lang="ts">
  import type { ZDropdownMenuItem } from '@shival99/z-ui-svelte';
  import Menu from './Menu.svelte';
  import { toast } from '../../app/platform/toast';

  let {
    onPasteText,
    onPasteClipboard,
  }: {
    onPasteText?: (text: string) => boolean;
    onPasteClipboard?: (payload: { html: string; text: string; files: File[] }) => Promise<boolean>;
  } = $props();
  let open = $state(false);
  let at = $state({ x: 0, y: 0 });
  let target = $state<HTMLTextAreaElement | null>(null);
  let selection = $state({ start: 0, end: 0 });
  const mac = typeof navigator !== 'undefined' && /mac/i.test(navigator.platform);
  const modifier = mac ? '⌘' : 'Ctrl+';
  const rowHeight = 28;
  const ruleHeight = 9;
  const menuPadding = 8;
  const popoverChrome = 23; // picker chrome plus the one-pixel context anchor

  /** Open the shared edit menu for a textarea and remember its selection before the menu takes focus. */
  export function openAt(event: MouseEvent, textarea: HTMLTextAreaElement): void {
    event.preventDefault();
    target = textarea;
    selection = { start: textarea.selectionStart, end: textarea.selectionEnd };
    const rules = items.filter((item) => item.divide === 'before').length;
    const menuHeight = items.length * rowHeight + rules * ruleHeight + menuPadding;
    const roomNeeded = menuHeight + popoverChrome;
    at = {
      x: Math.min(event.clientX, Math.max(6, window.innerWidth - 140)),
      y: Math.min(event.clientY, Math.max(6, window.innerHeight - roomNeeded)),
    };
    open = true;
  }

  function selectedText(): string {
    return target?.value.slice(selection.start, selection.end) ?? '';
  }

  function replaceSelection(text: string, inputType: string): void {
    if (!target) return;
    const start = Math.min(selection.start, target.value.length);
    const end = Math.min(selection.end, target.value.length);
    target.focus();
    target.setRangeText(text, start, end, 'end');
    target.dispatchEvent(new InputEvent('input', { bubbles: true, inputType, data: text || null }));
    selection = { start: target.selectionStart, end: target.selectionEnd };
  }

  async function copySelection(cut: boolean): Promise<void> {
    const text = selectedText();
    if (!text) return;
    try {
      await navigator.clipboard.writeText(text);
      if (cut) replaceSelection('', 'deleteByCut');
    } catch {
      toast.error('Could not access the clipboard');
    }
  }

  async function paste(): Promise<void> {
    if (!target) return;
    try {
      let html = '';
      let text = '';
      const files: File[] = [];
      try {
        for (const item of await navigator.clipboard.read()) {
          for (const type of item.types) {
            const blob = await item.getType(type);
            if (type === 'text/html') html ||= await blob.text();
            else if (type === 'text/plain') text ||= await blob.text();
            else if (type.startsWith('image/')) {
              const ext = type.split('/')[1]?.replace('jpeg', 'jpg') || 'png';
              files.push(new File([blob], `Pasted image.${ext}`, { type }));
            }
          }
        }
      } catch {
        // Older WebViews may expose only readText; keep the normal text-paste path below.
      }
      if (await onPasteClipboard?.({ html, text, files })) return;
      if (!text) text = await navigator.clipboard.readText();
      if (onPasteText?.(text)) return;
      replaceSelection(text, 'insertFromPaste');
    } catch {
      toast.error('Could not read the clipboard');
    }
  }

  const items = $derived<ZDropdownMenuItem[]>([
    { label: 'Cut', shortcut: `${modifier}X`, disabled: !selectedText(), class: 'text-xs! py-1', onClick: () => void copySelection(true) },
    { label: 'Copy', shortcut: `${modifier}C`, disabled: !selectedText(), class: 'text-xs! py-1', onClick: () => void copySelection(false) },
    { label: 'Paste', shortcut: `${modifier}V`, divide: 'before', class: 'text-xs! py-1', onClick: () => void paste() },
    { label: 'Select all', shortcut: `${modifier}A`, divide: 'before', disabled: !target?.value, class: 'text-xs! py-1', onClick: () => target?.select() },
  ]);
</script>

<Menu {items} bind:open {at} position="bottom-left" width={140} fitContent label="Text editing menu" />
