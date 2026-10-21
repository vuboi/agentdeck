# Table — `z-table`

Selector: `z-table`, `exportAs="zTable"`. A feature-rich data table built on `@tanstack/angular-table` —
sorting, filtering, pagination, virtual scrolling, column pin/resize/reorder, inline editing, row
selection/expansion/pinning, grouping, bulk actions, and Excel export are all driven by a single `zConfig`
object. Never hand-roll a data table with `<table>` + `*ngFor`.

```html
<z-table [zConfig]="tableConfig" (zChange)="onTableChange($event)" />
```

```ts
protected readonly tableConfig: ZTableConfig<User> = {
  data: this.users(),
  columns: [
    { id: 'name', accessorKey: 'name', header: 'Name', sort: true, filter: true },
    { id: 'email', accessorKey: 'email', header: 'Email' },
    {
      id: 'actions',
      body: {
        actions: [{ key: 'edit', icon: 'lucidePencil', tooltip: 'Edit' }],
      },
    },
  ],
};
```

## Inputs

| Input      | Type                        | Default                     | Notes                                                                                                      |
| ---------- | --------------------------- | --------------------------- | ---------------------------------------------------------------------------------------------------------- |
| `zClass`   | `ClassValue`                | `''`                        | Extra classes merged onto the table container                                                              |
| `zConfig`  | `ZTableConfig<T>`           | `{ data: [], columns: [] }` | The whole table — data, columns, mode, and every feature flag (see below)                                  |
| `zLoading` | `boolean`                   | `false`                     | External loading flag, ORed with `zConfig.loading`                                                         |
| `zKey`     | `string`                    | `''`                        | Cache key for persisting column visibility/order/pin/sizing via `ZCacheService`; no persistence when empty |
| `zVariant` | `'default' \| 'borderless'` | `'default'`                 | `'default'` has a card shadow container; `'borderless'` has no outer border                                |

## Outputs

| Output     | Payload                | Notes                                                                                |
| ---------- | ---------------------- | ------------------------------------------------------------------------------------ |
| `zChange`  | `ZTableChangeEvent<T>` | Single unified event for **every** interaction — discriminated by `type`. See below. |
| `zControl` | `ZTableControl<T>`     | Emitted once after init — an imperative handle for programmatic control (see below)  |

### `zChange` — discriminated by `type`

One output for everything, instead of a dozen separate `(sortChange)`/`(pageChange)`/etc. Switch on
`event.type`, one of: `'page' \| 'sort' \| 'filter' \| 'search' \| 'select' \| 'expand' \| 'rowSelect' \|
'rowSelectAll' \| 'rowExpand' \| 'rowDrag' \| 'rowInsert' \| 'rowDelete' \| 'rowCopy' \| 'rowPaste' \|
'rowMove' \| 'cellClick' \| 'cellEdit' \| 'action' \| 'bulkAction' \| 'grouping' \| 'groupExpand'`. Each
variant carries its own payload (e.g. `'page'` carries `{ pageIndex, pageSize }`, `'action'` carries
`{ key, row, rowId, action }` matching `ZTableActionClickEvent<T>`). This is how you wire server-side
mode: listen for `type === 'page' | 'sort' | 'filter' | 'search'` and refetch.

```ts
onTableChange(event: ZTableChangeEvent<User>): void {
  switch (event.type) {
    case 'page':
      this.fetchPage(event.pageIndex, event.pageSize);
      break;
    case 'action':
      if (event.key === 'edit') this.openEdit(event.row);
      break;
  }
}
```

### `zControl` — imperative handle

```ts
protected tableCtrl?: ZTableControl<User>;
```

```html
<z-table [zConfig]="tableConfig" (zControl)="tableCtrl = $event" />
```

| Method                                           | Notes                                 |
| ------------------------------------------------ | ------------------------------------- |
| `updateConfig(config: Partial<ZTableConfig<T>>)` | Patch config imperatively             |
| `toggleSettings()`                               | Open/close the column-settings drawer |
| `resetFilters()` / `resetSorting()`              | Clear filter/sort state               |
| `clearSelection()`                               | Deselect all rows                     |
| `addItem(item: T)`                               | Append a row (local mode)             |
| `updateItem(id, updates: Partial<T>, fieldKey?)` | Patch a row by id (or a custom field) |
| `deleteItem(id, fieldKey?)`                      | Remove a row by id                    |
| `getItems(): T[]`                                | Current dataset snapshot              |
| `setColumnVisibility(columnIds, visible)`        | Show/hide one or more columns         |

