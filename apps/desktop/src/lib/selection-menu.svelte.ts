// The right-click menu of chat text: copy what is selected, or the whole message (as text or as Markdown).
export const selectionMenu = $state({ open: false, x: 0, y: 0, selected: '', plain: '', markdown: '' });

/** Opens the menu for a right click inside a message. Links, fields and the like keep their own menus. */
export function openSelectionMenu(event: MouseEvent, markdown: string) {
  const target = event.target as HTMLElement | null;
  if (event.defaultPrevented || target?.closest('input, textarea, [contenteditable="true"], a[href]')) return;
  event.preventDefault();
  const article = (event.currentTarget as HTMLElement | null) ?? target;
  Object.assign(selectionMenu, {
    open: true,
    x: event.clientX,
    y: event.clientY,
    selected: window.getSelection()?.toString() ?? '',
    plain: article?.innerText.trim() ?? '',
    markdown,
  });
}
