<script lang="ts">
    import type {
        ZDropdownMenuItem,
        ZIconName,
        ZPopoverPosition,
    } from '@shival99/z-ui-svelte';
    import PickerPopover, { type PickerItem } from './PickerPopover.svelte';
    import Menu from './Menu.svelte';

    // The menu of the app: icon and text rows, a line between sections, a shortcut at the end, submenus: the same list
    // as every picker (`PickerPopover`), so a right click on a tab, a file or a chat and the menu of a button all look
    // and move the same. Give it items in the shape the old dropdown took; open it on an `anchor` (a button) or `at` a
    // point (a right click).
    let {
        items,
        open = $bindable(false),
        anchor = null,
        at = null,
        position = 'bottom-left',
        width = 220,
        label = 'Menu',
        onselect,
        onenter,
    }: {
        items: ZDropdownMenuItem[];
        open?: boolean;
        anchor?: HTMLElement | null;
        at?: { x: number; y: number } | null;
        position?: ZPopoverPosition;
        width?: number;
        label?: string;
        onselect?: () => void;
        onenter?: () => void;
    } = $props();

    // A right click has no element to sit on: a 1px box at the pointer is the anchor.
    let dot = $state<HTMLElement | null>(null);
    const place = $derived(at ? dot : anchor);

    const uid = Math.random().toString(36).slice(2, 7);
    // Rows by section: `divide` starts a new one before or after a row.
    const rows = $derived.by(() => {
        const out: PickerItem<ZDropdownMenuItem>[] = [];
        let section = 0;
        for (const item of items) {
            if (item.hidden) continue;
            if (item.divide === 'before' && out.length > 0) section++;
            out.push({
                id: `${uid}:${out.length}`,
                label: item.label,
                icon: item.icon as ZIconName | undefined,
                iconSrc: (item as ZDropdownMenuItem & { iconSrc?: string }).iconSrc,
                shortcut: item.shortcut,
                selected: item.checked,
                disabled: item.disabled,
                danger: item.class?.includes('destructive') || undefined,
                more: Boolean(item.submenu?.length) || undefined,
                group: `s${section}`,
                data: item,
            });
            if (item.divide === 'after') section++;
        }
        return out;
    });

    // The submenu of the row that has one: opens beside it.
    let sub = $state<{
        items: ZDropdownMenuItem[];
        anchor: HTMLElement;
    } | null>(null);
    let subOpen = $state(false);
    let closeSubTimer: ReturnType<typeof setTimeout> | undefined;
    function clearSubTimer() {
        if (closeSubTimer) clearTimeout(closeSubTimer);
        closeSubTimer = undefined;
    }
    function closeAll() {
        clearSubTimer();
        open = false;
        sub = null;
        subOpen = false;
        onselect?.();
    }
    function hover(row: PickerItem<ZDropdownMenuItem>, element: HTMLElement) {
        clearSubTimer();
        if (!row.data.submenu?.length) {
            closeSubTimer = setTimeout(() => {
                sub = null;
                subOpen = false;
            }, 120);
            return;
        }
        sub = { items: row.data.submenu, anchor: element };
        subOpen = true;
    }
    function leaveRow(row: PickerItem<ZDropdownMenuItem>) {
        if (!row.data.submenu?.length && !sub) return;
        closeSubTimer = setTimeout(() => {
            sub = null;
            subOpen = false;
        }, 160);
    }
    $effect(() => {
        if (!open) {
            sub = null;
            subOpen = false;
        }
    });

    function pick(row: PickerItem<ZDropdownMenuItem>) {
        const item = row.data;
        if (item.submenu?.length) {
            const element = document.querySelector<HTMLElement>(
                `[data-picker-row="${CSS.escape(row.id)}"]`,
            );
            if (element) {
                sub = { items: item.submenu, anchor: element };
                subOpen = true;
            }
            return;
        }
        closeAll();
        item.onClick?.();
    }
</script>

{#if at}
    <div
        bind:this={dot}
        class="pointer-events-none fixed z-50 size-px"
        style:left="{at.x}px"
        style:top="{at.y}px"
        aria-hidden="true"
    ></div>
{/if}

<PickerPopover
    anchor={place}
    bind:open
    items={rows}
    {label}
    {position}
    {width}
    size="sm"
    headings={false}
    tall
    keys
    initialActive={false}
    onpick={pick}
    onhover={hover}
    onrowleave={leaveRow}
    {onenter}
/>

{#if sub}
    <Menu
        items={sub.items}
        bind:open={subOpen}
        anchor={sub.anchor}
        position="right-top"
        {width}
        label="Submenu"
        onselect={closeAll}
        onenter={clearSubTimer}
    />
{/if}
