<script lang="ts" module>
import type { ZTreeNode } from '@shival99/z-ui-svelte';

/** A node: `id`, `label`, `children` (a node with children expands; `isLeaf` never does), `icon`, `disabled`, `data`. */
export type TreeNode<T = unknown> = ZTreeNode<T>;

/** What a row's snippet is given. */
export type TreeRow<T = unknown> = {
  node: TreeNode<T>;
  level: number;
  expanded: boolean;
  expandable: boolean;
  selected: boolean;
  active: boolean;
};

type Flat<T> = TreeRow<T> & { key: string };

/** Rows drawn above and below the visible window, so a short scroll does not show a gap. */
const OVERSCAN = 8;
/** How long a collapse or expand animates; scrolling never animates. */
const MOTION_MS = 220;
</script>

<script lang="ts" generics="T">
  import type { Snippet } from 'svelte';
  import { slide } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { ZIcon } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';

  let {
    nodes,
    expanded = $bindable([]),
    selected = $bindable([]),
    onSelect,
    onReveal,
    node: nodeSnippet,
    label = 'Tree',
    rowHeight = 28,
    indent = 14,
    maxHeight,
    lines = false,
    expandOnClick = true,
    class: className = '',
  }: {
    nodes: TreeNode<T>[];
    expanded?: string[];
    selected?: string[];
    onSelect?: (node: TreeNode<T>) => void;
    /** Called once for each node that scrolls into view: a loader row asks for its next page here. */
    onReveal?: (node: TreeNode<T>) => void;
    node?: Snippet<[TreeRow<T>]>;
    label?: string;
    rowHeight?: number;
    indent?: number;
    /** The height of the tree; without it the tree fills its parent. */
    maxHeight?: number;
    /** Branch lines in the indent. */
    lines?: boolean;
    /** A click on a folder's label also expands or collapses it (the chevron always does). */
    expandOnClick?: boolean;
    class?: string;
  } = $props();

  let box = $state<HTMLDivElement | undefined>(undefined);
  let viewport = $state(0);
  let scrollTop = $state(0);
  let active = $state('');
  /** True for a moment after a toggle: only then do the rows slide, so scrolling is never animated. */
  let moving = $state(false);
  let movingTimer: ReturnType<typeof setTimeout> | undefined;

  const expandedSet = $derived(new Set(expanded));
  const selectedSet = $derived(new Set(selected));

  // The visible rows, flattened: a row is listed when every folder above it is open.
  const rows = $derived.by(() => {
    const out: Flat<T>[] = [];
    const walk = (list: TreeNode<T>[], level: number) => {
      for (const node of list) {
        const expandable = !node.isLeaf && (node.children?.length ?? 0) > 0;
        const open = expandable && expandedSet.has(node.id);
        out.push({
          key: node.id,
          node,
          level,
          expanded: open,
          expandable,
          selected: selectedSet.has(node.id),
          active: active === node.id,
        });
        if (open) walk(node.children ?? [], level + 1);
      }
    };
    walk(nodes, 1);
    return out;
  });

  // Only the rows in view (and a few more) are in the DOM: the rest is empty space, sized by the two spacers.
  const height = $derived(maxHeight ?? viewport);
  const first = $derived(Math.max(0, Math.floor(scrollTop / rowHeight) - OVERSCAN));
  const last = $derived(Math.min(rows.length, Math.ceil((scrollTop + height) / rowHeight) + OVERSCAN));
  const visible = $derived(rows.slice(first, last));

  // Only the user's own toggle slides its rows: a folder that loads its listing, or a placeholder that is replaced, does not
  // move a second time.
  function startMotion() {
    moving = true;
    clearTimeout(movingTimer);
    movingTimer = setTimeout(() => (moving = false), MOTION_MS + 40);
  }

  // A node is reported when it comes into view (a loader row asks for its next page). It is reported again once the
  // rows have grown, while it is still in view: a loader at the end of a long page keeps the list going.
  const revealed = new Set<string>();
  $effect(() => {
    if (!onReveal) return;
    for (const item of visible) {
      const once = `${item.node.id}#${rows.length}`;
      if (revealed.has(once)) continue;
      revealed.add(once);
      onReveal(item.node);
    }
  });

  function toggle(node: TreeNode<T>) {
    startMotion();
    expanded = expandedSet.has(node.id) ? expanded.filter((id) => id !== node.id) : [...expanded, node.id];
  }

  function select(item: Flat<T>) {
    if (item.node.disabled) return;
    active = item.node.id;
    selected = [item.node.id];
    onSelect?.(item.node);
    if (item.expandable && expandOnClick) toggle(item.node);
  }

  function reveal(index: number) {
    if (!box) return;
    const top = index * rowHeight;
    if (top < box.scrollTop) box.scrollTop = top;
    else if (top + rowHeight > box.scrollTop + box.clientHeight) box.scrollTop = top + rowHeight - box.clientHeight;
  }

  function onKey(event: KeyboardEvent) {
    const index = rows.findIndex((item) => item.node.id === active);
    const current = rows[index];
    let next = -1;
    switch (event.key) {
      case 'ArrowDown':
        next = Math.min(rows.length - 1, index + 1);
        break;
      case 'ArrowUp':
        next = Math.max(0, index - 1);
        break;
      case 'Home':
        next = 0;
        break;
      case 'End':
        next = rows.length - 1;
        break;
      case 'ArrowRight':
        if (current?.expandable && !current.expanded) toggle(current.node);
        else if (current?.expanded) next = index + 1;
        break;
      case 'ArrowLeft':
        if (current?.expanded) toggle(current.node);
        else if (current) {
          // Up to the folder this row is in.
          for (let up = index - 1; up >= 0; up--) {
            if (rows[up].level < current.level) {
              next = up;
              break;
            }
          }
        }
        break;
      case 'Enter':
      case ' ':
        if (current) select(current);
        break;
      default:
        return;
    }
    event.preventDefault();
    if (next >= 0 && rows[next]) {
      active = rows[next].node.id;
      reveal(next);
    }
  }
