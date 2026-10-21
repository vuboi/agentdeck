# Pagination — `z-pagination`

Selector: `z-pagination`. A self-contained pager: total label, page-size `z-select`, prev/page-number/next
`z-button-group`, and an optional quick-jump `z-input`. Never hand-roll page-number buttons or a page-size
dropdown — `z-pagination` already wires them together and emits one combined change event.

```html
<z-pagination
  [zTotal]="totalRows"
  [(zPageIndex)]="pageIndex"
  [(zPageSize)]="pageSize"
  (zOnPageChange)="onPageChange($event)"
/>
```

## Inputs

| Input              | Type                        | Default             | Notes                                                                                                                                           |
| ------------------ | --------------------------- | ------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| `zTotal`           | `number`                    | **required**        | Total item count, drives total-pages calculation                                                                                                |
| `zPageIndex`       | `model<number>`             | `1`                 | Current page, 1-based — two-way bindable with `[(zPageIndex)]`                                                                                  |
| `zPageSize`        | `model<number>`             | `10`                | Items per page — two-way bindable with `[(zPageSize)]`                                                                                          |
| `zPageSizeOptions` | `number[]`                  | `[10, 20, 50, 100]` | Options shown in the page-size `z-select`                                                                                                       |
| `zShowSizeChanger` | `boolean`                   | `true`              | Show/hide the page-size selector                                                                                                                |
| `zShowQuickJumper` | `boolean`                   | `false`             | Show/hide the "go to page" number input                                                                                                         |
| `zShowTotal`       | `boolean`                   | `true`              | Show/hide the total-count label                                                                                                                 |
| `zSimple`          | `boolean`                   | `false`             | Simple mode: just prev/next + "current / total" text, no page numbers, size changer, or jumper                                                  |
| `zSize`            | `'sm' \| 'default' \| 'lg'` | `'default'`         | Accepted but not read by any template logic or variant class in the current source — treat as reserved, do not rely on it to change visual size |
| `zDisabled`        | `boolean`                   | `false`             | Disables all interactive controls                                                                                                               |
| `zTotalLabel`      | `string \| null`            | `null`              | Override total text; supports a `{{total}}` placeholder, otherwise falls back to the i18n key `i18n_z_ui_pagination_total_label`                |
| `zPerPageLabel`    | `string \| null`            | `null`              | Override the "per page" suffix text in size options; falls back to i18n key `i18n_z_ui_pagination_per_page`                                     |
| `zGoToLabel`       | `string \| null`            | `null`              | Override the quick-jumper's leading label; falls back to i18n key `i18n_z_ui_pagination_go_to`                                                  |

## Outputs

| Output          | Payload                                   | Fires                                                                                   |
| --------------- | ----------------------------------------- | --------------------------------------------------------------------------------------- |
| `zOnPageChange` | `{ pageIndex: number; pageSize: number }` | Any time the page or page size changes (page click, prev/next, size change, quick jump) |

Note: `zPageIndex` and `zPageSize` are `model()`s, so `[(zPageIndex)]`/`[(zPageSize)]` already stay in sync
without `zOnPageChange` — use the output only when you need a single combined callback (e.g. to trigger a
data fetch).

## `zSize` caveat

`zSize` is declared as an input (`'sm' | 'default' | 'lg'`) but the component has no `.variants.ts` file and
no template binding reads it — it currently has no visual effect. Don't rely on it to produce a compact or
large pager; if you need a smaller pager, that would require a source change, not a prop you can pass today.

## Examples

```html
<!-- Full pager wired to a table -->
<z-pagination
  [zTotal]="totalRows"
  [(zPageIndex)]="pageIndex"
  [(zPageSize)]="pageSize"
  [zPageSizeOptions]="[10, 25, 50]"
  (zOnPageChange)="fetchPage($event)"
/>

<!-- Simple mode for a lightweight list -->
<z-pagination [zTotal]="totalRows" [(zPageIndex)]="pageIndex" [zSimple]="true" />

<!-- No size changer, with quick jumper -->
<z-pagination [zTotal]="totalRows" [(zPageIndex)]="pageIndex" [zShowSizeChanger]="false" [zShowQuickJumper]="true" />
```

## Anti-patterns

```html
<!-- WRONG: hand-rolled page buttons with raw z-button, no ellipsis/total/size-changer logic -->
<div class="flex gap-1">
  <button z-button zType="outline" (click)="page = page - 1">Prev</button>
  <button z-button zType="outline" (click)="page = page + 1">Next</button>
</div>

<!-- WRONG: invented prop, there is no zPageCount -->
<z-pagination [zPageCount]="10" />

<!-- WRONG: zTotal is required — omitting it leaves totalPages() undefined behavior -->
<z-pagination [(zPageIndex)]="pageIndex" />
```
