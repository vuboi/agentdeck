# Icon — `z-icon`

Selectors: `z-icon` (element), `[z-icon]` (attribute form, e.g. `<i z-icon zType="lucideTrash" />`). Renders
one icon from a large multi-family registry (Lucide, Phosphor, Tabler, Heroicons, Iconoir, Sax, HugeIcons,
brand logos, plus ~250 optionally-animated icons), lazy-loaded per-icon from a CDN with local-package
fallback. Never inline a raw `<svg>` by hand — use `z-icon` so you get the shared size scale, stroke-width
theming, caching, and animation support for free.

```html
<z-icon zType="lucideTrash2" />
<i z-icon zType="lucideCircleCheck" zSize="20" class="text-success" />
```

## Inputs

| Input               | Type                                                                                   | Default     | Notes                                                                                                                                                                   |
| ------------------- | -------------------------------------------------------------------------------------- | ----------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `class`             | `ClassValue`                                                                           | `''`        | Extra classes merged with size classes                                                                                                                                  |
| `zType`             | `ZIcon` (huge union of registry icon names)                                            | `undefined` | Static icon name — see naming below                                                                                                                                     |
| `zAnimatedType`     | `ZAnimatedIcon`                                                                        | `undefined` | Name of an animated icon (from `ng-animated-icons`) to render instead of `zType`                                                                                        |
| `zAnimate`          | `boolean`                                                                              | `false`     | Force the animated icon to play (only relevant with `zAnimatedType`)                                                                                                    |
| `zAnimationTrigger` | `ZIconAnimationTrigger`: `'manual' \| 'hover' \| 'focus' \| 'interaction' \| 'always'` | `'manual'`  | When the animated icon plays: `hover`/`focus`/`interaction` react to host pointer/focus/touch events, `always` loops continuously, `manual` only responds to `zAnimate` |
| `zSize`             | `ZIconVariants['zSize']` — any string `'10'`–`'100'`                                   | `'16'`      | Pixel size (also sets host `width`/`height` inline styles)                                                                                                              |
| `zStrokeWidth`      | `number`                                                                               | `2`         | Stroke width for stroke-based icon families; `0` adds `stroke-none`                                                                                                     |
| `zSvg`              | `string`                                                                               | `''`        | Escape hatch: pass a raw `<svg>...</svg>` string or a `.svg` URL/path to render a custom icon instead of a registry name (URL results are cached in IndexedDB)          |

No outputs. `zType` and `zAnimatedType`/`zSvg` are mutually exclusive selection paths — if `zAnimatedType`
resolves to a known animated icon it takes priority over `zType`; if `zSvg` resolves to something renderable
it takes priority over `zType` too.

## Picking an icon name

`ZIcon` is a big compile-time union assembled from every installed icon family package, so your editor's
autocomplete on `zType` is the source of truth — do not guess a name. Family prefixes tell you which set an
icon belongs to and roughly how to search for it:

| Prefix      | Family                                                                                                        | Example                             |
| ----------- | ------------------------------------------------------------------------------------------------------------- | ----------------------------------- |
| `lucide*`   | Lucide (default go-to set, most consistent stroke style)                                                      | `lucideTrash2`, `lucideCircleCheck` |
| `phosphor*` | Phosphor (has weight suffixes: `Bold`, `Fill`, `Light`, `Thin`, `Duotone`; no suffix = regular)               | `phosphorHeartFill`                 |
| `tabler*`   | Tabler (has a `Fill` suffix for the filled set)                                                               | `tablerHome`, `tablerHomeFill`      |
| `hero*`     | Heroicons (suffixes `Solid`, `Mini`, `Micro`; no suffix = 24px outline)                                       | `heroBellSolid`                     |
| `iconoir*`  | Iconoir (suffix `Solid`; no suffix = regular)                                                                 | `iconoirUser`                       |
| `sax*`      | Iconsax (style suffix: `Bold`, `Outline`, `Linear`, `Bulk`, `Broken`, `TwoTone`)                              | `saxHomeOutline`                    |
| `huge*`     | HugeIcons                                                                                                     | `hugeHome01`                        |
| `brand*`    | Brand/company logos (optional suffix pins a variant: `Default`, `Color`, `Light`, `Dark`, `Wordmark`, `Mono`) | `brandGithub`, `brandSlackWordmark` |

Icons are fetched on demand from a CDN per family (see
[z-icon-loader.provider.ts](../../../../libs/core-ui/components/z-icon/z-icon-loader.provider.ts)) and cached;
if the CDN fails it falls back to the locally bundled package for that family, so any valid `ZIcon` name
always renders something, worst case `lucideCircleHelp`.

## `zSize` — no fixed scale, any 10–100 value

`zIconVariants` generates a `size-<n>` Tailwind class for every integer `10`–`100` — pass the pixel size you
need as a string (`'14'`, `'16'`, `'20'`, `'24'`, ...). There is no named `sm`/`lg` scale; pick a size that
matches the surrounding text/control (e.g. `14` inside a dense badge, `16`–`20` inside default buttons, `24`+
for standalone illustrative icons).

## Animated icons

Pass `zAnimatedType` (a separate name space of ~250 kebab-case icons from `ng-animated-icons`, derived by
stripping the `Icon` suffix and kebab-casing, e.g. `HeartIcon` -> `heart`) instead of/alongside `zType` to get
a micro-interaction animation on trigger. `zAnimationTrigger="hover"` is the common case for interactive UI;
`"always"` is for persistent attention-getters (use sparingly).

## Examples

```html
<!-- Default static icon inside a button -->
<button z-button zType="ghost" zSize="sm">
  <z-icon zType="lucidePlus" zSize="16" />
</button>

<!-- Larger standalone icon -->
<z-icon zType="lucideCircleAlert" zSize="32" class="text-destructive" />

<!-- Animated icon that plays on hover -->
<z-icon zAnimatedType="heart" zAnimationTrigger="hover" zSize="20" />

<!-- Filled Phosphor variant -->
<z-icon zType="phosphorStarFill" zSize="18" />

<!-- Custom SVG passthrough -->
<z-icon [zSvg]="'/assets/custom-mark.svg'" zSize="24" />
```

## Anti-patterns

```html
<!-- WRONG: raw inline svg, no shared size scale / caching / stroke theming -->
<svg width="16" height="16" viewBox="0 0 24 24"><path d="..." /></svg>

<!-- WRONG: guessing a name outside the registry naming convention -->
<z-icon zType="trash" />

<!-- WRONG: invented size keyword, zSize takes a pixel-number string, not "md" -->
<z-icon zType="lucideTrash2" zSize="md" />
```