## `ZTableConfig<T>` — the core shape

| Field                                                                                           | Type                                                                     | Notes                                                                                                                  |
| ----------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------- |
| `data`                                                                                          | `T[]`                                                                    | Required. In `mode: 'server'`, this is just the current page                                                           |
| `columns`                                                                                       | `ZTableColumnConfig<T>[]`                                                | Required. See column config below                                                                                      |
| `mode`                                                                                          | `'local' \| 'server'`                                                    | Default `'local'`. Server mode: you handle fetch/sort/filter/paginate, table just renders and emits `zChange`          |
| `getRowId`                                                                                      | `(row, index, parent, data) => string`                                   | Custom row id — important for stable selection/expansion                                                               |
| `getSubRows`                                                                                    | `(row, index, parent, data) => T[] \| undefined`                         | Tree/hierarchical rows                                                                                                 |
| `totalCount`                                                                                    | `number`                                                                 | Server-mode total row count (for pagination)                                                                           |
| `enableRowDragging`                                                                             | `boolean`                                                                | Drag-and-drop row reordering (local mode only)                                                                         |
| `virtual`                                                                                       | `boolean \| ZTableVirtualConfig`                                         | Virtual scrolling — `true` for defaults, or a config object (row height, overscan, etc.)                               |
| `virtualColumns`                                                                                | `boolean \| ZTableVirtualColumnConfig`                                   | Horizontal column virtualization                                                                                       |
| `search`                                                                                        | `boolean \| ZTableSearchConfig`                                          | Built-in debounced search bar                                                                                          |
| `pagination`                                                                                    | `ZTablePaginationConfig`                                                 | `{ enabled, pageSize, pageIndex, pageSizeOptions, showSizeChanger, showQuickJumper, showTotal, totalLabel, disabled }` |
| `enableSettings`                                                                                | `boolean`                                                                | Column-settings drawer toggle button                                                                                   |
| `enableFilterHeader`                                                                            | `boolean`                                                                | Active-filter/sort chips row above the table                                                                           |
| `enableColumnResizing` / `enableColumnPinning` / `enableColumnOrdering` / `enableColumnSorting` | `boolean`                                                                | Global per-feature toggles (columns can still individually opt out)                                                    |
| `enableMultiSort`                                                                               | `boolean`                                                                | Default `true`                                                                                                         |
| `enableRowPinning`                                                                              | `boolean`                                                                | Pin rows to top/bottom                                                                                                 |
| `enableCellSelection`                                                                           | `boolean \| ZTableCellSelectionOptions<T>`                               | Excel-style cell/range selection + copy + context menu; default off                                                    |
| `enableContentRowInsert`                                                                        | `boolean`                                                                | Right-click "insert row" menu (only relevant with `editContent` columns)                                               |
| `enableGrouping`                                                                                | `boolean \| ZTableGroupingConfig<T>`                                     | Group rows by column value, local or server                                                                            |
| `bulkAction`                                                                                    | `boolean \| ZTableBulkActionConfig<T>`                                   | Floating action bar shown when rows are selected                                                                       |
| `expandedRowTemplate`                                                                           | `TemplateRef<{ $implicit: Row<T> }>`                                     | Custom expanded-row content                                                                                            |
| `subTableConfig`                                                                                | `ZTableConfig<unknown> \| ((row) => ZTableConfig<unknown> \| undefined)` | Render a nested `z-table` inside an expanded row                                                                       |
| `emptyTemplate` / `loadingTemplate`                                                             | `TemplateRef`                                                            | Override empty/loading states                                                                                          |
| `rowClass` / `rowStyle`                                                                         | `string \| ((row: Row<T>) => string)` etc.                               | Per-row class/style                                                                                                    |
| `width` / `height` / `maxHeight` / `minHeight`                                                  | `string`                                                                 | Fixed table dimensions                                                                                                 |
| `debounceTime`                                                                                  | `number` (ms)                                                            | For async state transitions                                                                                            |
| `useSkeleton`                                                                                   | `boolean`                                                                | Skeleton rows instead of a spinner while loading                                                                       |
| `loading`                                                                                       | `boolean`                                                                | Same effect as the `zLoading` input                                                                                    |

## `ZTableColumnConfig<T>` — per column

