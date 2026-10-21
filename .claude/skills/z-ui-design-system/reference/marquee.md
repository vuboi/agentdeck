# Marquee — `z-marquee`

Selector: `z-marquee`. Infinitely-looping scrolling content (logos, testimonials, ticker items) via CSS
animation, not JS. Projects a single `<ng-template>` and repeats it internally — never hand-duplicate content
in the template yourself.

```html
<z-marquee [zRepeat]="8" zPauseOnHover>
  <ng-template>
    <img src="/logo1.svg" class="h-8" />
  </ng-template>
</z-marquee>
```

## Inputs

| Input           | Type                                                         | Default               | Notes                                                                                                                                     |
| --------------- | ------------------------------------------------------------ | --------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| `class`         | `ClassValue`                                                 | `''`                  | Extra classes on the host                                                                                                                 |
| `zDirection`    | `'left' \| 'right' \| 'up' \| 'down'`                        | `'left'`              | Scroll direction; `up`/`down` switch the track to vertical (host becomes `h-full flex-col`)                                               |
| `zPauseOnHover` | `boolean`                                                    | `false`               | Pauses the animation on host hover                                                                                                        |
| `zRepeat`       | `number`                                                     | `4`                   | How many times the projected template is repeated per track copy (floored, min 1) — increase until content fills the viewport with no gap |
| `zDuration`     | `number`                                                     | `60`                  | Full loop duration in seconds (lower = faster)                                                                                            |
| `zGap`          | `string`                                                     | `'1rem'`              | CSS gap between repeated items, any valid CSS length                                                                                      |
| `zFade`         | `'none' \| 'left' \| 'right' \| 'top' \| 'bottom' \| 'both'` | `'none'`              | Edge fade-out gradient overlay; `both` fades left+right when horizontal or top+bottom when vertical                                       |
| `zFadeColor`    | `string`                                                     | `'var(--background)'` | Color the fade gradient blends into — set it to match a non-default surface color behind the marquee                                      |

No outputs — purely presentational, no interaction events.

## Content projection

The component reads its content from an `<ng-template>` child (via `contentChild(TemplateRef)`), not plain
projected content — a bare `<div>` child will not render. It internally renders two full copies of the track
(for seamless looping) times `zRepeat`, so keep the template small and side-effect-free.

## No size/variant table

`z-marquee` has no `.variants.ts` file and no `zSize`/`zType` inputs — it's a layout/animation primitive, not
a themed component. All visual sizing comes from your projected content and `zGap`/`zDuration`.

## Examples

```html
<!-- Horizontal logo strip, pauses on hover -->
<z-marquee [zRepeat]="6" [zDuration]="40" zPauseOnHover zFade="both">
  <ng-template>
    <img src="/partner-a.svg" class="h-10 opacity-70" />
  </ng-template>
</z-marquee>

<!-- Vertical ticker -->
<z-marquee class="h-64" zDirection="up" [zGap]="'0.5rem'" [zDuration]="25">
  <ng-template>
    <div class="text-sm">New order #{{ orderId }}</div>
  </ng-template>
</z-marquee>

<!-- Reverse direction -->
<z-marquee zDirection="right" [zRepeat]="10">
  <ng-template>
    <span class="text-muted-foreground text-xs">BREAKING NEWS</span>
  </ng-template>
</z-marquee>
```

## Anti-patterns

```html
<!-- WRONG: plain projected content, not an ng-template — nothing renders -->
<z-marquee>
  <img src="/logo.svg" />
</z-marquee>

<!-- WRONG: invented prop, there is no zSpeed -->
<z-marquee [zSpeed]="2">...</z-marquee>

<!-- WRONG: hand-rolled infinite scroll with duplicated markup instead of zRepeat -->
<div class="flex animate-[scroll_10s_linear_infinite]">
  <img src="/logo.svg" />
  <img src="/logo.svg" />
  <img src="/logo.svg" />
</div>
```
