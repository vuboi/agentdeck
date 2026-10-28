<script lang="ts" module>
    import type { ZIconName } from '@shival99/z-ui-svelte';

    /** One row of a picker. `hint` is a dimmed second part on the same line (a folder); `description` shows beside the
     *  list while the row is active. Rows with the same `group` sit under one heading, in the order given. */
    export type PickerItem<T = unknown> = {
        id: string;
        label: string;
        hint?: string;
        icon?: ZIconName;
        /** A platform app icon as a data URL; the standard icon stays underneath as a failure fallback. */
        iconSrc?: string;
        description?: string;
        group?: string;
        /** The row is the current one (the open tab): it keeps the chosen tint and a tick. */ selected?: boolean;
        /** Cannot be picked. */ disabled?: boolean;
        /** Destructive (delete): drawn in red. */ danger?: boolean;
        /** A key hint at the end of the row (`Ctrl+C`). */ shortcut?: string;
        /** Opens a submenu: a chevron at the end of the row. */ more?: boolean;
        /** An agent: drawn with its brand logo in its colour, in place of `icon`. */ agent?: Agent;
        /** Lines added and removed, drawn at the end of the row (a changed file). */ stat?: {
            added: number;
            removed: number;
        };
        data: T;
    };

    /** 999, 1K, 1.2K, 12K, 1.5M: a count short enough for a small badge. */
    export function compact(count: number): string {
        if (count < 1000) return String(count);
        const unit = count < 1_000_000 ? 1000 : 1_000_000;
        const value = count / unit;
        const text =
            value < 10
                ? value.toFixed(1).replace(/\.0$/, '')
                : String(Math.round(value));
        return `${text}${unit === 1000 ? 'K' : 'M'}`;
    }
</script>

