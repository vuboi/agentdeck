# Radio — `z-radio`

Selector: `z-radio`. Implements `ControlValueAccessor` — works with `[(ngModel)]` and reactive
`formControlName`/`[formControl]`. Renders either a single radio button (`zType="default"`) or a whole
option group (`zType="group"`) from one component; never hand-roll a set of `<input type="radio">` +
`<label>` pairs.

```html
<!-- Single radio -->
<z-radio zText="Enable notifications" zValue="on" [(ngModel)]="notifPref" />

<!-- Group of options -->
<z-radio zType="group" zLabel="Plan" [zOptions]="planOptions" [(ngModel)]="plan" />
```

## Inputs

| Input            | Type                              | Default      | Notes                                                                                                       |
| ---------------- | --------------------------------- | ------------ | ----------------------------------------------------------------------------------------------------------- |
| `class`          | `ClassValue`                      | `''`         | Extra classes merged onto the host                                                                          |
| `zType`          | `'default' \| 'group'`            | `'default'`  | `'default'` = one radio button; `'group'` = renders `zOptions` as a set                                     |
| `zSize`          | `'sm' \| 'default' \| 'lg'`       | `'default'`  | Radio dot + label size                                                                                      |
| `zLabel`         | `string`                          | `''`         | Small label rendered above the radio/group (both types)                                                     |
| `zText`          | `string`                          | `''`         | Inline text next to the control — only used in `zType="default"`                                            |
| `zValue`         | `string \| number`                | `''`         | The value this single radio represents — only used in `zType="default"`                                     |
| `zOptions`       | `ZRadioOption[]`                  | `[]`         | `{ label, value, disabled? }[]` — only used in `zType="group"`                                              |
| `zOrientation`   | `'vertical' \| 'horizontal'`      | `'vertical'` | Layout of options — only used in `zType="group"`                                                            |
| `zDisabled`      | `boolean`                         | `false`      | Disables the whole control (individual options can also set their own `disabled`)                           |
| `zSelectedValue` | `model<string \| number \| null>` | `null`       | Two-way bindable selected value — use `[(zSelectedValue)]` as an alternative to `ngModel`/`formControlName` |

## Outputs

| Output     | Payload                    | Fires                                                                      |
| ---------- | -------------------------- | -------------------------------------------------------------------------- |
| `zChange`  | `string \| number \| null` | Selected value changes (click on the radio, or on an option in group mode) |
| `zOnFocus` | `FocusEvent`               | Radio button gains focus                                                   |
| `zOnBlur`  | `FocusEvent`               | Radio button loses focus                                                   |
| `zEvent`   | `ZEvent`                   | Generic event bus wrapping focus/blur                                      |

## `zSize` — dot and label scale

| `zSize`   | Radio    | Indicator dot | Label text  |
| --------- | -------- | ------------- | ----------- |
| `sm`      | `size-4` | `size-2`      | `text-xs`   |
| `default` | `size-5` | `size-2.5`    | `text-sm`   |
| `lg`      | `size-6` | `size-3`      | `text-base` |

## Examples

```html
<!-- Reactive forms, single radio -->
<z-radio zText="I agree to the terms" zValue="agreed" formControlName="agreement" />

<!-- Group with disabled option -->
<z-radio
  zType="group"
  zLabel="Shipping method"
  [zOptions]="[
    { label: 'Standard', value: 'standard' },
    { label: 'Express', value: 'express' },
    { label: 'Same day', value: 'same-day', disabled: true },
  ]"
  [(ngModel)]="shipping"
/>

<!-- Horizontal compact group -->
<z-radio zType="group" zSize="sm" zOrientation="horizontal" [zOptions]="sizeOptions" [(ngModel)]="size" />
```

## Anti-patterns

```html
<!-- WRONG: raw radio inputs, no CVA integration, no consistent focus ring/theming -->
<label>
  <input type="radio" name="plan" value="pro" />
  Pro
</label>
<label>
  <input type="radio" name="plan" value="team" />
  Team
</label>

<!-- WRONG: invented prop, there is no zItems on z-radio -->
<z-radio zType="group" [zItems]="planOptions" />

<!-- WRONG: mixing single-radio inputs into group mode -->
<z-radio zType="group" zText="Pro plan" zValue="pro" />
<!-- RIGHT -->
<z-radio zType="group" [zOptions]="[{ label: 'Pro plan', value: 'pro' }]" />
```
