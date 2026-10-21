# Input — `z-input`

Selector: `z-input`. Implements `ControlValueAccessor` — works with both `[(ngModel)]` and reactive
`formControlName`/`[formControl]`. Never write a raw `<input>` or `<textarea>` for a form field; `z-input`
covers text, number, password, email, tel, url, color, and textarea through one component.

```html
<z-input zLabel="Email" zType="email" [zRequired]="true" [(ngModel)]="email" />
<z-input zLabel="Notes" zType="textarea" formControlName="notes" [zRows]="4" />
```

## Core inputs

| Input                 | Type                                                                                     | Default     | Notes                                                                             |
| --------------------- | ---------------------------------------------------------------------------------------- | ----------- | --------------------------------------------------------------------------------- |
| `zType`               | `'text' \| 'number' \| 'password' \| 'email' \| 'tel' \| 'url' \| 'color' \| 'textarea'` | `'text'`    | Drives which native control + affordances render                                  |
| `zSize`               | `'sm' \| 'default' \| 'lg'`                                                              | `'default'` | See size table                                                                    |
| `zAlign`              | `'left' \| 'center' \| 'right'`                                                          | `'left'`    | Text alignment inside the field                                                   |
| `zLabel`              | `string`                                                                                 | `''`        | Field label                                                                       |
| `zLabelClass`         | `string`                                                                                 | `''`        | Extra classes merged onto the label element                                       |
| `zPlaceholder`        | `string`                                                                                 | `''`        | Placeholder text                                                                  |
| `zRequired`           | `boolean`                                                                                | `false`     | Marks required, drives built-in "required" validation message                     |
| `zDisabled`           | `boolean`                                                                                | `false`     | Disabled state                                                                    |
| `zReadonly`           | `boolean`                                                                                | `false`     | Read-only state (visually distinct from disabled)                                 |
| `zLoading`            | `boolean`                                                                                | `false`     | Shows a loading indicator (e.g. during async validation)                          |
| `zLoadingOutline`     | `boolean`                                                                                | `false`     | Renders `zLoading` as a focus-ring-style outline instead of the default indicator |
| `zPrefix` / `zSuffix` | `string \| TemplateRef<unknown>`                                                         | `''`        | Leading/trailing slot — plain text or a template                                  |
| `zValidators`         | `ZInputValidator[]`                                                                      | `[]`        | Sync validators: `{ error, message, pattern?, min?, max?, validate? }`            |
| `zAsyncValidators`    | `ZAsyncValidator[]`                                                                      | `[]`        | Async validators returning `Promise<boolean> \| Observable<boolean>`              |
| `zAsyncDebounce`      | `number`                                                                                 | `300`       | Debounce (ms) before running async validators                                     |
| `zAsyncValidateOn`    | `'change' \| 'blur'`                                                                     | `'change'`  | When async validators run                                                         |
| `zAllowClear`         | `boolean`                                                                                | `false`     | Shows a clear (×) button once there's a value                                     |
| `zAutofocus`          | `boolean`                                                                                | `false`     | Focus on init                                                                     |
| `zAutoComplete`       | `string`                                                                                 | `'off'`     | Native `autocomplete` attribute passed through                                    |

Type-specific:

| Input                                                      | Applies to | Notes                                                                                     |
| ---------------------------------------------------------- | ---------- | ----------------------------------------------------------------------------------------- |
| `zMin` / `zMax` / `zStep`                                  | `number`   | Bounds and increment                                                                      |
| `zShowArrows`                                              | `number`   | Show up/down steppers, default `true`                                                     |
| `zAllowNegative`                                           | `number`   | Allow values below `0`, default `false`                                                   |
| `zDecimalPlaces` / `zThousandSeparator` / `zDecimalMarker` | `number`   | Formatting via `ngx-mask`                                                                 |
| `zMask`                                                    | any        | Raw ngx-mask pattern, overrides auto-derived number mask                                  |
| `zShowPasswordToggle`                                      | `password` | Show/hide toggle button, default `true`                                                   |
| `zSearch`                                                  | `text`     | Emits debounced `zOnSearch`, shows a search prefix icon                                   |
| `zAutoSuggest`                                             | `text`     | Inline ghost-text suggestion string shown ahead of the caret; `undefined`/`''` shows none |
| `zRows` / `zAutoSizeContent` / `zResize` / `zMaxLength`    | `textarea` | Row count / auto-grow / user resize / char cap                                            |
| `zColorConfig`                                             | `color`    | `{ mode: 'blossom' \| 'picker', colors?, savedColors?, ... }` picker configuration        |