<script lang="ts" generics="T">
    import {
        ZIcon,
        ZPopover,
        createVirtualizer,
        zFuzzyMatch,
        zHighlightPositions,
    } from '@shival99/z-ui-svelte';
    import type { ZPopoverPosition } from '@shival99/z-ui-svelte';
    import { tick, untrack } from 'svelte';
    import { glide, listHeight, ROW_ON } from '../popover';
    import { fade } from 'svelte/transition';
    import { type Agent } from '../sessions.svelte';
    import AgentLogo from './AgentLogo.svelte';

    // The pickers of the app (a file, a mention, a command): a popover on its anchor instead of a full-screen palette.
    // With `search` it has its own box to type in (Find a file); without, the text comes from elsewhere (the message box
    // after `@` or `/`) and the owner forwards its keys to `key()`. Only the rows in view are mounted; both ends of the
    // list fade while there is more to scroll to.
    let {
        anchor,
        open = $bindable(false),
        items: all,
        query = $bindable(''),
        local = false,
        multiple = false,
        search = false,
        headings = true,
        tall = false,
        fillHeight = false,
        fitContent = false,
        keys = false,
        placeholder = 'Search…',
        label = placeholder,
        empty = 'Nothing found',
        footer = '',
        loading = false,
        position = 'bottom-left',
        width = 'anchor',
        size = 'md',
        rowHeight,
        heightReduction = 0,
        offset = 6,
        onpick,
        onclose,
        onmore,
        initialActive = true,
        onhover,
        onrowleave,
        onenter,
    }: {
        anchor: HTMLElement | null | undefined;
        open?: boolean;
        items: PickerItem<T>[];
        /** The picker narrows `items` by what is typed, itself (a short fixed list); else the owner does. */
        local?: boolean;
        /** This list permits multiple selected provider rows. */
        multiple?: boolean;
        query?: string;
        search?: boolean;
        /** Group names are written above their rows; else a line is drawn between groups (a menu). */
        headings?: boolean;
        /** As tall as the room allows, not the share of the window the lists have (a menu shows all its rows). */
        tall?: boolean;
        /** Use the full list-height cap even when there are only a few items, so related pickers stay the same height. */
        fillHeight?: boolean;
        /** Size a local list to its rows instead of filling the height cap. */
        fitContent?: boolean;
        /** Takes the arrows, Enter and Escape from the window while open (a menu, whose focus is somewhere else). */
        keys?: boolean;
        placeholder?: string;
        /** What the list is, for screen readers (the placeholder when there is a search box). */
        label?: string;
        empty?: string;
        footer?: string;
        loading?: boolean;
        position?: ZPopoverPosition;
        /** Pixels, or 'anchor': as wide as what it opens from (never under 280px nor over 420px). */
        width?: number | 'anchor';
        /** `sm` for tight lists (a branch, a project), `md` for the rest. */
        size?: 'sm' | 'md';
        /** Height of a row in px; by default the one of its `size` (28, or 32), gap included. */
        rowHeight?: number;
        /** Shrinks the available list height by a fixed amount for compact option popovers. */
        heightReduction?: number;
        offset?: number;
        onpick: (item: PickerItem<T>) => void;
        onclose?: () => void;
        /** The list was scrolled to its end: the owner may load more rows (a long search shows a few at first). */
        onmore?: () => void;
        /** Whether to highlight the first row immediately when opened. Menus leave rows inactive until pointer/key input. */
        initialActive?: boolean;
        onhover?: (item: PickerItem<T>, element: HTMLElement) => void;
        onrowleave?: (item: PickerItem<T>) => void;
        onenter?: () => void;
    } = $props();

    const SIZE = {
        sm: { row: 28, text: 'text-xs', icon: 'size-3.5' },
        md: { row: 34, text: 'text-[13px]', icon: 'size-4' },
    } as const;
    const look = $derived(SIZE[size]);
    const rowPx = $derived(rowHeight ?? look.row);
    const ROW_PX = 30;
    const fold = (text: string) =>
        text
            .toLowerCase()
            .normalize('NFD')
            .replace(/[\u0300-\u036f]/g, '')
            .replace(/đ/g, 'd');
    const items = $derived.by<PickerItem<T>[]>(() => {
        const needle = fold(query).trim();
        if (!local || !needle) return all;
        return all.filter((item) =>
            fold(`${item.label} ${item.hint ?? ''}`).includes(needle),
        );
    });
    const popWidth = $derived(
        width === 'anchor'
            ? Math.round(
                  Math.min(
                      420,
                      Math.max(
                          280,
                          anchor?.getBoundingClientRect().width ?? 320,
                      ),
                  ),
              )
            : width,
    );
    const HEAD = 26;
    const RULE = 9;
    // The list is never taller than the room on its side of the anchor (a short window would push it off screen).
    // Worked out when it opens, before the first paint: the popover is at its height from the first frame (it used to open
    // at a guess and settle a frame later, which looked like it shrinking).
    const height = $derived.by(() => {
        if (!open || !anchor) return 340;
        const box = anchor.getBoundingClientRect();
        const chrome = (search ? 40 : 0) + (footer ? 28 : 0) + offset + 16;
        const room = position.startsWith('top')
            ? box.top - chrome
            : window.innerHeight - box.bottom - chrome;
        const cap = Math.max(
            60,
            (tall || fitContent ? Math.round(room) : listHeight(room)) -
                heightReduction,
        );
        if (fitContent && local) {
            const groups = new Set(all.map((item) => item.group ?? '')).size;
            const content =
                all.length * rowPx +
                (headings && groups > 1
                    ? groups * HEAD
                    : Math.max(0, groups - 1) * RULE) +
                8;
            return Math.max(60, Math.min(cap, content));
        }
        if (fillHeight) return Math.max(60, cap);
        // A short list that is narrowed here (`local`) is only as tall as all its rows, worked out from the whole list so that
        // typing does not shrink it; a long one, and one the owner fills (a search that is still coming), take the whole share.
        if (local && all.length > 0) {
            const groups = new Set(all.map((item) => item.group ?? '')).size;
            const content =
                all.length * rowPx +
                (headings && groups > 1
                    ? groups * HEAD
                    : Math.max(0, groups - 1) * RULE) +
                8;
            return Math.max(60, Math.min(cap, content));
        }
        return Math.max(120, cap);
    });

    type Row =
        | { kind: 'head'; text: string }
        | { kind: 'rule' }
        | { kind: 'item'; index: number };
    // Headings only when the rows come in more than one group.
    const rows = $derived.by<Row[]>(() => {
        const grouped = new Set(items.map((item) => item.group ?? '')).size > 1;
        const out: Row[] = [];
        let last: string | undefined;
        items.forEach((item, index) => {
            if (grouped && item.group && item.group !== last) {
                if (headings) out.push({ kind: 'head', text: item.group });
                else if (last !== undefined) out.push({ kind: 'rule' });
            }
            last = item.group;
            out.push({ kind: 'item', index });
        });
        return out;
    });

    let active = $state(-1);
    // A new list (more typed, another search answer) starts on its first row.
    $effect(() => {
        void items;
        active = initialActive ? 0 : -1;
    });

    let scroller = $state<HTMLDivElement>();
    const rowSize = (index: number) =>
        rows[index]?.kind === 'head'
            ? HEAD
            : rows[index]?.kind === 'rule'
              ? RULE
              : rowPx;
    const virtualizer = createVirtualizer<HTMLDivElement, HTMLDivElement>({
        count: 0,
        getScrollElement: () => scroller ?? null,
        estimateSize: rowSize,
        overscan: 8,
    });
    $effect(() => {
        const count = rows.length;
        void scroller;
        untrack(() =>
            $virtualizer.setOptions({
                count,
                getScrollElement: () => scroller ?? null,
                estimateSize: rowSize,
                overscan: 8,
            }),
        );
    });
    const shown = $derived($virtualizer.getVirtualItems());
    const total = $derived($virtualizer.getTotalSize());

    // Keep the list clear at rest; fades appear only after the user has started scrolling.
    let atTop = $state(true);
    let atEnd = $state(true);
    function measure() {
        if (!scroller) return;
        atTop = scroller.scrollTop <= 0;
        // The list's height from the virtualizer (plus the scroller's p-1): scrollHeight here forced a layout on every
        // scroll event.
        const full = total + 8;
        atEnd = scroller.scrollTop + scroller.clientHeight >= full - 2;
        if (scroller.scrollTop + scroller.clientHeight >= full - 3 * ROW_PX)
            onmore?.();
    }
    $effect(() => {
        void total;
        void tick().then(measure);
    });

    // Each new opening starts at the top, even when the component stays mounted between openings.
    let wasOpen = false;
    $effect(() => {
        if (!open) {
            wasOpen = false;
            return;
        }
        if (wasOpen) return;
        wasOpen = true;
        const selected = untrack(() =>
            items.findIndex((item) => item.selected),
        );
        active = selected < 0 ? (initialActive ? 0 : -1) : selected;
        void tick().then(() => {
            if (!open || !scroller) return;
            scroller.scrollTop = 0;
            measure();
        });
    });

    // A new search starts at the top of what it found, not where the list was scrolled to before.
    let lastQuery = '';
    $effect(() => {
        const now = query;
        if (now === lastQuery) return;
        lastQuery = now;
        untrack(() => {
            if (!scroller) return;
            scroller.scrollTop = 0;
            measure();
        });
    });

    /** The description worth showing: none for an empty one or a leftover YAML marker (`>`, `|`). */
    const describe = (item: PickerItem<T> | undefined) => {
        const text = item?.description?.trim();
        return text && !/^[>|][-+]?$/.test(text) ? text : '';
    };
    const rowOf = (itemIndex: number) =>
        rows.findIndex((row) => row.kind === 'item' && row.index === itemIndex);
    function move(by: number) {
        if (items.length === 0) return;
        active = (active < 0 ? (by > 0 ? -1 : 0) : active) + by;
        active = (active + items.length) % items.length;
        $virtualizer.scrollToIndex(rowOf(active), { align: 'auto' });
    }
    function pick(index = active) {
        const item = items[index];
        if (!item || item.disabled) return;
        onpick(item);
    }
    function close() {
        open = false;
        onclose?.();
    }

    /** The keys of the picker, for a box outside it (the message box) that keeps the focus. True when it took the key. */
    export function key(event: KeyboardEvent): boolean {
        if (!open) return false;
        if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
            event.preventDefault();
            move(event.key === 'ArrowDown' ? 1 : -1);
            return true;
        }
        if (
            (event.key === 'Enter' || event.key === 'Tab') &&
            !event.shiftKey &&
            items.length > 0
        ) {
            event.preventDefault();
            pick();
            return true;
        }
        if (event.key === 'Escape') {
            event.preventDefault();
            close();
            return true;
        }
        return false;
    }

    $effect(() => {
        if (!keys || !open) return;
        const onKey = (event: KeyboardEvent) => {
            if (key(event)) event.stopPropagation();
        };
        window.addEventListener('keydown', onKey, true);
        return () => window.removeEventListener('keydown', onKey, true);
    });

    /** The label with what was typed in bold. */
    function marked(label: string) {
        const hit = query.trim() ? zFuzzyMatch(label, query) : null;
        return hit
            ? zHighlightPositions(label, hit.positions)
            : [{ text: label, match: false }];
    }

    // The search box is ready to type in as soon as the picker opens.
    let input = $state<HTMLInputElement>();
    $effect(() => {
        if (!open || !search) return;
        // The popover mounts a moment after `open` turns true: try now, next frame and once more shortly after.
        const focus = () => input?.focus({ preventScroll: true });
        focus();
        const frame = requestAnimationFrame(focus);
        const late = setTimeout(focus, 60);
        return () => {
            cancelAnimationFrame(frame);
            clearTimeout(late);
        };
    });

    // The active row's description, in a card beside the list.
    let activeRow = $state<HTMLElement | null>(null);
    $effect(() => {
        const id = items[active]?.id;
        void shown;
        if (!open || !describe(items[active])) {
            activeRow = null;
            return;
        }
        void tick().then(
            () =>
                (activeRow =
                    scroller?.querySelector<HTMLElement>(
                        `[data-picker-row="${CSS.escape(id ?? '')}"]`,
                    ) ?? null),
        );
    });
