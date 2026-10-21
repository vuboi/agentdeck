# Select — `z-select`

Selector: `z-select`. A combobox/dropdown select with single, multiple, and tags modes, search, async
options, virtual scroll, load-more, and `ControlValueAccessor`/`NgModel`/reactive-forms support. Never build
a dropdown by hand with a styled `<div>` + `<ul>` — always use `z-select` so you inherit keyboard nav, focus
ring, popover positioning, and form integration for free.

```html
<z-select zLabel="Country" [zOptions]="countryOptions" [(ngModel)]="selectedCountry" />

<z-select zMode="multiple" zLabel="Tags" [zOptions]="tagOptions" [zAllowClear]="true" [(ngModel)]="selectedTags" />
```

## Inputs

Core:

| Input             | Type                               | Default     | Notes                                                                            |
| ----------------- | ---------------------------------- | ----------- | -------------------------------------------------------------------------------- |
| `class`           | `ClassValue`                       | `''`        | Extra classes on the trigger, merged with variant classes                        |
| `zClassSelect`    | `ClassValue`                       | `''`        | Extra classes on the dropdown panel                                              |
| `zMode`           | `'single' \| 'multiple' \| 'tags'` | `'single'`  | Selection mode                                                                   |
| `zSize`           | `'sm' \| 'default' \| 'lg'`        | `'default'` | Trigger height                                                                   |
| `zVariant`        | `'default' \| 'ghost'`             | `'default'` | Trigger surface; `ghost` removes the bordered field treatment                    |
| `zLabel`          | `string`                           | `''`        | Field label text                                                                 |
| `zLabelClass`     | `string`                           | `''`        | Extra classes on the label                                                       |
| `zPlaceholder`    | `string`                           | `''`        | Trigger placeholder (falls back to i18n key)                                     |
| `zRequired`       | `boolean`                          | `false`     | Marks required, drives the built-in "required" validator                         |
| `zDisabled`       | `boolean`                          | `false`     | Disables the control                                                             |
| `zReadonly`       | `boolean`                          | `false`     | Read-only visual state, blocks interaction                                       |
| `zLoading`        | `boolean`                          | `false`     | Shows a spinner and blocks interaction                                           |
| `zLoadingOutline` | `boolean`                          | `false`     | Renders `zLoading` as a focus-ring-style outline instead of blocking             |
| `zPrefix`         | `ZIcon \| ''`                      | `''`        | Leading icon on the trigger                                                      |
| `zAllowClear`     | `boolean`                          | `false`     | Shows a clear ("x") action when there is a value                                 |
| `zShowCheck`      | `boolean`                          | `true`      | Show the check icon next to the selected option(s)                               |
| `zWrap`           | `boolean`                          | `true`      | Multi-mode: wrap tags to multiple lines vs. single-line truncate                 |
| `zPosition`       | `ZPopoverPosition`                 | `'bottom'`  | Dropdown popover position                                                        |
| `zDropdownWidth`  | `'auto' \| 'trigger' \| number`    | `'trigger'` | Match the trigger width, use intrinsic width, or set a fixed px width            |
| `zTree`           | `boolean`                          | `false`     | Treat options with `children` as drill-down levels instead of selectable parents |
| `zTreeRootLabel`  | `string`                           | `''`        | Label for the root breadcrumb in tree mode (falls back to i18n)                  |

Options and search:

| Input                                         | Type                          | Default                       | Notes                                                                        |
| --------------------------------------------- | ----------------------------- | ----------------------------- | ---------------------------------------------------------------------------- |
| `zOptions`                                    | `ZSelectOption<T>[]`          | `[]`                          | Static option list (`{ label, value, disabled?, group?, icon?, children? }`) |
| `zShowSearch`                                 | `boolean`                     | `true`                        | Show the search input in the dropdown                                        |
| `zPlaceholderSearch`                          | `string`                      | `''`                          | Search input placeholder                                                     |
| `zDebounce`                                   | `number`                      | `300`                         | Search debounce (ms); overridden by `zConfig.debounceTime`                   |
| `zTranslateLabels`                            | `boolean`                     | `true`                        | Run option labels through `ZTranslateService`                                |
| `zKey`                                        | `string`                      | `''`                          | Object key used to extract/compare values when `value` is an object          |
| `zNotFoundText` / `zEmptyText` / `zEmptyIcon` | `string` / `string` / `ZIcon` | `''` / `''` / `'lucideInbox'` | Empty-state copy for "no search results" vs. "no options at all"             |
| `zConfig`                                     | `Partial<ZSelectConfig<T>>`   | `{}`                          | `{ minLength, debounceTime, async }` — see async below                       |

Async / server-driven options:

