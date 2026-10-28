import { getCurrentWindow } from '@tauri-apps/api/window';
import { inTauri } from './history';
import { toast } from './toast';

/** Attach to a desktop header to allow dragging from its tabs and buttons without consuming normal clicks. */
export function windowDrag(element: HTMLElement): (() => void) | undefined {
  if (!inTauri()) return;
  let press: { x: number; y: number; id: number } | undefined;
  let suppressClick = false;
  const excluded = 'input, textarea, select, [contenteditable="true"], [data-no-window-drag]';

  const clear = () => {
    press = undefined;
    window.removeEventListener('pointermove', move, true);
    window.removeEventListener('pointerup', clear, true);
    window.removeEventListener('pointercancel', clear, true);
    window.removeEventListener('blur', clear);
  };

  const move = (event: PointerEvent) => {
    if (!press || event.pointerId !== press.id) return;
    if (!(event.buttons & 1)) { clear(); return; }
    if (Math.hypot(event.clientX - press.x, event.clientY - press.y) < 5) return;
    clear();
    suppressClick = true;
    event.preventDefault();
    event.stopImmediatePropagation();
    void getCurrentWindow().startDragging().catch((error: unknown) => {
      suppressClick = false;
      toast.error(`Could not move window: ${String(error)}`);
    });
  };

  const down = (event: PointerEvent) => {
    suppressClick = false;
    clear();
    if (event.button !== 0 || !event.isPrimary || event.pointerType !== 'mouse') return;
    if ((event.target as Element).closest(excluded)) return;
    press = { x: event.clientX, y: event.clientY, id: event.pointerId };
    window.addEventListener('pointermove', move, true);
    window.addEventListener('pointerup', clear, true);
    window.addEventListener('pointercancel', clear, true);
    window.addEventListener('blur', clear);
  };

  const click = (event: MouseEvent) => {
    if (!suppressClick || event.detail === 0) return;
    suppressClick = false;
    event.preventDefault();
    event.stopImmediatePropagation();
  };

  // Only the blank header maximizes on double click; interactive controls retain their own double-click behavior.
  const doubleClick = (event: MouseEvent) => {
    if (event.button !== 0 || (event.target as Element).closest(`${excluded}, button, a, [role="tab"], [role="button"]`)) return;
    void getCurrentWindow().toggleMaximize().catch((error: unknown) => toast.error(String(error)));
  };

  element.addEventListener('pointerdown', down, true);
  element.addEventListener('click', click, true);
  element.addEventListener('dblclick', doubleClick);
  return () => {
    clear();
    element.removeEventListener('pointerdown', down, true);
    element.removeEventListener('click', click, true);
    element.removeEventListener('dblclick', doubleClick);
  };
}
