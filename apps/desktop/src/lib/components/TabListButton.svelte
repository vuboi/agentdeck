<script lang="ts">
    import { ZButton, ZIcon } from '@shival99/z-ui-svelte';
    import type { ZIconName } from '@shival99/z-ui-svelte';
    import PickerPopover, { type PickerItem } from './PickerPopover.svelte';
    import { TRIGGER_ON } from '../popover';

    // The ▾ at the end of a tab strip (chats, files, the browser): every open tab in a list with a search box, to jump to
    // one when they do not all fit. The current tab is ticked.
    let {
        items,
        onpick,
        label = 'Open tabs',
        icon = 'lucideChevronDown',
    }: {
        items: PickerItem<string>[];
        onpick: (id: string) => void;
        label?: string;
        icon?: ZIconName;
    } = $props();

    let anchor = $state<HTMLElement | null>(null);
    let open = $state(false);
    let query = $state('');

    function pick(item: PickerItem<string>) {
        open = false;
        query = '';
        onpick(item.data);
    }
</script>

<ZButton
    bind:ref={anchor}
    zWave={false}
    zType="ghost"
    zSize="sm"
    class="size-7! shrink-0 p-0 {open
        ? TRIGGER_ON
        : '!bg-transparent !text-muted-foreground hover:!bg-primary/10 hover:!text-primary'}"
    aria-label={label}
    aria-haspopup="listbox"
    aria-expanded={open}
    onclick={() => (open = !open)}
>
    <ZIcon zType={icon} class="size-4" />
</ZButton>

<PickerPopover
    {anchor}
    bind:open
    bind:query
    {items}
    local
    search
    placeholder="Search tabs"
    {label}
    empty="No tab matches"
    position="bottom-right"
    size="sm"
    width={280}
    onpick={pick}
    onclose={() => (query = '')}
/>