| Input             | Type      | Default | Notes                                                                            |
| ----------------- | --------- | ------- | -------------------------------------------------------------------------------- |
| `zSearchServer`   | `boolean` | `false` | Search filtering happens server-side; component won't filter locally             |
| `zLoadingMore`    | `boolean` | `false` | External "loading more" flag (combined with internal async state)                |
| `zEnableLoadMore` | `boolean` | `false` | Enables `zOnLoadMore` for non-async (manually paginated) option lists            |
| `zScrollDistance` | `number`  | `2`     | Viewport-heights-from-bottom threshold that triggers load-more in virtual scroll |

Multi-select tag display:

| Input          | Type                       | Default | Notes                                                                                                                                                              |
| -------------- | -------------------------- | ------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `zMaxTagCount` | `number \| null`           | `null`  | Max tags shown before collapsing into a `+N` badge; `null` keeps all selected tags visible                                                                         |
| `zTagWidth`    | `number \| string \| null` | `null`  | Width applied to every selected tag; numbers are pixels, `null` fits content, and the nowrap tag crossing the trigger's right clipping edge shrinks and ellipsizes |

Dropdown/list behavior:

| Input                | Type             | Default | Notes                                                                   |
| -------------------- | ---------------- | ------- | ----------------------------------------------------------------------- |
| `zDropdownMaxHeight` | `number`         | `256`   | Max px height of the options panel                                      |
| `zOptionHeight`      | `number`         | `32`    | Row height (px), used for virtual scroll sizing                         |
| `zVirtualScroll`     | `boolean`        | `false` | Force virtual scrolling on                                              |
| `zDynamicSize`       | `boolean`        | `false` | Let virtualized rows measure their own (variable) height                |
| `zMaxVisible`        | `number \| null` | `null`  | Auto-enable virtual scroll once option count exceeds this               |
| `zShowAction`        | `boolean`        | `false` | Show the footer action bar (select-all/clear-all, or `zActionTemplate`) |
| `zScrollClose`       | `boolean`        | `false` | Close dropdown on outer scroll (passed to the popover)                  |
| `zSticky`            | `boolean`        | `false` | Sticky popover positioning (passed to the popover)                      |

Templates:

| Input               | Type                               | Default | Notes                                                                                                   |
| ------------------- | ---------------------------------- | ------- | ------------------------------------------------------------------------------------------------------- |
| `zSelectedTemplate` | `ZSelectLabelTemplate<T> \| null`  | `null`  | Custom render for the selected value/tag (or use `[z-select-selected]` directive on an `<ng-template>`) |
| `zOptionTemplate`   | `ZSelectOptionTemplate<T> \| null` | `null`  | Custom render for each option row (or use `[z-select-option]` directive)                                |
| `zActionTemplate`   | `TemplateRef<unknown> \| null`     | `null`  | Custom footer action content, shown when `zShowAction` is true                                          |

Validation:

| Input              | Type                         | Default    | Notes                                                     |
| ------------------ | ---------------------------- | ---------- | --------------------------------------------------------- |
| `zValidators`      | `ZSelectValidator<T>[]`      | `[]`       | Sync validators: `{ error, message, validate(selected) }` |
| `zAsyncValidators` | `ZSelectAsyncValidator<T>[]` | `[]`       | Async validators, debounced by `zAsyncDebounce`           |
| `zAsyncDebounce`   | `number`                     | `300`      | Debounce (ms) before running async validators             |
| `zAsyncValidateOn` | `'change' \| 'blur'`         | `'change'` | When async validation runs                                |

## Outputs

| Output        | Payload                                                | Fires when                                                                                                                                                 |
| ------------- | ------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `zOnSearch`   | `string`                                               | Debounced search text changes                                                                                                                              |
| `zOnLoadMore` | `ZSelectLoadMoreEvent` (`{ currentCount, scrollTop }`) | User scrolls near the end and `zEnableLoadMore`/async has-more is true                                                                                     |
| `zOnBlur`     | `FocusEvent`                                           | Dropdown closes / trigger blurs                                                                                                                            |
| `zOnFocus`    | `FocusEvent`                                           | Dropdown opens                                                                                                                                             |
| `zControl`    | `ZSelectControl<T>`                                    | Once on init — emits an imperative handle (`valid()`, `reset()`, `focus()`, `setErrorMessage()`, signals for `hasError`/`isOpen`/`value`/`selected`, etc.) |
| `zEvent`      | `ZEvent`                                               | Generic wrapped focus/blur event stream                                                                                                                    |

`exportAs: 'zSelect'` — use `#sel="zSelect"` on the template ref instead of the `zControl` output when you
just need a template-local handle.

## Content projection

