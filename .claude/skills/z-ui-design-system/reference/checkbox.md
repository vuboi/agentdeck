# Checkbox — `z-checkbox`

Selector: `z-checkbox`. A button-based checkbox (not a native `<input type="checkbox">`) that supports a single
boolean checkbox or a multi-select group from `zOptions`. Implements `ControlValueAccessor`, so it works with
`[(ngModel)]` or `formControlName` — binding `ngModel` writes to `zChecked` (single mode) or `zGroupValue` (group
mode) automatically. It also exposes `[(zChecked)]` and `[(zGroupValue)]` as `model()`s for direct two-way
binding without forms.

```html
<z-checkbox zText="Accept terms and conditions" [(ngModel)]="accepted" />

<z-checkbox zType="group" zLabel="Select your favorite fruits" [zOptions]="fruitOptions" [(ngModel)]="selectedFruits" />
```

Never build a checkbox by hand with a styled `<input type="checkbox">` or a `<div>` + manual check icon — always
use `z-checkbox` so you inherit focus ring, disabled state, indeterminate state, and group/check-all logic for
free.

## Inputs

| Input            | Type                         | Default      | Notes                                                                      |
| ---------------- | ---------------------------- | ------------ | -------------------------------------------------------------------------- |
| `class`          | `ClassValue`                 | `''`         | Extra Tailwind classes, merged with variant classes                        |
| `zType`          | `'default' \| 'group'`       | `'default'`  | `'default'` = single checkbox, `'group'` = multi-select from `zOptions`    |
| `zSize`          | `'sm' \| 'default' \| 'lg'`  | `'default'`  | Box size + label text size                                                 |
| `zLabel`         | `string`                     | `''`         | Label text shown above the checkbox/group (like `z-input`'s label)         |
| `zText`          | `string`                     | `''`         | Text shown next to the checkbox (single mode only)                         |
| `zDisabled`      | `boolean`                    | `false`      | Disables the checkbox / whole group                                        |
| `zIndeterminate` | `boolean`                    | `false`      | Shows indeterminate (dash) state (single mode only)                        |
| `zValue`         | `string \| number`           | `''`         | Value associated with the checkbox (form binding metadata)                 |
| `zOptions`       | `ZCheckboxOption[]`          | `[]`         | Options for group mode: `{ label, value, disabled?, checked? }`            |
| `zOrientation`   | `'vertical' \| 'horizontal'` | `'vertical'` | Group layout direction                                                     |
| `zCheckAll`      | `boolean`                    | `false`      | Group mode only — shows a "Check All" option at the top                    |
| `zCheckAllText`  | `string`                     | `''`         | Custom label for the Check All option (falls back to a translated default) |

## Models (two-way bindable)

| Model         | Type                   | Default | Notes                                              |
| ------------- | ---------------------- | ------- | -------------------------------------------------- |
| `zChecked`    | `boolean`              | `false` | Single-mode checked state — use `[(zChecked)]`     |
| `zGroupValue` | `(string \| number)[]` | `[]`    | Group-mode selected values — use `[(zGroupValue)]` |

## Outputs

| Output         | Type                   | Notes                                                                                                                                                                      |
| -------------- | ---------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `zChange`      | `boolean`              | Emits on toggle in single mode                                                                                                                                             |
| `zGroupChange` | `(string \| number)[]` | Emits on selection change in group mode                                                                                                                                    |
| `zOnFocus`     | `FocusEvent`           | Single mode only                                                                                                                                                           |
| `zOnBlur`      | `FocusEvent`           | Single mode only                                                                                                                                                           |
| `zControl`     | `ZCheckboxControl`     | Group mode with `zCheckAll` only — emitted once after view init; gives programmatic `checkAll()`/`uncheckAll()`/`toggleAll()` and `isAllChecked`/`isIndeterminate` signals |
| `zEvent`       | `ZEvent`               | Generic focus/blur event wrapper (internal event bus)                                                                                                                      |

## `zSize` — box size

| `zSize`   | Box      | Label text  |
| --------- | -------- | ----------- |
| `sm`      | `size-4` | `text-xs`   |
| `default` | `size-5` | `text-sm`   |
| `lg`      | `size-6` | `text-base` |

There is no `zType="primary"`-style color variant — the checked/indeterminate look (`bg-primary`,
`border-primary`) is fixed via `data-[state=...]` selectors, not a variant prop.

## `ZCheckboxOption` (group mode)

| Property   | Type               | Required | Notes                                                                    |
| ---------- | ------------------ | -------- | ------------------------------------------------------------------------ |
| `label`    | `string`           | Yes      | Display text                                                             |
| `value`    | `string \| number` | Yes      | Value tracked in `zGroupValue`                                           |
| `disabled` | `boolean`          | No       | Disables just this option                                                |
| `checked`  | `boolean`          | No       | Initial checked hint (actual state is driven by `zGroupValue`/`ngModel`) |

## `ZCheckboxControl` (from `zControl` output)

| Member            | Type              | Notes                                                  |
| ----------------- | ----------------- | ------------------------------------------------------ |
| `checkAll`        | `() => void`      | Checks all enabled options                             |
| `uncheckAll`      | `() => void`      | Unchecks all enabled options                           |
| `toggleAll`       | `() => void`      | Toggles based on current all-checked state             |
| `isAllChecked`    | `Signal<boolean>` | True when every enabled option is checked              |
| `isIndeterminate` | `Signal<boolean>` | True when some but not all enabled options are checked |

## Examples

```html
<!-- Basic single checkbox with ngModel -->
<z-checkbox zText="Accept terms and conditions" [(ngModel)]="accepted" (zChange)="onChange($event)" />

<!-- Single checkbox with a label above it -->
<z-checkbox zLabel="Agreement" zText="I agree to the privacy policy" [(ngModel)]="termsAccepted" />

<!-- Sizes -->
<z-checkbox zSize="sm" zText="Small" [(ngModel)]="value" />
<z-checkbox zSize="lg" zText="Large" [(ngModel)]="value" />

<!-- Indeterminate / disabled states -->
<z-checkbox zText="Indeterminate" [zIndeterminate]="true" />
<z-checkbox zText="Disabled, checked" [zChecked]="true" [zDisabled]="true" />

<!-- Group mode with ngModel -->
<z-checkbox
  zType="group"
  zLabel="Select your favorite fruits"
  [zOptions]="fruitOptions"
  [(ngModel)]="selectedFruits"
  (zGroupChange)="onGroupChange($event)"
/>

<!-- Horizontal group -->
<z-checkbox
  zType="group"
  zLabel="Notification Settings"
  zOrientation="horizontal"
  [zOptions]="settingOptions"
  [(ngModel)]="notificationSettings"
/>

<!-- Group with Check All, using the zControl output for programmatic control -->
<z-checkbox
  zType="group"
  zLabel="Permissions"
  [zCheckAll]="true"
  zCheckAllText="Select All Permissions"
  [zOptions]="permissionOptions"
  [(ngModel)]="selectedPermissions"
  (zControl)="onControlReady($event)"
/>
```

```ts
protected readonly fruitOptions: ZCheckboxOption[] = [
  { label: 'Apple', value: 'apple' },
  { label: 'Banana', value: 'banana' },
  { label: 'Orange (Disabled)', value: 'orange', disabled: true },
];
```

## Anti-patterns

```html
<!-- WRONG: raw native checkbox, no focus ring / indeterminate handling / theme tokens -->
<input type="checkbox" [(ngModel)]="accepted" />
Accept terms

<!-- WRONG: hand-rolling a "group" out of repeated single checkboxes instead of zType="group" + zOptions -->
<z-checkbox zText="Apple" [(zChecked)]="apple" />
<z-checkbox zText="Banana" [(zChecked)]="banana" />
<z-checkbox zText="Orange" [(zChecked)]="orange" />

<!-- WRONG: invented input, zColor/zVariant are not real props -->
<z-checkbox zColor="green" zText="Confirm" />

<!-- WRONG: guessing a size that doesn't exist -->
<z-checkbox zSize="medium" zText="Confirm" />

<!-- WRONG: manually managing "check all" logic instead of zCheckAll + zControl -->
<z-checkbox zText="Select all" (zChange)="checkAllManually()" />
<z-checkbox zType="group" [zOptions]="options" [(ngModel)]="values" />
```
