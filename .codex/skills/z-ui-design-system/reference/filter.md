# Filter — `z-filter`

Selector: `z-filter`. A field-driven filter-bar builder: pass a `zFields` config (labels, types, options) and
a `zFilters` array of active filters, and it renders an "Add filter" popover plus one pill per active filter
with a field label, an operator dropdown, and a type-appropriate value editor (select, multiselect, date/date
range, time, datetime, text, number/number range, boolean switch, or a fully custom renderer). State is
fully controlled — the component never mutates `zFilters` itself, it only emits change events. Never hand-roll
a row of `<select>`s and text inputs for tabular filtering.

```html
<z-filter
  [zFields]="[
    { key: 'status', label: 'Status', type: 'select', options: statusOptions },
    { key: 'createdAt', label: 'Created', type: 'daterange' },
  ]"
  [zFilters]="filters()"
  (zFiltersChange)="filters.set($event)"
/>
```

## Inputs

| Input            | Type                         | Default        | Notes                                                                                                                       |
| ---------------- | ---------------------------- | -------------- | --------------------------------------------------------------------------------------------------------------------------- |
| `class`          | `ClassValue`                 | `''`           | Extra classes merged with variant classes                                                                                   |
| `zFilters`       | `ZFilter<T>[]`               | **required**   | Controlled array of active filters — the component only emits changes, you own the state                                    |
| `zFields`        | `ZFilterFieldsConfig<T>`     | **required**   | Flat `ZFilterFieldConfig<T>[]` or grouped `ZFilterFieldGroup<T>[]` describing filterable fields                             |
| `zSize`          | `'sm' \| 'md' \| 'lg'`       | `'md'`         | Pill height + text/padding scale (see table below)                                                                          |
| `zI18n`          | `Partial<ZFilterI18nConfig>` | `{}`           | Overrides merged onto `DEFAULT_FILTER_I18N` (operator labels, placeholders, validation messages)                            |
| `zShowAddButton` | `boolean`                    | `true`         | Show/hide the "Add filter" trigger button                                                                                   |
| `zAllowMultiple` | `boolean`                    | `true`         | Allow the same field to be filtered more than once; when `false`, already-used fields are excluded from the add-filter list |
| `zAddButtonIcon` | `ZIcon`                      | `'lucidePlus'` | Icon on the add-filter button                                                                                               |
| `zAddButtonText` | `string`                     | `undefined`    | Add-filter button label; falls back to `i18n.addFilter` ("Add filter")                                                      |

## Outputs

| Output           | Payload                                                | Fires when                                                                                           |
| ---------------- | ------------------------------------------------------ | ---------------------------------------------------------------------------------------------------- |
| `zFiltersChange` | `ZFilter<T>[]`                                         | Any change to the filters array (add/remove/update) — bind this to update your controlled `zFilters` |
| `zFilterAdd`     | `ZFilter<T>`                                           | A new filter is added via the add-filter popover                                                     |
| `zFilterRemove`  | `ZFilter<T>`                                           | A filter pill's remove (×) button is clicked                                                         |
| `zFilterUpdate`  | `{ filter: ZFilter<T>; updates: Partial<ZFilter<T>> }` | An existing filter's operator or value(s) change                                                     |

## Content projection

| Slot        | Selector                                        | Purpose                                                  |
| ----------- | ----------------------------------------------- | -------------------------------------------------------- |
| `addButton` | `<ng-template #addButton>` (via `contentChild`) | Replace the default "Add filter" trigger button entirely |

## `ZFilter<T>` and `ZFilterFieldConfig<T>` — the data model

```ts
interface ZFilter<T = unknown> {
  id: string;
  field: string;
  operator: string;
  values: T[];
}
```

`ZFilterFieldConfig<T>` (key fields): `key`, `label`, `icon?`, `type?: ZFilterFieldType`, `group?`,
`options?: ZFilterOption<T>[]`, `operators?: ZFilterOperator[]` (override the built-in operator set for this
field), `customRenderer?: TemplateRef<ZFilterCustomRendererContext<T>>`, `placeholder?`, `min?`/`max?`/`step?`
(number fields), `pattern?`, `allowCustomValues?`, `defaultOperator?`.

`ZFilterFieldType` union: `'select' | 'multiselect' | 'date' | 'daterange' | 'text' | 'number' | 'numberrange'
| 'boolean' | 'email' | 'url' | 'tel' | 'time' | 'datetime' | 'custom' | 'separator'`.

Use the `createFilter(field, operator?, values?)` helper (exported from the package) to build a `ZFilter<T>`
by hand, e.g. for a "clear all" or programmatic-add action.

## `zSize` — pill height + padding

From `z-filter.variants.ts` (several `cva` calls share these variant keys — container gap, item height, field
label, value input, remove button, "between" separator):

| `zSize`        | Item height | Container gap | Field label      | Remove button padding |
| -------------- | ----------- | ------------- | ---------------- | --------------------- |
| `sm`           | `h-8`       | `gap-1.5`     | `px-2 text-xs`   | `px-1.5`              |
| `md` (default) | `h-9`       | `gap-2`       | `px-2.5 text-sm` | `px-2`                |
| `lg`           | `h-10`      | `gap-2.5`     | `px-3 text-sm`   | `px-2.5`              |

There is no `zType`/visual-style variant — every filter pill uses the same neutral
`border-input bg-background` styling; only density changes with `zSize`.

## Examples

```html
<!-- Basic controlled filter bar -->
<z-filter [zFields]="fields" [zFilters]="filters()" (zFiltersChange)="filters.set($event)" />

<!-- Grouped fields, single-use-per-field, custom i18n -->
<z-filter
  [zFields]="[
    { group: 'People', fields: [{ key: 'owner', label: 'Owner', type: 'select', options: userOptions }] },
    { group: 'Dates', fields: [{ key: 'dueDate', label: 'Due date', type: 'date' }] },
  ]"
  [zFilters]="filters()"
  [zAllowMultiple]="false"
  [zI18n]="{ addFilter: 'Filter by...' }"
  (zFiltersChange)="filters.set($event)"
  (zFilterRemove)="onFilterRemoved($event)"
/>

<!-- Custom add-button trigger -->
<z-filter [zFields]="fields" [zFilters]="filters()" (zFiltersChange)="filters.set($event)">
  <ng-template #addButton>
    <button z-button zType="ghost" zSize="sm">
      <z-icon zType="lucideFilter" />
      Filter
    </button>
  </ng-template>
</z-filter>
```

```ts
// Programmatic add using the exported helper
import { createFilter } from '@shival99/z-ui/components/z-filter';

this.filters.update(list => [...list, createFilter('status', 'is', ['active'])]);
```

## Anti-patterns

```html
<!-- WRONG: hand-rolled filter row, no operator system, no per-type value editors -->
<select [(ngModel)]="statusFilter">
  <option value="active">Active</option>
</select>

<!-- WRONG: invented input; the component is controlled, it never accepts a two-way [(zFilters)] model -->
<z-filter [(zFilters)]="filters" [zFields]="fields" />

<!-- WRONG: mutating the filters array in place instead of emitting a new one -->
<z-filter
  [zFields]="fields"
  [zFilters]="filters()"
  (zFilterUpdate)="filters().find(f => f.id === $event.filter.id).values = $event.updates.values"
/>
```

```html
<!-- RIGHT: treat zFilters as controlled state, always replace it via zFiltersChange -->
<z-filter [zFields]="fields" [zFilters]="filters()" (zFiltersChange)="filters.set($event)" />
```
