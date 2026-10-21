# Scroll Area — `z-scrollarea`

Selector: `z-scrollarea`. A styled scroll container — custom themed scrollbar (via `ngx-scrollbar`) by
default, or the browser's native scrollbar with matching color/thickness styling. Never wrap content in a
plain `<div class="overflow-auto">` when you want a consistent, theme-aware scrollbar — use `z-scrollarea`.

```html
<z-scrollarea class="h-64">
  <p>Long content...</p>
</z-scrollarea>
```

## Inputs

| Input                 | Type                                   | Default      | Notes                                                                                                                                                  |
| --------------------- | -------------------------------------- | ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `class`               | `ClassValue`                           | `''`         | Extra classes merged onto the host                                                                                                                     |
| `zType`               | `'custom' \| 'native'`                 | `'custom'`   | `'custom'` renders `ngx-scrollbar` (fully styleable thumb/track); `'native'` renders a plain scrollable `<div>` with CSS-only native-scrollbar theming |
| `zOrientation`        | `'vertical' \| 'horizontal' \| 'auto'` | `'vertical'` | Scroll axis                                                                                                                                            |
| `zVisibility`         | `'hover' \| 'visible' \| 'native'`     | `'hover'`    | When the scrollbar is shown: only on hover, always visible, or fully native behavior                                                                   |
| `zSize`               | `'sm' \| 'default' \| 'lg'`            | `'default'`  | Scrollbar thickness — see table below                                                                                                                  |
| `zDisableInteraction` | `boolean`                              | `false`      | Disables drag interaction on the custom scrollbar track/thumb (`zType="custom"` only)                                                                  |
| `zSensorThrottleTime` | `number`                               | `200`        | ms throttle for the custom scrollbar's resize/content sensor (`zType="custom"` only)                                                                   |
| `zDisableSensor`      | `boolean`                              | `false`      | Disables the resize/content-change sensor entirely (`zType="custom"` only)                                                                             |

## Outputs

| Output           | Payload       | Fires                                                                                       |
| ---------------- | ------------- | ------------------------------------------------------------------------------------------- |
| `zScroll`        | `Event`       | Native scroll event on the viewport                                                         |
| `zScrollbarInit` | `NgScrollbar` | Once, after view init, with the underlying `ngx-scrollbar` instance (`zType="custom"` only) |

## Public methods (template ref)

```html
<z-scrollarea #area class="h-64">...</z-scrollarea>
<button z-button (click)="area.scrollTo({ top: 0, behavior: 'smooth' })">Top</button>
```

- `scrollTo(options: ScrollToOptions): void` — works for both `zType` values.
- `scrollToElement(selectorOrElement: string | HTMLElement, options?: { top?; left?; duration? }): void` —
  works for both `zType` values (native mode ignores `duration` beyond choosing smooth vs instant).

## `zSize` — scrollbar thickness

No `.variants.ts` CVA table — sizing is done via CSS custom properties set per size class:

| `zSize`   | `--scrollbar-size` | `--scrollbar-padding` |
| --------- | ------------------ | --------------------- |
| `sm`      | `4px`              | `2px`                 |
| `default` | `6px`              | `4px`                 |
| `lg`      | `8px`              | `6px`                 |

## Examples

```html
<!-- Vertical list with hover-only scrollbar -->
<z-scrollarea class="h-80">
  <div class="flex flex-col gap-2 p-2">...</div>
</z-scrollarea>

<!-- Horizontal chip row, always-visible scrollbar -->
<z-scrollarea zOrientation="horizontal" zVisibility="visible" class="w-full">
  <div class="flex gap-2">...</div>
</z-scrollarea>

<!-- Native scrollbar (lighter weight, no ngx-scrollbar overhead) -->
<z-scrollarea zType="native" class="h-64">...</z-scrollarea>

<!-- Programmatic scroll -->
<z-scrollarea #chatScroll class="h-96" (zScroll)="onScroll($event)">
  @for (msg of messages(); track msg.id) {
  <div>{{ msg.text }}</div>
  }
</z-scrollarea>
```

## Anti-patterns

```html
<!-- WRONG: plain overflow div, no themed/hover scrollbar behavior -->
<div class="h-64 overflow-y-auto">...</div>

<!-- WRONG: invented prop, there is no zThickness -->
<z-scrollarea [zThickness]="10">...</z-scrollarea>

<!-- WRONG: relying on outer height alone without giving the scrollarea a bounded height -->
<z-scrollarea><p>Long content...</p></z-scrollarea>
<!-- RIGHT: give it an explicit bounded height so it actually scrolls -->
<z-scrollarea class="h-64"><p>Long content...</p></z-scrollarea>
```