`[z-select-selected]` / `[zSelectSelected]` directive on an `<ng-template let-option>` — custom rendering of
the selected value(s), alternative to the `zSelectedTemplate` input.

`[z-select-option]` / `[zSelectOption]` directive on an `<ng-template let-option let-selected="selected">` —
custom rendering of each dropdown row, alternative to the `zOptionTemplate` input.

### Highlighting search text in a custom option template

The built-in option renderer highlights the current query, but a custom `zOptionTemplate` owns its markup. Import
`ZHighlightPipe` and `ZSafeHtmlPipe` from `@shival99/z-ui/pipes`, bind `zOnSearch` to a signal, and compose the
pipes in the projected template:

```ts
import { ZHighlightPipe, ZSafeHtmlPipe } from '@shival99/z-ui/pipes';

// Add both standalone pipes to the consuming component's `imports`.
```

```html
<z-select
  [zOptions]="userOptions"
  [zOptionTemplate]="userOptionTpl"
  (zOnSearch)="searchText.set($event)"
  [(ngModel)]="selectedUser"
/>

<ng-template #userOptionTpl let-option>
  <span [innerHTML]="option.label | zHighlight: searchText() | zSafeHtml"></span>
</ng-template>
```

`zOnSearch` is debounced. `zHighlight` preserves the option label and wraps the first matching term with a
`span`; `zSafeHtml` sanitizes the resulting HTML before `[innerHTML]` renders it.

## `zSize` — trigger height

| `zSize`   | Min height        | Use for                         |
| --------- | ----------------- | ------------------------------- |
| `sm`      | `min-h-8` (32px)  | Compact/dense forms             |
| `default` | `min-h-9` (36px)  | Standard forms — default choice |
| `lg`      | `min-h-10` (40px) | Prominent form fields           |

Only 3 sizes exist — do not pass `zSize="xs"` or `"xl"`, which `z-button` has but `z-select` does not.

## Tree mode

Set `zTree` when the options contain nested `children`. A parent option drills into the next level instead of
being selected; leaf options are selected normally. The dropdown renders a breadcrumb with a back action, and
`zTreeRootLabel` customizes its root label.

```html
<z-select
  zLabel="Location"
  [zTree]="true"
  zTreeRootLabel="All locations"
  [zOptions]="[
    { label: 'Europe', value: 'europe', children: [
      { label: 'Germany', value: 'germany', children: [{ label: 'Berlin', value: 'berlin' }] }
    ] },
    { label: 'Asia', value: 'asia', children: [{ label: 'Japan', value: 'japan' }] }
  ]"
  [(ngModel)]="location"
/>
```

Tree navigation is a single-select interaction; keep parent values out of the submitted model when they are only
navigation nodes.

## Examples

```html
<!-- Basic single select -->
<z-select zLabel="Status" [zOptions]="statusOptions" [(ngModel)]="status" />

<!-- Multi-select with search off and clear -->
<z-select zMode="multiple" [zShowSearch]="false" [zAllowClear]="true" [zOptions]="roleOptions" [(ngModel)]="roles" />

<!-- Async, server-searched options -->
<z-select
  zLabel="Assignee"
  [zSearchServer]="true"
  [zConfig]="{ minLength: 2, async: { load: loadUsers } }"
  [(ngModel)]="assigneeId"
/>

<!-- Nested options with drill-down navigation -->
<z-select zLabel="Location" [zTree]="true" [zOptions]="locationOptions" [(ngModel)]="location" />

<!-- Reading the imperative control -->
<z-select #sel="zSelect" [zOptions]="opts" [zRequired]="true" [(ngModel)]="value" />
<button z-button (click)="sel.valid() ? save() : null">Save</button>

<!-- Custom option row -->
<z-select [zOptions]="userOptions" [(ngModel)]="userId">
  <ng-template z-select-option let-user let-selected="selected">
    <img [src]="user.icon" class="size-4 rounded-full" />
    <span>{{ user.label }}</span>
  </ng-template>
</z-select>
```

## Anti-patterns

```html
<!-- WRONG: native select, no theming, no search/multi/async support -->
<select [(ngModel)]="status">
  <option value="active">Active</option>
</select>

<!-- WRONG: invented input, there is no zColor -->
<z-select zColor="primary" [zOptions]="opts"></z-select>

<!-- WRONG: guessing a size that doesn't exist on z-select -->
<z-select zSize="xs" [zOptions]="opts"></z-select>

<!-- WRONG: rolling your own multi-value array push/splice instead of zMode -->
<z-select [zOptions]="opts" (zOnSearch)="manuallyPushToArray($event)"></z-select>
<!-- RIGHT: -->
<z-select zMode="multiple" [zOptions]="opts" [(ngModel)]="values"></z-select>
```