</script>

<!-- The rows are plain elements in a window: only the visible ones exist, so a long list stays light to scroll. -->
<div
  bind:this={box}
  bind:clientHeight={viewport}
  role="tree"
  aria-label={label}
  tabindex="0"
  class="tree relative w-full overflow-x-hidden overflow-y-auto overscroll-y-contain outline-none {className}"
  style:height={maxHeight !== undefined ? `${maxHeight}px` : '100%'}
  onscroll={(event) => (scrollTop = event.currentTarget.scrollTop)}
  onkeydown={onKey}
>
  <div style:height="{first * rowHeight}px" aria-hidden="true"></div>
  {#each visible as item, offset (item.key)}
    {@const isSelected = item.selected}
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions (keys are handled on the tree) -->
    <div
      role="treeitem"
      tabindex="-1"
      data-index={first + offset}
      data-selected={isSelected}
      data-disabled={item.node.disabled || undefined}
      aria-level={item.level}
      aria-expanded={item.expandable ? item.expanded : undefined}
      aria-selected={isSelected}
      aria-disabled={item.node.disabled || undefined}
      class="tree-row group/tree-row relative flex w-full cursor-pointer items-center gap-1.5 pr-2 select-none text-muted-foreground hover:text-foreground data-[selected=true]:font-medium data-[selected=true]:text-foreground data-[disabled=true]:cursor-not-allowed data-[disabled=true]:opacity-50"
      style:height="{rowHeight}px"
      style:padding-left="{(item.level - 1) * indent}px"
      onclick={() => select(item)}
      onfocus={() => (active = item.node.id)}
      onmousedown={(event) => event.detail > 1 && event.preventDefault()}
      transition:slide={{ duration: moving ? MOTION_MS : 0, easing: cubicOut }}
    >
      {#if lines}
        {#each { length: item.level - 1 } as _, depth (depth)}
          <span class="pointer-events-none absolute inset-y-0 w-px bg-[var(--z-tree-guide)]" style:left="{depth * indent + 10}px" aria-hidden="true"></span>
        {/each}
      {/if}
      <span
        class="inline-flex shrink-0 items-center justify-center {item.expandable && !item.node.disabled ? 'hover:text-foreground' : ''}"
        style:width="16px"
        style:height="16px"
        onclick={(event) => {
          event.stopPropagation();
          if (item.expandable) toggle(item.node);
        }}
        role="presentation"
      >
        {#if item.expandable}
          <ZIcon zType={'lucideChevronRight' as ZIconName} zSize={14} class="tree-chevron {item.expanded ? 'rotate-90' : ''}" />
        {/if}
      </span>
      {#if nodeSnippet}
        {@render nodeSnippet({ node: item.node, level: item.level, expanded: item.expanded, expandable: item.expandable, selected: isSelected, active: active === item.node.id })}
      {:else}
        <ZIcon zType={(item.node.icon ?? (item.expandable ? "lucideFolder" : "lucideFile")) as ZIconName} zSize={14} class="shrink-0" />
        <span class="min-w-0 flex-1 truncate">{item.node.label}</span>
      {/if}
    </div>
  {/each}
  <div style:height="{Math.max(0, rows.length - last) * rowHeight}px" aria-hidden="true"></div>
</div>

<style>
  .tree {
    --z-tree-guide: color-mix(in oklab, var(--color-foreground) 14%, var(--color-background));
    --z-tree-row-gap: 2px;
    /* Own stacking context: a row's highlight (z-index -1) paints above the panel's background, under the text. */
    isolation: isolate;
  }
  /* The highlight of a row: hover and the selected row share --row-bg, which the row's own buttons (the hover overlay at
     the right) read too, so the highlight is the same colour under them. */
  .tree-row {
    position: relative;
    transition: color 200ms;
  }
  .tree-row::before {
    content: '';
    position: absolute;
    inset: calc(var(--z-tree-row-gap) / 2) 4px;
    z-index: -1;
    border-radius: 3px;
    background: var(--row-bg, transparent);
    pointer-events: none;
  }
  .tree-row:hover,
  .tree-row[data-selected='true'] {
    --row-bg: var(--color-muted);
  }
</style>
