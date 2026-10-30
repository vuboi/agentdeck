<script lang="ts">
  import type { ZDropdownMenuItem } from '@shival99/z-ui-svelte';
  import Menu from '../ui/Menu.svelte';

  // The right-click menu of an attached picture or document. The owner builds the items for the chip under the pointer.
  let open = $state(false);
  let at = $state({ x: 0, y: 0 });
  let items = $state<ZDropdownMenuItem[]>([]);
  const rowHeight = 28;
  const ruleHeight = 9;
  const menuPadding = 8;
  const popoverChrome = 23; // picker chrome plus the one-pixel context anchor
  const width = 200;

  /** Open the menu at the pointer with these items; the browser's own menu stays off. */
  export function openAt(event: MouseEvent, menuItems: ZDropdownMenuItem[]): void {
    event.preventDefault();
    items = menuItems;
    const rules = menuItems.filter((item) => item.divide === 'before').length;
    const menuHeight = menuItems.length * rowHeight + rules * ruleHeight + menuPadding;
    const roomNeeded = menuHeight + popoverChrome;
    at = {
      x: Math.min(event.clientX, Math.max(6, window.innerWidth - width)),
      y: Math.min(event.clientY, Math.max(6, window.innerHeight - roomNeeded)),
    };
    open = true;
  }
</script>

<Menu {items} bind:open {at} position="bottom-left" {width} fitContent label="Attachment menu" />