</script>

<ZPopover
    {anchor}
    bind:open
    zTrigger="manual"
    zPosition={position}
    zOffset={offset}
    zWidth={popWidth}
    onClose={() => onclose?.()}
>
    {#snippet children()}
        <div
            class="flex w-full flex-col {look.text}"
            role="listbox"
            aria-multiselectable={multiple || undefined}
            aria-label={label}
            tabindex="-1"
            onpointerenter={onenter}
        >
            {#if search}
                <div
                    class="flex h-10 shrink-0 items-center gap-2 border-b border-border px-3"
                >
                    <ZIcon
                        zType={'lucideSearch' as ZIconName}
                        class="size-3.5 shrink-0 text-muted-foreground"
                    />
                    <input
                        bind:this={input}
                        bind:value={query}
                        class="min-w-0 flex-1 bg-transparent text-[13px] outline-none placeholder:text-muted-foreground"
                        {placeholder}
                        aria-label={placeholder}
                        spellcheck="false"
                        autocomplete="off"
                        onkeydown={(event) => key(event)}
                    />
                </div>
            {/if}
            <div class="relative">
                <div
                    bind:this={scroller}
                    class="overflow-x-hidden overflow-y-auto p-1"
                    style:height={search ? `${height}px` : undefined}
                    style:max-height="{height}px"
                    onscroll={measure}
                >
                    {#if items.length === 0}
                        <!-- In the fixed height: the box does not change size whether it is waiting, empty or full. -->
                        <div
                            class="grid h-full min-h-20 place-items-center px-2 text-center text-xs text-muted-foreground"
                        >
                            {#if loading}
                                <ZIcon
                                    zType={'lucideLoaderCircle' as ZIconName}
                                    class="size-5 animate-spin text-primary"
                                    aria-label="Loading"
                                />
                            {:else}
                                <p class="m-0">{empty}</p>
                            {/if}
                        </div>
                    {:else}
                        <div
                            class="relative w-full"
                            style:height="{total}px"
                            in:fade={{ duration: 160 }}
                            {@attach glide()}
                        >
                            {#each shown as slot (slot.key)}
                                {@const row = rows[slot.index]}
                                <!-- The slot is the row and its gap: the row inside leaves 1px above and below, so rows never touch. -->
                                <div
                                    class="absolute top-0 left-0 w-full {row?.kind ===
                                    'item'
                                        ? 'py-px'
                                        : ''}"
                                    style:height="{slot.size}px"
                                    style:transform="translateY({slot.start}px)"
                                >
                                    {#if row?.kind === 'head'}
                                        <div
                                            class="flex h-full items-end px-2 pb-1 text-[11px] font-medium text-muted-foreground"
                                        >
                                            {row.text}
                                        </div>
                                    {:else if row?.kind === 'rule'}
                                        <div class="flex h-full items-center">
                                            <div
                                                class="mx-1 h-px w-full bg-border"
                                            ></div>
                                        </div>
                                    {:else if row}
                                        {@const item = items[row.index]}
                                        <button
                                            type="button"
                                            role="option"
                                            aria-selected={item.selected ===
                                                true}
                                            aria-disabled={item.disabled ||
                                                undefined}
                                            data-picker-row={item.id}
                                            data-glide-item={item.disabled
                                                ? undefined
                                                : ''}
                                            class="flex h-full w-full items-center gap-2.5 rounded-md px-2 text-left outline-none {item.disabled
                                                ? 'cursor-not-allowed opacity-50'
                                                : 'cursor-pointer'} {item.danger
                                                ? row.index === active
                                                    ? 'bg-destructive/15 text-destructive'
                                                    : 'text-destructive'
                                                : row.index === active ||
                                                    item.selected
                                                  ? ROW_ON
                                                  : 'text-foreground'}"
                                            onmousedown={(event) =>
                                                event.preventDefault()}
                                            onclick={() => pick(row.index)}
                                            onpointerenter={(event) => {
                                                active = row.index;
                                                onhover?.(
                                                    item,
                                                    event.currentTarget,
                                                );
                                            }}
                                            onpointerleave={() =>
                                                onrowleave?.(item)}
                                        >
                                            {#if item.agent}<AgentLogo
                                                    agent={item.agent}
                                                    class="size-4 shrink-0"
                                                />{:else if item.iconSrc}<span class="relative size-4 shrink-0"><span class="hidden"><ZIcon
                                                            zType={item.icon ?? 'lucideAppWindow'}
                                                            class="size-4 text-muted-foreground"
                                                        /></span><img
                                                        src={item.iconSrc}
                                                        alt=""
                                                        class="relative size-4 object-contain"
                                                        onerror={(event) => {
                                                            const image = event.currentTarget as HTMLImageElement;
                                                            image.hidden = true;
                                                            (image.previousElementSibling as HTMLElement | null)?.classList.remove('hidden');
                                                        }}
                                                    /></span>{:else if item.icon}<ZIcon
                                                    zType={item.icon}
                                                    class="shrink-0 {look.icon} {item.danger
                                                        ? 'text-destructive'
                                                        : 'text-muted-foreground'}"
                                                />{/if}
                                            <span
                                                class="min-w-0 shrink truncate"
                                                >{#each marked(item.label) as part, at (at)}{#if part.match}<span
                                                            class="font-semibold text-primary"
                                                            >{part.text}</span
                                                        >{:else}{part.text}{/if}{/each}</span
                                            >
                                            {#if item.hint}<span
                                                    class="min-w-0 flex-1 truncate text-xs text-muted-foreground"
                                                    >{item.hint}</span
                                                >{:else if item.stat}<span
                                                    class="flex-1"
                                                ></span>{/if}
                                            {#if item.shortcut}<span
                                                    class="ml-auto shrink-0 pl-3 text-[11px] text-muted-foreground"
                                                    >{item.shortcut}</span
                                                >{/if}
                                            {#if item.more}<ZIcon
                                                    zType={'lucideChevronRight' as ZIconName}
                                                    class="ml-auto size-3.5 shrink-0 text-muted-foreground"
                                                />{/if}
                                            {#if item.selected}<ZIcon
                                                    zType={'lucideCheck' as ZIconName}
                                                    class="ml-auto size-3.5 shrink-0"
                                                />{/if}
                                            {#if item.stat}<span
                                                    class="flex shrink-0 gap-1.5 font-mono text-xs tabular-nums"
                                                    ><span
                                                        class="text-emerald-500"
                                                        >+{compact(
                                                            item.stat.added,
                                                        )}</span
                                                    ><span class="text-red-500"
                                                        >−{compact(
                                                            item.stat.removed,
                                                        )}</span
                                                    ></span
                                                >{/if}
                                        </button>
                                    {/if}
                                </div>
                            {/each}
                        </div>
                    {/if}
                </div>
                {#if !atTop}<div
                        class="pointer-events-none absolute inset-x-0 top-0 h-6 bg-gradient-to-b from-popover to-transparent"
                        aria-hidden="true"
                    ></div>{/if}
                {#if !atTop && !atEnd}<div
                        class="pointer-events-none absolute inset-x-0 bottom-0 h-6 bg-gradient-to-t from-popover to-transparent"
                        aria-hidden="true"
                    ></div>{/if}
            </div>
            {#if footer}
                <div
                    class="border-t border-border px-3 py-1.5 text-[10px] text-muted-foreground"
                >
                    {footer}
                </div>
            {/if}
        </div>
    {/snippet}
</ZPopover>

{#if activeRow && open}
    <ZPopover
        anchor={activeRow}
        open={true}
        zTrigger="manual"
        zPosition="right-top"
        zOffset={10}
        zWidth={280}
        zCloseOnOutside={false}
    >
        {#snippet children()}
            <p class="m-0 p-2.5 text-xs leading-relaxed text-muted-foreground">
                {describe(items[active])}
            </p>
        {/snippet}
    </ZPopover>
{/if}
