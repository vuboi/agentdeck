/** A text position in the plain-text view of the composer editor. Mention spans count as their serialized token. */
export type EditorSelection = { start: number; end: number };

export function editorMentions(root: HTMLElement): string[] {
  return [...root.querySelectorAll<HTMLElement>('[data-mention-token]')].map((node) => node.dataset.mentionToken ?? '');
}

export function renderEditor(root: HTMLElement, value: string, mentions: { kind: string; id: string; label: string }[]): void {
  if (editorText(root) === value && editorMentions(root).length === mentions.length) return;
  const caret = editorSelection(root).start;
  const tokens = mentions.map((mention) => ({ mention, token: mention.kind === 'workflow' ? `/w ${mention.label}` : `@${mention.label}` })).filter((item) => item.token && value.includes(item.token)).sort((a, b) => b.token.length - a.token.length);
  root.replaceChildren();
  let cursor = 0;
  while (cursor < value.length) {
    const match = tokens.map((item) => ({ ...item, at: value.indexOf(item.token, cursor) })).filter((item) => item.at >= cursor).sort((a, b) => a.at - b.at)[0];
    if (!match) { root.append(document.createTextNode(value.slice(cursor))); break; }
    if (match.at > cursor) root.append(document.createTextNode(value.slice(cursor, match.at)));
    const token = document.createElement('span');
    token.contentEditable = 'false';
    token.dataset.mentionToken = match.token;
    token.dataset.mentionId = match.mention.id;
    token.dataset.mentionKind = match.mention.kind;
    token.className = 'inline-flex translate-y-[-1px] items-center gap-1 rounded-sm bg-primary/15 px-1 text-primary';
    const marker = document.createElement('span');
    marker.setAttribute('aria-hidden', 'true');
    marker.className = 'inline-grid size-2.5 shrink-0 place-items-center text-[9px] leading-none';
    marker.textContent = match.mention.kind === 'ai' ? '✳' : match.mention.kind === 'workflow' ? '⌘' : match.mention.kind === 'session' ? '▣' : '▰';
    token.append(marker, document.createTextNode(match.token));
    root.append(token);
    cursor = match.at + match.token.length;
  }
  setEditorSelection(root, caret);
}

const tokenOf = (node: Node): string | null =>
  node instanceof HTMLElement ? node.dataset.mentionToken ?? null : null;

const textValue = (node: Node): string => {
  if (tokenOf(node) !== null) return tokenOf(node) ?? '';
  if (node.nodeType === Node.TEXT_NODE) return (node.nodeValue ?? '').replaceAll('\u200b', '');
  if (node instanceof HTMLBRElement) return '\n';
  return [...node.childNodes].map(textValue).join('');
};

/** Reads the editable surface as the exact plain text sent to an agent. */
export const editorText = (root: HTMLElement): string => textValue(root);

const lengthOf = (node: Node): number => textValue(node).length;

function pointAt(root: HTMLElement, wanted: number, preferAfterToken: boolean): { node: Node; offset: number } {
  let cursor = 0;
  let found: { node: Node; offset: number } | undefined;
  const visit = (node: Node): boolean => {
    const token = tokenOf(node);
    if (token !== null) {
      const end = cursor + token.length;
      if (wanted <= end) {
        const parent = node.parentNode ?? root;
        const index = [...parent.childNodes].indexOf(node as ChildNode);
        found = { node: parent, offset: wanted <= cursor || !preferAfterToken ? index : index + 1 };
        return true;
      }
      cursor = end;
      return false;
    }
    if (node.nodeType === Node.TEXT_NODE) {
      const value = node.nodeValue ?? '';
      const plain = value.replaceAll('\u200b', '');
      if (wanted <= cursor + plain.length) {
        const rawOffset = wanted - cursor;
        found = { node, offset: rawOffset };
        return true;
      }
      cursor += plain.length;
      return false;
    }
    if (node instanceof HTMLBRElement) {
      if (wanted <= cursor + 1) {
        const parent = node.parentNode ?? root;
        const index = [...parent.childNodes].indexOf(node);
        found = { node: parent, offset: wanted <= cursor ? index : index + 1 };
        return true;
      }
      cursor += 1;
      return false;
    }
    for (const child of [...node.childNodes]) if (visit(child)) return true;
    return false;
  };
  visit(root);
  return found ?? { node: root, offset: root.childNodes.length };
}

/** Reads a browser selection as offsets in the serialized plain-text value. */
export function editorSelection(root: HTMLElement): EditorSelection {
  const selection = window.getSelection();
  const range = selection?.rangeCount ? selection.getRangeAt(0) : null;
  if (!range || !root.contains(range.startContainer) || !root.contains(range.endContainer)) {
    const end = editorText(root).length;
    return { start: end, end };
  }
  return {
    start: offsetAt(root, range.startContainer, range.startOffset),
    end: offsetAt(root, range.endContainer, range.endOffset),
  };
}

function offsetAt(root: HTMLElement, container: Node, offset: number): number {
  let length = 0;
  let found = false;
  const visit = (node: Node): boolean => {
    if (node === container) {
      if (node.nodeType === Node.TEXT_NODE) length += (node.nodeValue ?? '').slice(0, offset).replaceAll('\u200b', '').length;
      else for (const child of [...node.childNodes].slice(0, offset)) length += lengthOf(child);
      return (found = true);
    }
    const token = tokenOf(node);
    if (token !== null) {
      length += token.length;
      return false;
    }
    if (node.nodeType === Node.TEXT_NODE) {
      length += (node.nodeValue ?? '').replaceAll('\u200b', '').length;
      return false;
    }
    if (node instanceof HTMLBRElement) {
      length += 1;
      return false;
    }
    for (const child of [...node.childNodes]) if (visit(child)) return true;
    return false;
  };
  visit(root);
  return found ? length : editorText(root).length;
}

/** Places the caret/selection by plain-text offsets, snapping positions inside an atomic mention to its edge. */
export function setEditorSelection(root: HTMLElement, start: number, end = start): void {
  const selection = window.getSelection();
  if (!selection) return;
  const low = Math.max(0, Math.min(start, editorText(root).length));
  const high = Math.max(low, Math.min(end, editorText(root).length));
  const from = pointAt(root, low, low === high);
  const to = pointAt(root, high, true);
  const range = document.createRange();
  range.setStart(from.node, from.offset);
  range.setEnd(to.node, to.offset);
  selection.removeAllRanges();
  selection.addRange(range);
}

/** Replaces a plain-text selection in the editor, deleting whole atomic mention spans when selected. */
export function replaceEditorRange(root: HTMLElement, start: number, end: number, insert: string): void {
  const from = pointAt(root, start, start === end);
  const to = pointAt(root, end, true);
  const range = document.createRange();
  range.setStart(from.node, from.offset);
  range.setEnd(to.node, to.offset);
  range.deleteContents();
  if (insert) {
    const text = document.createTextNode(insert);
    range.insertNode(text);
    range.setStart(text, text.length);
    range.collapse(true);
    const selection = window.getSelection();
    selection?.removeAllRanges();
    selection?.addRange(range);
  } else setEditorSelection(root, start, start);
}