## Outputs

| Output                 | Payload                    | Fires                                                                        |
| ---------------------- | -------------------------- | ---------------------------------------------------------------------------- |
| `zOnChange`            | `string \| number \| null` | Debounced value change (`zDebounce`, default 300ms)                          |
| `zOnSearch`            | `string \| number \| null` | Debounced, only when `zSearch` is true                                       |
| `zOnEnter`             | `string \| number \| null` | Enter key pressed                                                            |
| `zOnBlur` / `zOnFocus` | `FocusEvent`               | Native focus/blur                                                            |
| `zOnKeydown`           | `KeyboardEvent`            | Every keydown                                                                |
| `zControl`             | `ZInputControl`            | Emits an imperative handle once on init — see below                          |
| `zEvent`               | `ZEvent`                   | Generic event bus (blur/focus/keydown wrapped)                               |
| `zOnColorCollapse`     | `BlossomColorPickerColor`  | `color` type, `mode: 'blossom'` only — fires when the picker panel collapses |

## Imperative control (`zControl`)

```html
<z-input #email="zInput" zLabel="Email" [zRequired]="true" [(ngModel)]="value" />
```

```ts
@ViewChild('email') emailInput!: ZInputComponent;
// or capture via (zControl)="ctrl = $event" for: valid(), reset(), focus(), blur(),
// setValue(v), setErrorMessage(msg), hasError (Signal<boolean>), value (Signal<...>), errorMessage (Signal<string>)
```

Use `zControl.valid()` for boolean submit checks — `validate()` is deprecated in favor of it.

## `zSize` — field height

| `zSize`   | Height        | Text size | Use for                                                |
| --------- | ------------- | --------- | ------------------------------------------------------ |
| `sm`      | `h-8` (32px)  | `text-xs` | Compact filter bars, inline table editing              |
| `default` | `h-9` (36px)  | `text-sm` | Standard forms — default choice                        |
| `lg`      | `h-10` (40px) | `text-sm` | Prominent single-field forms (search hero, auth forms) |

Match `z-input`'s `zSize` to the `zSize` of any `z-button` in the same row (e.g. a search bar with a submit
button) so heights align.

## Examples

```html
<!-- Required field with validators -->
<z-input
  zLabel="Username"
  [zRequired]="true"
  [zValidators]="[{ error: 'minlength', message: 'Min 3 characters', validate: v => String(v ?? '').length >= 3 }]"
  [(ngModel)]="username"
/>

<!-- Number with formatting -->
<z-input zType="number" zLabel="Price" [zMin]="0" zThousandSeparator="," [zDecimalPlaces]="2" [(ngModel)]="price" />

<!-- Search field -->
<z-input [zSearch]="true" zPlaceholder="Search..." zSize="sm" (zOnSearch)="onSearch($event)" />

<!-- Textarea -->
<z-input zType="textarea" zLabel="Description" [zRows]="4" [zAutoSizeContent]="true" formControlName="description" />
```

## Anti-patterns

```html
<!-- WRONG: raw input, no validation UI, no theming -->
<input type="text" class="rounded border px-2 py-1" [(ngModel)]="name" />

<!-- WRONG: invented prop, there is no zVariant on z-input -->
<z-input zVariant="filled" />

<!-- WRONG: using zType="text" + manual mask instead of the built-in number type -->
<z-input zType="text" [zMask]="'separator.2'" [(ngModel)]="amount" />
<!-- RIGHT -->
<z-input zType="number" [zDecimalPlaces]="2" [(ngModel)]="amount" />
```
