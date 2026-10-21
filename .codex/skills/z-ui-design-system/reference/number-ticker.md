# Number Ticker — `z-number-ticker`

Selector: `z-number-ticker`. A number whose digits roll to their new value like an odometer. Never
animate a number by hand with a `setInterval` writing into an interpolated string.

```html
<z-number-ticker [zValue]="activeUsers()" class="text-5xl font-medium" />
```

Changing `zValue` rolls from whatever is on screen, so a live metric eases from the old figure instead
of restarting at zero. There is no dependency behind it — the wheels are the component's own, which is
why it also works under SSR and reduced motion (see below).

## Inputs

| Input                    | Type                                                | Default     | Notes                                                                                                        |
| ------------------------ | --------------------------------------------------- | ----------- | ------------------------------------------------------------------------------------------------------------ |
| `class`                  | `ClassValue`                                        | `''`        | Type scale, weight and colour all come from here — the component only sets an inline box and tabular figures |
| `zValue`                 | `number`                                            | `0`         | The value to roll to                                                                                         |
| `zStart`                 | `number`                                            | `0`         | Where the first roll starts; later changes roll from the displayed value                                     |
| `zDuration`              | `number`                                            | `1800`      | How long the roll takes, in ms                                                                               |
| `zEasing`                | `'linear' \| 'smooth' \| 'spring' \| 'bounce'`      | `'smooth'`  | Shape of the roll; `spring` and `bounce` overshoot the target digit and settle back onto it                  |
| `zFormat`                | `'decimal' \| 'currency' \| 'percent' \| 'compact'` | `'decimal'` | Maps onto `Intl.NumberFormat`                                                                                |
| `zLocale`                | `string`                                            | browser's   | Grouping separators and decimal marks                                                                        |
| `zCurrency`              | `string`                                            | `'USD'`     | Read only when `zFormat` is `'currency'`                                                                     |
| `zMinimumFractionDigits` | `number`                                            | —           | Passed to `Intl`                                                                                             |
| `zMaximumFractionDigits` | `number`                                            | —           | Passed to `Intl`. Left unset, it follows the target's own precision (see below)                              |
| `zMinimumIntegerDigits`  | `number`                                            | —           | Zero-pads the integer part; `2` renders `07`, which is what a clock segment needs                            |
| `zSignDisplay`           | `Intl.NumberFormatOptions['signDisplay']`           | —           | `'always'` keeps a `+` on positive values                                                                    |

There is deliberately no `zSize`: a ticker has no business deciding how big a number on someone's page
should be, so sizing is `class` like any other text.

## Custom properties

| Property                       | Default  | Notes                                                                         |
| ------------------------------ | -------- | ----------------------------------------------------------------------------- |
| `--z-number-ticker-line`       | `1.15em` | Height of the wheel window and of each digit cell                             |
| `--z-number-ticker-mask`       | `0.14em` | How far the depth fade reaches in from the window's edges, while rolling only |
| `--z-number-ticker-sign-shift` | `0.4em`  | How far a sign glyph slides as it swaps; `0` gives a pure crossfade           |

No outputs.

## How the roll works

Each digit position is a vertical strip of `0-9` (plus a repeated `0`, so a 9→0 wrap rolls on rather
than snapping back) inside a one-line window with `overflow: hidden`. A wheel's position is
interpolated between the digit **its own place** holds at the start and at the end:

```
offset(place) = (floor(|from| / 10^place) + steps * eased) mod 10
```

That single formula is what gives the odometer feel: the units wheel travels the whole change while the
tens wheel travels a tenth of it, so lower digits visibly spin faster — and both ends are exact,
because each end is that place's real digit. Positions are written as a `transform`, so the roll runs
on the compositor rather than relaying out the strip every frame.

The layout comes from `Intl.NumberFormat.formatToParts` of the wider of the two endpoints, so grouping
separators, decimal marks, currency symbols and signs land where the locale puts them and are rendered
as fixed text between the wheels. Pinning it to the wider endpoint keeps the wheel count — and the
width — stable for the whole roll.

Two behaviours worth knowing:

- **Precision follows the target.** Every frame of the roll is a float, and `Intl` would otherwise
  render up to three decimals of it, flashing values like `4,185.899` on the way to a whole number. With
  no explicit `zMaximumFractionDigits`, the ticker follows the target's own precision instead.
- **The mask lifts at rest.** The depth fade only exists while the wheels move; dimming the edges of a
  number that has come to rest just looks washed out. It is measured in `em` from the window's edges, so
  it eats into the window's slack rather than climbing onto the glyph.
- **A sign cell crossfades and slides.** `+` and `-` share one cell, so a sign flip neither resizes the
  line nor snaps: the outgoing glyph leaves to the left as the incoming one arrives from the right, both
  on the roll's own easing.
- **A changing digit count pops.** The wheel count is stable _within_ one roll, but if a later update
  crosses a power of ten (999 → 1000) the extra wheel appears rather than sliding in. If that matters,
  that is the one thing `number-flow` does better — at the cost of a runtime dependency on the
  published library.

Under `prefers-reduced-motion`, or on the server, no wheels are rendered at all: the component prints
the formatted number as plain text.

## Examples

```html
<!-- Count up on load -->
<z-number-ticker [zValue]="100" [zDuration]="4000" />

<!-- Currency -->
<z-number-ticker [zValue]="balance()" zFormat="currency" [zMinimumFractionDigits]="2" [zMaximumFractionDigits]="2" />

<!-- Clock segments: one ticker each, zero-padded, short enough to land before the next second -->
<div class="inline-flex items-center gap-2">
  <z-number-ticker [zValue]="hours()" [zMinimumIntegerDigits]="2" [zDuration]="400" />
  <span>:</span>
  <z-number-ticker [zValue]="minutes()" [zMinimumIntegerDigits]="2" [zDuration]="400" />
</div>

<!-- A delta: the sign is the ticker's, the colour and arrow are the surrounding markup's -->
<div class="inline-flex items-center gap-1 transition-colors" [class.text-emerald-500]="isUp()">
  <z-icon [zType]="isUp() ? 'lucideArrowUpRight' : 'lucideArrowDownRight'" zSize="40" />
  <z-number-ticker [zValue]="growth()" zFormat="percent" zSignDisplay="always" [zMaximumFractionDigits]="1" />
</div>
```

## Anti-patterns

```html
<!-- WRONG: a plain number, animated nowhere -->
<span class="text-5xl">{{ activeUsers() }}</span>

<!-- WRONG: percent takes the ratio, not the percentage — Intl multiplies by 100 -->
<z-number-ticker [zValue]="42.7" zFormat="percent" />
<!-- RIGHT -->
<z-number-ticker [zValue]="0.427" zFormat="percent" [zMaximumFractionDigits]="1" />

<!-- WRONG: a roll longer than the interval feeding it never finishes -->
<z-number-ticker [zValue]="secondsLeft()" [zDuration]="1200" />
<!-- RIGHT: land before the next tick -->
<z-number-ticker [zValue]="secondsLeft()" [zDuration]="400" />
```