| Field                                                                                                          | Type                                                                   | Notes                                                                                                                                                      |
| -------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `id`                                                                                                           | `string`                                                               | Required, must be stable                                                                                                                                   |
| `type`                                                                                                         | `'data' \| 'select' \| 'expand' \| 'rowDrag' \| 'rowPin' \| 'actions'` | Semantic role; usually inferred                                                                                                                            |
| `accessorKey`                                                                                                  | `keyof T & string`                                                     | Simple property accessor                                                                                                                                   |
| `accessorFn`                                                                                                   | `(row: T) => unknown`                                                  | Derived/computed value, mutually exclusive with `accessorKey`                                                                                              |
| `header`                                                                                                       | `ZTableHeaderColumnConfig<T> \| string \| TemplateRef \| function`     | Shorthand: pass a plain string/template, or a full config object with `content`, `align`, `tooltip`, `rowSpan`/`colSpan`                                   |
| `body`                                                                                                         | `ZTableBodyColumnConfig<T> \| content`                                 | Cell config: `content`, `type: 'default' \| 'tag'`, `tagColor`, `align`, `actions`, `edit`, `editContent`, `tooltip`, `popover`, `enabledClick`, `visible` |
| `footer`                                                                                                       | `ZTableFooterConfig<T> \| ZTableFooterConfig<T>[]`                     | Footer cell(s)                                                                                                                                             |
| `size` / `minSize` / `maxSize` / `width`                                                                       | `number \| string`                                                     | Column sizing                                                                                                                                              |
| `sort`                                                                                                         | `boolean \| ZTableSortConfig<T>`                                       | `true` = local sort with defaults                                                                                                                          |
| `filter`                                                                                                       | `boolean \| ZTableFilterConfig<T>`                                     | `true` = local filter with defaults; `{ mode: 'server' }` delegates to your fetch                                                                          |
| `enableResizing` / `enablePinning` / `enableHiding` / `enableOrdering` / `enableGrouping` / `enableHeaderMenu` | `boolean`                                                              | Per-column feature overrides                                                                                                                               |
| `pinned`                                                                                                       | `'left' \| 'right' \| false`                                           | Initial pin position                                                                                                                                       |
| `columns`                                                                                                      | `ZTableColumnConfig<T>[]`                                              | Nested columns for multi-level header groups                                                                                                               |

### `body.actions` — action buttons in a cell

```ts
body: {
  actions: (row) => [
    { key: 'edit', icon: 'lucidePencil', tooltip: 'Edit', type: 'ghost' },
    { key: 'delete', icon: 'lucideTrash', tooltip: 'Delete', type: 'ghost-destructive' },
  ],
}
```

`ZTableActionItem<T>`: `key` (required), `label`, `icon` (`ZIcon`), `tooltip`, `type`/`size` (same values as
`z-button`'s `zType`/`zSize`), `loading`/`disabled`/`hidden` (static or `(row) => boolean`), `divide`.
Clicking one emits a `zChange` event with `type: 'action'`.

## Examples

```html
<!-- Basic sortable/filterable local table -->
<z-table [zConfig]="{ data: users(), columns: userColumns }" />

<!-- Server-side paging/sorting/filtering -->
<z-table
  [zConfig]="{ data: page(), columns: userColumns, mode: 'server', totalCount: total(), loading: isLoading() }"
  (zChange)="onServerChange($event)"
/>

<!-- Virtual scrolling for large datasets, persisted column layout -->
<z-table [zConfig]="{ data: bigDataset(), columns: cols, virtual: true }" zKey="orders-table" />

<!-- Row selection + bulk action bar -->
<z-table
  [zConfig]="{ data: rows(), columns: [{ id: 'select', type: 'select' }, ...cols], bulkAction: true }"
  (zChange)="onChange($event)"
/>
```

## Anti-patterns

```html
<!-- WRONG: hand-rolled table markup, no sort/filter/virtualization -->
<table>
  <tr *ngFor="let row of rows">...</tr>
</table>

<!-- WRONG: invented separate outputs — z-table has ONE unified zChange, not per-feature events -->
<z-table [zConfig]="cfg" (sortChange)="..." (pageChange)="..." />

<!-- WRONG: mutating zConfig.data in place and expecting change detection to pick it up —
     pass a new array reference, or use the zControl handle's addItem/updateItem/deleteItem -->
<script>
  tableConfig.data.push(newRow);
</script>
```
