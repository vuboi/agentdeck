<script lang="ts">
  import { ZButton, ZIcon } from '@shival99/z-ui-svelte';
  import type { ZDropdownMenuItem, ZIconName, ZPopoverPosition } from '@shival99/z-ui-svelte';
  import Menu from './Menu.svelte';
  import { TRIGGER_ON } from '../popover';

  // An icon button that opens a menu under it, lit while the menu is open.
  let {
    items,
    icon = 'lucideEllipsis',
    label,
    position = 'bottom-right',
    width = 220,
    class: className = 'size-7!',
  }: { items: ZDropdownMenuItem[]; icon?: string; label: string; position?: ZPopoverPosition; width?: number; class?: string } = $props();

  let anchor = $state<HTMLElement | null>(null);
  let open = $state(false);
</script>

<ZButton
  bind:ref={anchor}
  zWave={false}
  zType="ghost"
  zSize="sm"
  class="{className} shrink-0 p-0 {open ? TRIGGER_ON : '!bg-transparent !text-muted-foreground hover:!bg-primary/10 hover:!text-primary'}"
  aria-label={label}
  aria-haspopup="menu"
  aria-expanded={open}
  onclick={() => (open = !open)}
>
  <ZIcon zType={icon as ZIconName} class="size-4" />
</ZButton>

<Menu {items} bind:open {anchor} {position} {width} {label} />
