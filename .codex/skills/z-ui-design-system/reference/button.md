# Button — `z-button`

Selectors: `z-button` (element), `button[z-button]`, `a[z-button]` (attribute form on a native `<button>`/`<a>`
when you need native semantics, e.g. inside a `<form>` or as a link).

```html
<button z-button zType="primary">Save</button>
<a z-button zType="outline" [routerLink]="['/settings']">Settings</a>
<z-button zType="destructive" (click)="onDelete()">Delete</z-button>
```

Never use a plain `<button class="...">` styled by hand — always add `z-button` (or use `<z-button>`), even for
a link-styled action, so you inherit focus ring, disabled state, loading state, and wave/ripple animation for
free.

## Inputs

| Input                   | Type                                        | Default     | Notes                                                                          |
| ----------------------- | ------------------------------------------- | ----------- | ------------------------------------------------------------------------------ |
| `class`                 | `ClassValue`                                | `''`        | Extra Tailwind classes, merged (not replaced) with variant classes             |
| `zType`                 | see size/variant table below                | `'default'` | Visual style                                                                   |
| `zSize`                 | `'default' \| 'xs' \| 'sm' \| 'lg' \| 'xl'` | `'default'` | Height + padding                                                               |
| `zShape`                | `'default' \| 'circle' \| 'square'`         | `'default'` | Corner radius override                                                         |
| `zLabel`                | `string`                                    | `''`        | Translated label text (uses `TranslatePipe`); if set, overrides `<ng-content>` |
| `zLoading`              | `boolean`                                   | `false`     | Shows spinner icon, disables pointer events                                    |
| `zDisabled`             | `boolean`                                   | `false`     | Disabled visual state                                                          |
| `zTypeIcon`             | `ZIcon`                                     | `undefined` | Leading icon (renders a `z-icon`)                                              |
| `zAnimatedTypeIcon`     | `ZAnimatedIcon`                             | `undefined` | Animated icon variant instead of static                                        |
| `zAnimateIcon`          | `boolean`                                   | `false`     | Enable icon animation                                                          |
| `zAnimationTriggerIcon` | `ZIconAnimationTrigger`                     | `'manual'`  | When the icon animation fires                                                  |
| `zSizeIcon`             | `ZIconVariants['zSize']`                    | `'20'`      | Icon pixel size                                                                |
| `zStrokeWidthIcon`      | `number`                                    | `2`         | Icon stroke width                                                              |
| `zWave`                 | `boolean`                                   | `true`      | Click ripple effect (auto-disabled for `zType="link"` or while loading)        |

No outputs beyond the native `(click)` on the host element — there is no `zOnClick`, don't invent one.

## `zSize` — pick by density, not habit

| `zSize`   | Height                     | Icon-only square | Use for                                          |
| --------- | -------------------------- | ---------------- | ------------------------------------------------ |
| `xs`      | `h-7` (28px)               | `size-7`         | Dense toolbars, inline table-row actions         |
| `sm`      | `h-8` (32px)               | `size-8`         | Compact forms, secondary toolbar actions         |
| `default` | `h-9` (36px)               | `size-9`         | Standard forms and page actions — default choice |
| `lg`      | `h-10` (40px)              | `size-10`        | Prominent CTAs, dialogs footers                  |
| `xl`      | `h-12` (48px), `text-base` | `size-12`        | Hero/landing CTAs only                           |

When the button contains only an icon (no text and no `zLabel`), it auto-detects icon-only mode and becomes a
square button at that size — you don't need a separate "icon button" component or manual width/height classes.

## `zType` — pick by intent, not by guessing a hex

Grouped by semantic intent; each has a plain, `outline-*`, `outline-*-soft`, `ghost-*`, and `subtle-*` variant
family (fully listed in [z-button.variants.ts](../../../../libs/core-ui/components/z-button/z-button.variants.ts) if you need one not in this table):

| Intent                     | Solid                                                                       | Outline               | Ghost               | Subtle               |
| -------------------------- | --------------------------------------------------------------------------- | --------------------- | ------------------- | -------------------- |
| Primary action             | `default` / `primary`                                                       | `outline-primary`     | `ghost-primary`     | `subtle-primary`     |
| Neutral/secondary          | `secondary`                                                                 | `outline-secondary`   | —                   | —                    |
| Destructive                | `destructive` (or `destructive-heartbeat` for an attention-pulsing confirm) | `outline-destructive` | `ghost-destructive` | `subtle-destructive` |
| Success                    | `success`                                                                   | `outline-success`     | `ghost-success`     | `subtle-success`     |
| Info                       | `info`                                                                      | `outline-info`        | `ghost-info`        | `subtle-info`        |
| Warning                    | `warning`                                                                   | `outline-warning`     | `ghost-warning`     | `subtle-warning`     |
| Neutral outline (no color) | —                                                                           | `outline`             | `ghost`             | `subtle`             |
| Text-only link             | —                                                                           | —                     | —                   | `link`               |

Rule of thumb: exactly one `default`/`primary` button per view/section (the primary action). Secondary actions
use `outline` or `ghost`. Destructive actions always use a `destructive*` variant, never `default` + red text
classes.

## Examples

```html
<!-- Primary CTA, standard size -->
<button z-button zType="primary">Save changes</button>

<!-- Icon-only, dense toolbar -->
<button z-button zType="ghost" zSize="xs" [zTypeIcon]="'lucideTrash'"></button>

<!-- Destructive confirm in a dialog footer -->
<button z-button zType="destructive" zSize="lg" (click)="confirmDelete()">Delete account</button>

<!-- Async action -->
<button z-button zType="primary" [zLoading]="isSaving()">Save</button>

<!-- Link-styled navigation -->
<a z-button zType="link" [routerLink]="['/docs']">Read the docs</a>
```

## Anti-patterns

```html
<!-- WRONG: raw button, hand-rolled styling, no focus ring / loading / a11y -->
<button class="rounded bg-blue-600 px-3 py-2 text-white">Save</button>

<!-- WRONG: invented input, zColor is not a real prop -->
<z-button zColor="blue">Save</z-button>

<!-- WRONG: guessing a size that doesn't exist -->
<z-button zSize="medium">Save</z-button>
```
