# QR Code — `z-qrcode`

Selector: `z-qrcode`. Renders a stylized QR code onto a `<canvas>` (dot style, eye style, gradient fill,
logo overlay, frame/banner) via the `qrcode` library, with optional download affordances. Never generate a
QR code with a raw third-party widget or `<img [src]="externalQrApiUrl">` — use `z-qrcode` so styling, DPI
scaling, and download stay consistent.

```html
<z-qrcode zValue="https://example.com" [zSize]="200" />
```

## Inputs

| Input                | Type                                                | Default      | Notes                                                                                                          |
| -------------------- | --------------------------------------------------- | ------------ | -------------------------------------------------------------------------------------------------------------- |
| `zValue`             | `string`                                            | **required** | Encoded content                                                                                                |
| `zSize`              | `number`                                            | `256`        | QR drawing size in px (square, excludes frame padding)                                                         |
| `zLevel`             | `'L' \| 'M' \| 'Q' \| 'H'`                          | `'H'`        | Error-correction level                                                                                         |
| `zFgColor`           | `string`                                            | `'#000000'`  | Foreground (module) color                                                                                      |
| `zBgColor`           | `string`                                            | `'#ffffff'`  | Background color                                                                                               |
| `zDotStyle`          | `'square' \| 'dots' \| 'rounded'`                   | `'square'`   | Module shape                                                                                                   |
| `zEyeStyle`          | `'square' \| 'circle' \| 'rounded'`                 | `'square'`   | Corner "eye" shape                                                                                             |
| `zEyeColor`          | `string`                                            | `''`         | Eye color override; empty falls back to `zFgColor` (or gradient)                                               |
| `zMargin`            | `number`                                            | `4`          | Quiet-zone margin, in QR modules                                                                               |
| `zFgGradient`        | `ZQrCodeGradient \| undefined`                      | `undefined`  | `{ type: 'linear' \| 'radial', colors: string[], angle?: number }` — overrides `zFgColor` for modules when set |
| `zLogo`              | `string`                                            | `''`         | Logo image URL, drawn centered over the code                                                                   |
| `zLogoSize`          | `number`                                            | `0.2`        | Logo width as a fraction of QR size (0.2 = 20%)                                                                |
| `zLogoBgColor`       | `string`                                            | `'#ffffff'`  | Fill behind the logo                                                                                           |
| `zLogoBgShape`       | `'circle' \| 'square' \| 'none'`                    | `'circle'`   | Shape of the logo backing plate                                                                                |
| `zLogoPadding`       | `number`                                            | `4`          | Padding (px) between backing plate and logo image                                                              |
| `zFrame`             | `'none' \| 'border' \| 'text-bottom' \| 'text-top'` | `'none'`     | Outer frame style; `text-*` adds a banner with `zFrameText`                                                    |
| `zFrameText`         | `string`                                            | `'SCAN ME'`  | Banner text for `text-bottom`/`text-top` frames                                                                |
| `zFrameColor`        | `string`                                            | `'#0f172a'`  | Frame border / banner pill color                                                                               |
| `zFramePadding`      | `number`                                            | `12`         | Padding (px) between QR and frame edge                                                                         |
| `zFrameBorderRadius` | `number \| undefined`                               | `undefined`  | Frame corner radius in px; when omitted, follows the theme's `var(--radius-lg)`                                |
| `zDownloadable`      | `boolean`                                           | `false`      | Show a hover overlay with a download button on the canvas itself                                               |
| `zDownloadButton`    | `boolean`                                           | `false`      | Show a standalone download button below the canvas                                                             |
| `zShadow`            | `boolean`                                           | `true`       | Drop shadow on the canvas wrapper                                                                              |
| `zLoading`           | `boolean`                                           | `false`      | Shows a spinner overlay instead of interaction                                                                 |

No outputs — the component is presentation-only; use `zDownloadable`/`zDownloadButton` for the built-in
download-as-PNG affordance rather than wiring your own click handler.

## Sizing

There is no `.variants.ts` / `zSize` "variant" table — `zSize` is a raw pixel number you set directly, not a
`'sm' | 'default' | 'lg'` token like other components. Pick a value that fits your layout (e.g. `120` for an
inline badge, `256` default for a standalone display, `400+` for a print/poster context).

## Examples

```html
<!-- Basic -->
<z-qrcode zValue="https://example.com" />

<!-- Branded with logo, rounded eyes/dots, gradient -->
<z-qrcode
  zValue="https://example.com/invite/abc"
  [zSize]="200"
  zLogo="/assets/logo.png"
  zDotStyle="rounded"
  zEyeStyle="rounded"
  [zFgGradient]="{ type: 'linear', colors: ['#3b82f6', '#8b5cf6'], angle: 45 }"
/>

<!-- Framed with call-to-action banner and download button -->
<z-qrcode zValue="https://example.com" zFrame="text-bottom" zFrameText="SCAN ME" [zDownloadButton]="true" />

<!-- Loading state while value is being generated server-side -->
<z-qrcode [zValue]="qrValue() ?? ''" [zLoading]="!qrValue()" />
```

## Anti-patterns

```html
<!-- WRONG: raw canvas + manual qrcode library call, loses styling/frame/logo/download support -->
<canvas #c></canvas>

<!-- WRONG: invented prop, there is no zColor shorthand -->
<z-qrcode zValue="https://example.com" zColor="blue" />

<!-- WRONG: zSize is a pixel number, not a size token -->
<z-qrcode zValue="https://example.com" zSize="lg" />
```
