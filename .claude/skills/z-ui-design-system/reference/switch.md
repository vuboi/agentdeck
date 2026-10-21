# Switch — `z-switch`

Selector: `z-switch`. A toggle control implementing `ControlValueAccessor` (works with `formControlName` /
`ngModel`) or standalone via `[(zChecked)]`. Never build a toggle with a styled `<input type="checkbox">`.

```html
<z-switch [(zChecked)]="notificationsEnabled" zText="Enable notifications" />
```

## Inputs

| Input            | Type                        | Default     | Notes                                                                                                   |
| ---------------- | --------------------------- | ----------- | ------------------------------------------------------------------------------------------------------- |
| `class`          | `ClassValue`                | `''`        | Extra classes on the host wrapper                                                                       |
| `zSwitchClass`   | `ClassValue`                | `''`        | Extra classes merged onto the track (`<button role="switch">`)                                          |
| `zThumbClass`    | `ClassValue`                | `''`        | Extra classes merged onto the thumb                                                                     |
| `zSize`          | `'sm' \| 'default' \| 'lg'` | `'default'` | Track/thumb dimensions                                                                                  |
| `zMode`          | `'default' \| 'inner'`      | `'default'` | `'inner'` shows `zCheckedText`/`zUncheckedText` inside the track itself                                 |
| `zLabel`         | `string`                    | `''`        | Text rendered **above** the switch as a block `<label>` (switches the host layout to `flex-col`)        |
| `zText`          | `string`                    | `''`        | Inline text next to the switch (position controlled by `zTextPosition`); clicking it toggles the switch |
| `zTextPosition`  | `'left' \| 'right'`         | `'right'`   | Side `zText` renders on                                                                                 |
| `zCheckedText`   | `string`                    | `'ON'`      | Inner text shown when checked, only in `zMode="inner"` (truncated at 8 chars)                           |
| `zUncheckedText` | `string`                    | `'OFF'`     | Inner text shown when unchecked, only in `zMode="inner"`                                                |
| `zCheckedIcon`   | `ZIcon \| null`             | `null`      | Icon rendered on the thumb when checked                                                                 |
| `zUncheckedIcon` | `ZIcon \| null`             | `null`      | Icon rendered on the thumb when unchecked                                                               |
| `zDisabled`      | `boolean`                   | `false`     | Disabled state (also settable via `ControlValueAccessor.setDisabledState`)                              |
| `zLoading`       | `boolean`                   | `false`     | Shows a spinner on the thumb and forces disabled                                                        |
| `zWave`          | `boolean`                   | `true`      | Click ripple effect on the track                                                                        |
| `zChecked`       | `model<boolean>`            | `false`     | Two-way bindable checked state — use `[(zChecked)]`                                                     |

## Outputs

| Output     | Payload      | Fires when                                                                      |
| ---------- | ------------ | ------------------------------------------------------------------------------- |
| `zChange`  | `boolean`    | The value changes via user click (not on programmatic `writeValue`)             |
| `zOnFocus` | `FocusEvent` | Track receives focus                                                            |
| `zOnBlur`  | `FocusEvent` | Track loses focus                                                               |
| `zEvent`   | `ZEvent`     | Unified focus/blur event wrapper (see other z-ui components for `ZEvent` shape) |

## `zSize` — track/thumb dimensions

| `zSize`   | Track (default mode) | Thumb    |
| --------- | -------------------- | -------- |
| `sm`      | `h-5 w-9`            | `size-4` |
| `default` | `h-6 w-11`           | `size-5` |
| `lg`      | `h-7 w-14`           | `size-6` |

`zMode="inner"` overrides the track to a slightly wider fixed size (e.g. `min-w-[3.375rem] h-[1.625rem]` for
`default`) to make room for the inner ON/OFF text.

## Examples

```html
<!-- Basic reactive form control -->
<z-switch formControlName="isActive" />

<!-- With trailing label text -->
<z-switch [(zChecked)]="darkMode" zText="Dark mode" />

<!-- Inner ON/OFF text mode -->
<z-switch [(zChecked)]="autoSave" zMode="inner" zCheckedText="ON" zUncheckedText="OFF" />

<!-- Loading state while an async toggle request is in flight -->
<z-switch [(zChecked)]="published" [zLoading]="isSaving()" (zChange)="onPublishToggle($event)" />

<!-- Small size with a label above -->
<z-switch [(zChecked)]="compact" zSize="sm" zLabel="Compact layout" />
```

## Anti-patterns

```html
<!-- WRONG: hand-rolled checkbox styled as a toggle -->
<input type="checkbox" class="h-5 w-10 appearance-none rounded-full bg-gray-300 checked:bg-blue-500" />

<!-- WRONG: invented input, zColor is not a real prop -->
<z-switch zColor="green" [(zChecked)]="active" />

<!-- WRONG: guessing a size that doesn't exist -->
<z-switch zSize="xs" [(zChecked)]="active" />
```
