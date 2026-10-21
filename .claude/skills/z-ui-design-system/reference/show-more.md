# Show More — `z-show-more`

Selector: `z-show-more`. Wraps arbitrary projected content, clamps it to a max height with a bottom fade,
and reveals a "Show more"/"Show less" toggle button only when the content actually overflows. Never hand-roll
a `max-height` + `overflow: hidden` clamp with a manual boolean flag — `z-show-more` measures overflow via
`ResizeObserver` for you.

```html
<z-show-more [zMaxHeight]="150">
  <p>Long content that may or may not overflow...</p>
</z-show-more>
```

## Inputs

| Input             | Type                       | Default           | Notes                                                                        |
| ----------------- | -------------------------- | ----------------- | ---------------------------------------------------------------------------- |
| `class`           | `ClassValue`               | `''`              | Extra classes on the host                                                    |
| `zMaxHeight`      | `number \| string`         | `150`             | Collapsed max height in px (number) or any CSS length (string, e.g. `'40%'`) |
| `zShowMoreLabel`  | `string \| undefined`      | `undefined`       | Custom "show more" text (falls back to i18n key)                             |
| `zShowLessLabel`  | `string \| undefined`      | `undefined`       | Custom "show less" text (falls back to i18n key)                             |
| `zShowLessButton` | `boolean`                  | `true`            | If `false`, hides the toggle once expanded (one-way reveal, no collapse)     |
| `zButtonType`     | `ZButtonVariants['zType']` | `'ghost-primary'` | `z-button` `zType` used for the toggle                                       |
| `zButtonSize`     | `ZButtonVariants['zSize']` | `'sm'`            | `z-button` `zSize` used for the toggle                                       |
| `zWave`           | `boolean`                  | `false`           | Ripple effect on the toggle button                                           |
| `zBlurHeight`     | `number`                   | `64`              | Height (px) of the bottom fade-out gradient while collapsed                  |
| `zBlurColor`      | `string \| undefined`      | `undefined`       | Gradient base color; defaults to `var(--card, var(--background, #fff))`      |

## Outputs

| Output          | Payload   | Fires when                                                         |
| --------------- | --------- | ------------------------------------------------------------------ |
| `zExpandChange` | `boolean` | Toggle is clicked — `true` when expanding, `false` when collapsing |

## Examples

```html
<!-- Default clamp at 150px -->
<z-show-more>
  <div [innerHTML]="longHtmlContent"></div>
</z-show-more>

<!-- Percentage-based max height, no fade color override needed on a card -->
<z-card zTitle="Description">
  <z-show-more zMaxHeight="120px" [zBlurHeight]="40">{{ product.description }}</z-show-more>
</z-card>

<!-- One-way reveal: no "show less" once expanded -->
<z-show-more [zMaxHeight]="200" [zShowLessButton]="false" (zExpandChange)="onExpand($event)">
  <p>{{ article.body }}</p>
</z-show-more>
```

## Anti-patterns

```html
<!-- WRONG: manual clamp, no overflow detection, toggle always shown even when content fits -->
<div class="overflow-hidden" [style.max-height.px]="expanded ? 'none' : 150">
  <p>{{ content }}</p>
</div>
<button (click)="expanded = !expanded">Show more</button>

<!-- WRONG: invented input, there is no zCollapsedHeight -->
<z-show-more zCollapsedHeight="100">...</z-show-more>
```
