# Autocomplete — `z-autocomplete`

Selector: `z-autocomplete`. A searchable single-value input with a dropdown of options — supports local
filtering, server-side search, async/paginated loading, virtual scroll, custom option templates, and
keyboard navigation (arrows/enter/escape). Implements `ControlValueAccessor`, so it works directly with
`formControlName`/`[(ngModel)]`. Never hand-roll a search input + manual `<div>` dropdown.

```html
<z-autocomplete
  [zOptions]="[{ label: 'Apple', value: 'apple' }, { label: 'Banana', value: 'banana' }]"
  zPlaceholder="Search fruit..."
  formControlName="fruit"
/>
```

`exportAs="zAutocomplete"`.

## Inputs

| Input                | Type                                     | Default               | Notes                                                                                                                       |
| -------------------- | ---------------------------------------- | --------------------- | --------------------------------------------------------------------------------------------------------------------------- |
| `class`              | `ClassValue`                             | `''`                  | Extra classes merged with the input's variant classes                                                                       |
| `zOptions`           | `ZAutocompleteOption<T>[]`               | `[]`                  | `{ label, value, disabled?, description?, icon? }[]` — local option list                                                    |
| `zConfig`            | `Partial<ZAutocompleteConfig<T>>`        | `{}`                  | See config table below                                                                                                      |
| `zSize`              | `'sm' \| 'default' \| 'lg'`              | `'default'`           | Input height                                                                                                                |
| `zType`              | `'default' \| 'address_bar'`             | `'default'`           | `address_bar` mode fills the input as you arrow through options (browser-omnibox style) instead of leaving typed text alone |
| `zLabel`             | `string`                                 | `''`                  | Field label text above the input                                                                                            |
| `zLabelClass`        | `string`                                 | `''`                  | Extra classes on the label element                                                                                          |
| `zPlaceholder`       | `string`                                 | `''`                  | Input placeholder                                                                                                           |
| `zDisabled`          | `boolean`                                | `false`               | Disables the control (also settable via `setDisabledState` from a reactive form)                                            |
| `zReadonly`          | `boolean`                                | `false`               | Read-only; blurs immediately on focus attempt and blocks the dropdown                                                       |
| `zLoading`           | `boolean`                                | `false`               | External loading flag — shows spinner, disables interaction while true                                                      |
| `zLoadingOutline`    | `boolean`                                | `false`               | While `zLoading`, also adds a focus-ring-style outline (`z-loading-outline-active`)                                         |
| `zRequired`          | `boolean`                                | `false`               | Shows a `*` next to the label                                                                                               |
| `zPrefix`            | `ZIcon \| ''`                            | `''`                  | Leading icon inside the input                                                                                               |
| `zSuffix`            | `ZIcon \| ''`                            | `''`                  | Trailing icon (hidden while the clear button is shown)                                                                      |
| `zAllowClear`        | `boolean`                                | `true`                | Shows a clear (`×`) button when there's a value                                                                             |
| `zVirtualScroll`     | `boolean`                                | `false`               | Force virtual scrolling on for the option list (auto-enables above 50 options anyway)                                       |
| `zDynamicSize`       | `boolean`                                | `true`                | Virtual items measure their own rendered height instead of using a fixed `zOptionHeight`                                    |
| `zOptionHeight`      | `number`                                 | `36`                  | Row height in px, used for virtual scroll sizing                                                                            |
| `zDropdownMaxHeight` | `number \| null`                         | `null`                | Max dropdown height in px; falls back to `zConfig`/default (`280`)                                                          |
| `zHeightExpand`      | `number \| null`                         | `null`                | **Deprecated** — use `zDropdownMaxHeight` instead                                                                           |
| `zMaxVisible`        | `number \| null`                         | `null`                | If option count exceeds this, virtual scroll turns on automatically                                                         |
| `zResetOnSelect`     | `boolean`                                | `true`                | Clear the input text after selecting an option (`false` fills it with the option's label instead)                           |
| `zOpenOnFocus`       | `boolean`                                | `true`                | Open the dropdown on focus/click; set `false` to open only after the user starts typing                                     |
| `zEmptyText`         | `string`                                 | `''`                  | Message when a search yields no results; falls back to config/default i18n key                                              |
| `zEmptyIcon`         | `ZIcon`                                  | `'lucideSearchX'`     | Icon for the "no results" state                                                                                             |
| `zNoDataText`        | `string`                                 | `''`                  | Message when there's no data at all yet; falls back to config/default i18n key                                              |
| `zNoDataIcon`        | `ZIcon`                                  | `'lucidePackageOpen'` | Icon for the "no data" state                                                                                                |
| `zAllowCustomValue`  | `boolean`                                | `false`               | If no options match, treat the typed text itself as a selectable/enterable value                                            |
| `zDebounceTime`      | `number`                                 | `200`                 | Debounce (ms) before search/async triggers; overridable per-call via `zConfig.debounceTime`                                 |
| `zLoadingMore`       | `boolean`                                | `false`               | External "loading more" flag (pagination), shown at the bottom of the list                                                  |
| `zEnableLoadMore`    | `boolean`                                | `false`               | Enables load-more (non-async) pagination via scroll/intersection                                                            |
| `zScrollDistance`    | `number`                                 | `2`                   | Multiplier of viewport height used as the load-more trigger threshold during virtual scroll                                 |
| `zKey`               | `string`                                 | `''`                  | Property name used to dedupe/track options when `value` isn't a primitive                                                   |
| `zOptionTemplate`    | `ZAutocompleteOptionTemplate<T> \| null` | `null`                | Custom `TemplateRef<{ $implicit: option, active: boolean }>` for rendering each option row                                  |

### `zConfig` (`Partial<ZAutocompleteConfig<T>>`)

| Key                              | Type                                          | Default                                        | Notes                                                                                                                                                       |
| -------------------------------- | --------------------------------------------- | ---------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `minLength`                      | `number`                                      | `1`                                            | Min characters before searching/calling `async.load`                                                                                                        |
| `debounceTime`                   | `number`                                      | `0` (falls back to `zDebounceTime` when unset) | Overrides `zDebounceTime`                                                                                                                                   |
| `maxDisplayed`                   | `number`                                      | `0` (unlimited)                                | Caps rendered option count after filtering                                                                                                                  |
| `highlightMatch`                 | `boolean`                                     | `true`                                         | Highlights the matched substring in each label                                                                                                              |
| `emptyMessage` / `noDataMessage` | `string`                                      | i18n keys                                      | Overridden by `zEmptyText`/`zNoDataText` when set                                                                                                           |
| `closeOnSelect`                  | `boolean`                                     | `true`                                         | Close the dropdown after selecting                                                                                                                          |
| `optionHeight`                   | `number`                                      | `36`                                           | Overridden by `zOptionHeight` when set                                                                                                                      |
| `dropdownMaxHeight`              | `number`                                      | `280`                                          | Overridden by `zDropdownMaxHeight` when set                                                                                                                 |
| `serverSearch`                   | `boolean`                                     | `false`                                        | Skip local filtering; you filter server-side via `zOnSearch` and pass results back through `zOptions`                                                       |
| `async`                          | `ZAsyncOptionsConfig<ZAutocompleteOption<T>>` | —                                              | Enables built-in async load/paginate; see `@shival99/z-ui/utils` for the shape (`load`, `pageSize`, `loadOnFocus`, `dedupeBy`, selected-options resolution) |

## Outputs

| Output         | Payload                                        | Fires when                                                                                                               |
| -------------- | ---------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------ |
| `zOnSearch`    | `string`                                       | Debounced query changes (also drives `async.load` when configured)                                                       |
| `zOnLoadMore`  | `{ currentCount, scrollTop }`                  | User scrolls near the bottom with `zEnableLoadMore` (non-async pagination only — async pagination is handled internally) |
| `zOnSelect`    | `ZAutocompleteOption<T>`                       | An option is selected/clicked                                                                                            |
| `zOnEnter`     | `string`                                       | Enter pressed with no matching options and `zAllowCustomValue` true                                                      |
| `zOnCommit`    | `{ value: string; source: 'enter' \| 'blur' }` | Value is "committed" via Enter or blur                                                                                   |
| `zValueChange` | `string`                                       | The text value changes for any reason (typing, select, clear, keyboard nav)                                              |
| `zOnBlur`      | `FocusEvent`                                   | Input blurs                                                                                                              |
| `zOnFocus`     | `FocusEvent`                                   | Input focuses                                                                                                            |
| `zControl`     | `ZAutocompleteControl<T>`                      | Emitted once on init — an imperative handle (see below)                                                                  |
| `zEvent`       | `ZEvent`                                       | Generic wrapped focus/blur event, for uniform event-bus style handling                                                   |

## Public methods (template ref / `zControl`)

`focus()`, `blur()`, `clear()`, `reset()` (clear + reset touched/dirty), `open()`, `close()`, `setValue(value: string)`.
The same set (plus reactive `value`/`selected`/`state` signals) is emitted once via the `zControl` output as a
`ZAutocompleteControl<T>` object — `state` on that object is marked `@deprecated` in favor of the individual
`zOnFocus`/`zOnBlur`/etc. outputs.

### `[z-autocomplete-option]` / `[zAutocompleteOption]` directive

Apply to an `<ng-template>` inside `z-autocomplete` to supply a custom option template inline instead of via
`zOptionTemplate`. Template context: `{ $implicit: option, active: boolean }`.

## `zSize` — input height

| `zSize`   | Height        |
| --------- | ------------- |
| `sm`      | `h-8` (32px)  |
| `default` | `h-9` (36px)  |
| `lg`      | `h-10` (40px) |

## Examples

```html
<!-- Reactive forms, local filtering -->
<z-autocomplete
  formControlName="country"
  zLabel="Country"
  zPlaceholder="Search countries..."
  [zOptions]="countryOptions"
/>

<!-- Async, paginated, load on focus -->
<z-autocomplete
  [zConfig]="{ async: { load: loadUsers, loadOnFocus: true, pageSize: 20 } }"
  zPlaceholder="Search users..."
  (zOnSelect)="onUserSelected($event)"
/>

<!-- Server-side search: you own filtering, feed results back in -->
<z-autocomplete [zOptions]="serverResults()" [zConfig]="{ serverSearch: true }" (zOnSearch)="fetchFromServer($event)" />

<!-- Allow free text with no matching option -->
<z-autocomplete [zOptions]="tagOptions" [zAllowCustomValue]="true" (zOnEnter)="addTag($event)" />

<!-- Custom option row via directive template -->
<z-autocomplete [zOptions]="userOptions">
  <ng-template z-autocomplete-option let-user let-active="active">
    <img [src]="user.value.avatar" class="size-5 rounded-full" />
    <span [class.font-semibold]="active">{{ user.label }}</span>
  </ng-template>
</z-autocomplete>
```

## Anti-patterns

```html
<!-- WRONG: hand-rolled input + manual dropdown div, no keyboard nav, no CVA integration -->
<input [(ngModel)]="query" (input)="filterList()" />
@if (showDropdown) {
<div class="absolute rounded border shadow">
  @for (opt of filtered; track opt) {
  <div (click)="select(opt)">{{ opt.label }}</div>
  }
</div>
}

<!-- WRONG: invented input, zItems is not real (it's zOptions) -->
<z-autocomplete [zItems]="options" />

<!-- WRONG: guessing an output name, there is no zSelectionChange -->
<z-autocomplete (zSelectionChange)="onSelect($event)" [zOptions]="options" />
```
