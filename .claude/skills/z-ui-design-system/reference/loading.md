# Loading — `z-loading`

Selector: `z-loading`. Renders one of 36 CSS-driven spinner styles, inline or as an overlay/fullscreen blocker,
with an optional caption. Never build a spinner by hand with a spinning `<div class="animate-spin">`.

```html
<z-loading [zLoading]="isSaving()" />

<z-loading [zLoading]="isSaving()" zSpinner="ring" zSize="lg" zText="Loading..." />
```

## Inputs

| Input          | Type                                               | Default     | Notes                                                                                                      |
| -------------- | -------------------------------------------------- | ----------- | ---------------------------------------------------------------------------------------------------------- |
| `class`        | `ClassValue`                                       | `''`        | Extra classes merged onto the spinner element                                                              |
| `zSpinner`     | one of 36 styles (below)                           | `'spinner'` | Which animation to render                                                                                  |
| `zSize`        | `'2xs' \| 'xs' \| 'default' \| 'lg' \| 'xl'`       | `'default'` | Spinner size                                                                                               |
| `zColor`       | `'primary' \| 'secondary' \| 'white' \| 'current'` | `'primary'` | Spinner/text color                                                                                         |
| `zText`        | `string`                                           | `''`        | Optional caption below the spinner                                                                         |
| `zLoading`     | `boolean`                                          | `false`     | Controls visibility; the component fades in/out on change rather than being removed by `@if` in the caller |
| `zOverlay`     | `boolean`                                          | `false`     | Render as an absolutely-positioned overlay over the nearest positioned ancestor                            |
| `zOverlayType` | `'default' \| 'dark' \| 'blur'`                    | `'dark'`    | Overlay backdrop style (only applies when `zOverlay` or `zFullscreen` is set)                              |
| `zFullscreen`  | `boolean`                                          | `false`     | Render as a fixed, viewport-covering overlay                                                               |

No outputs — `z-loading` is presentation-only, driven entirely by `zLoading`.

Important: when `zLoading()` is `false` and neither `zOverlay` nor `zFullscreen` is set, the component still
renders its DOM (opacity `0`, `pointer-events-none`) so the fade-out transition can play — it does not
disappear from the layout. Wrap it in `@if` yourself if you need it fully removed when idle.

## `zSize` — pixel scale

| `zSize`   | CVA size (`spinner` variant) |
| --------- | ---------------------------- |
| `2xs`     | `size-3`                     |
| `xs`      | `size-4`                     |
| `default` | `size-6`                     |
| `lg`      | `size-8`                     |
| `xl`      | `size-12`                    |

(Non-`spinner` styles use matching `z-loading-size-*` CSS classes defined in the component's stylesheet, not
Tailwind `size-*` utilities, but scale proportionally the same way.)

## `zSpinner` — pick a style

All 36 values (from [z-loading.define.ts](../../../../libs/core-ui/components/z-loading/z-loading.define.ts)):

`spinner` (SVG ring, default), `orbit`, `matrix`, `conic`, `polygon`, `triple`, `bar`, `beat`, `bounce`,
`circle`, `climbing-box`, `clip`, `clock`, `dot`, `fade`, `grid`, `hash`, `moon`, `pacman`, `propagate`,
`puff`, `pulse`, `ring`, `rise`, `rotate`, `scale`, `skew`, `square`, `sync`, `orbit-chase`, `bars`,
`matrix-grid`, `merge`, `encode`, `scan`, `lift`.

`spinner` is the only one that renders as an inline SVG; the rest are CSS `@keyframes`-driven `<span>`/`<i>`
structures. There's no functional difference in the API — swap `zSpinner` freely.

## Examples

```html
<!-- Inline button/section spinner -->
<z-loading [zLoading]="true" zSize="xs" />

<!-- Async panel with caption -->
<z-loading [zLoading]="isFetching()" zSpinner="dot" zText="Loading results..." />

<!-- Overlay a card while it refreshes -->
<div class="relative">
  <z-loading [zLoading]="isRefreshing()" zOverlay zOverlayType="blur" />
  ...card content...
</div>

<!-- Fullscreen app-level blocker -->
<z-loading [zLoading]="isBooting()" zFullscreen zOverlayType="dark" zText="Starting up..." />
```

## Anti-patterns

```html
<!-- WRONG: hand-rolled spinner, no consistent size/color scale -->
<div class="h-6 w-6 animate-spin rounded-full border-2 border-t-transparent"></div>

<!-- WRONG: invented prop, there is no zType on z-loading -->
<z-loading zType="dots" />

<!-- WRONG: gating with @if instead of zLoading loses the fade transition -->
@if (isSaving()) {
<z-loading [zLoading]="true" />
}
```
