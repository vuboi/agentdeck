# Skeleton Auto — `z-skeleton-auto`

Selector: `z-skeleton-auto`. Wraps [`boneyard-js`](https://www.npmjs.com/package/boneyard-js)'s
`<boneyard-skeleton>` to auto-generate a loading skeleton that mirrors the shape of your _actual_ projected
content (measures real DOM nodes and produces matching "bones"), instead of a fixed hand-authored shape. Use
this when the real content's layout is dynamic/unknown ahead of time. For a simple fixed-shape placeholder
(bar/circle/text), use [z-skeleton](./skeleton.md) instead — it's lighter and has no `boneyard-js` dependency.

```html
<z-skeleton-auto [zLoading]="isLoading()">
  <div class="flex flex-col gap-2">
    <h3>{{ item.title }}</h3>
    <p>{{ item.description }}</p>
  </div>
</z-skeleton-auto>
```

## Inputs

| Input                | Type                                                 | Default              | Notes                                                                                                                                                                        |
| -------------------- | ---------------------------------------------------- | -------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `class`              | `ClassValue`                                         | `''`                 | Extra classes, merged onto the boneyard `cssClass`                                                                                                                           |
| `zLoading`           | `boolean`                                            | `false`              | While `true`, shows the generated skeleton instead of the projected content                                                                                                  |
| `zName`              | `string`                                             | `'z-skeleton-auto'`  | Boneyard instance name (used for its internal caching/registration)                                                                                                          |
| `zColor`             | `string \| undefined`                                | `'#e0e0e0'`          | Bone color in light mode                                                                                                                                                     |
| `zDarkColor`         | `string \| undefined`                                | `'#374151'`          | Bone color in dark mode                                                                                                                                                      |
| `zAnimate`           | `ZSkeletonAutoAnimate` (`AnimationStyle \| boolean`) | `'shimmer'`          | Boneyard's own animation style; ignored/forced off when `zWavePreset="legacy-strong"`                                                                                        |
| `zWavePreset`        | `'boneyard' \| 'legacy-strong'`                      | `'legacy-strong'`    | `'legacy-strong'` disables boneyard's animation and applies this library's own stronger wave/shimmer CSS instead (via `z-skeleton-auto-bone-legacy` class + `zWaveDuration`) |
| `zWaveDuration`      | `number`                                             | `1.5`                | Wave duration in seconds, only relevant to the `legacy-strong` preset (exposed as `--z-skeleton-auto-wave-duration` CSS var)                                                 |
| `zInitialBones`      | `ZSkeletonAutoBones \| undefined`                    | `undefined`          | Pre-computed bones (skip the initial DOM measurement pass)                                                                                                                   |
| `zStagger`           | `number \| boolean`                                  | `false`              | Boneyard's stagger option, passed through as-is                                                                                                                              |
| `zTransition`        | `number \| boolean`                                  | `false`              | Boneyard's transition option, passed through as-is                                                                                                                           |
| `zBoneClass`         | `string`                                             | `''`                 | Extra class(es) applied to each generated bone element                                                                                                                       |
| `zFallbackRows`      | `number`                                             | `3`                  | Rows in the fallback `z-skeleton` shown before boneyard has measured anything                                                                                                |
| `zFallbackGap`       | `number`                                             | `8`                  | Gap (px) for the fallback skeleton                                                                                                                                           |
| `zFallbackRowHeight` | `string`                                             | `'12px'`             | Row height for the fallback skeleton                                                                                                                                         |
| `zFallbackRadius`    | `string`                                             | `'var(--radius-sm)'` | Border radius for the fallback skeleton; follows the active theme by default                                                                                                 |

No outputs.

## Content projection slots

| Slot     | Selector             | Purpose                                                                                                                                                                          |
| -------- | -------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Default  | (unselected content) | The real content to measure and to show once `zLoading` is `false`                                                                                                               |
| Fallback | `[fallback]`         | Content shown briefly before boneyard has measured the real DOM (defaults to a `z-skeleton zType="text"` using the `zFallback*` inputs — you don't need to supply this yourself) |

## Examples

```html
<!-- Auto-shaped skeleton for a dynamic card -->
<z-skeleton-auto [zLoading]="loading()">
  <z-card [zTitle]="item.title">
    <p>{{ item.body }}</p>
    <img [src]="item.image" />
  </z-card>
</z-skeleton-auto>

<!-- Legacy strong wave preset with custom duration -->
<z-skeleton-auto [zLoading]="loading()" zWavePreset="legacy-strong" [zWaveDuration]="2">
  <app-user-row [user]="user" />
</z-skeleton-auto>

<!-- Custom fallback shape while boneyard measures -->
<z-skeleton-auto [zLoading]="loading()" [zFallbackRows]="4" zFallbackRowHeight="16px">
  <app-comment [comment]="comment" />
</z-skeleton-auto>
```

## Anti-patterns

```html
<!-- WRONG: fixed-shape z-skeleton for genuinely dynamic/unknown content layout -->
<z-skeleton zType="text" [zRows]="3" />
<!-- content shape doesn't match the skeleton shape once loaded -->

<!-- WRONG: invented prop, there is no zSkeletonColor -->
<z-skeleton-auto zSkeletonColor="#ccc" [zLoading]="loading()"></z-skeleton-auto>

<!-- WRONG: using z-skeleton-auto for a simple fixed shape you already know (avatar circle, single bar) —
     prefer the lighter z-skeleton instead -->
<z-skeleton-auto [zLoading]="loading()"><div class="size-10 rounded-full"></div></z-skeleton-auto>
```
