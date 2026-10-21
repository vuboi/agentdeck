# Avatar — `z-avatar`

Selectors: `z-avatar` (single avatar), `z-avatar-group` (stacked, overlapping avatars with overflow count).
Handles image-load failure (falls back to initials or icon), status dot, and optional name/description label —
never hand-roll an `<img>` + manual `onerror` fallback.

```html
<z-avatar zSrc="https://example.com/avatar.jpg" zName="Jane Doe" />
```

## `z-avatar` Inputs

| Input             | Type                                         | Default        | Notes                                                                 |
| ----------------- | -------------------------------------------- | -------------- | --------------------------------------------------------------------- |
| `class`           | `ClassValue`                                 | `''`           | Extra classes merged with variant classes                             |
| `zSrc`            | `string`                                     | `''`           | Image URL; falls back to initials/icon on empty or on load error      |
| `zName`           | `string`                                     | `''`           | Used for initials fallback and the label text                         |
| `zAlt`            | `string`                                     | `''`           | Image `alt`; falls back to `zName`, then `'Avatar'`                   |
| `zIcon`           | `ZIcon`                                      | `'lucideUser'` | Icon shown when `zFallback="icon"` (or no initials available)         |
| `zSize`           | `'xs' \| 'sm' \| 'default' \| 'lg' \| 'xl'`  | `'default'`    | See size table                                                        |
| `zShape`          | `'circle' \| 'rounded' \| 'square'`          | `'circle'`     | Corner radius                                                         |
| `zStatus`         | `'online' \| 'offline' \| 'busy' \| 'away'`  | `undefined`    | Shows a status dot when set                                           |
| `zRing`           | `'none' \| 'default' \| 'active' \| 'muted'` | `'none'`       | Ring/offset styling around the avatar                                 |
| `zFallback`       | `'initials' \| 'icon'`                       | `'initials'`   | Which fallback to prefer when there's no image                        |
| `zShowLabel`      | `boolean`                                    | `false`        | Renders `zName` (and `zDescription`) next to the avatar               |
| `zStatusAnimated` | `boolean`                                    | `false`        | Adds a pulsing animation to the status dot                            |
| `zInteractive`    | `boolean`                                    | `false`        | Adds hover affordance classes (`z-avatar--interactive`, `hover:z-10`) |
| `zRandomColor`    | `boolean`                                    | `false`        | Applies a stable accent color to image-less avatars based on `zName`  |
| `zDescription`    | `string`                                     | `''`           | Secondary line under the name, only shown with `zShowLabel`           |

No outputs.

## `zSize` — real dimensions

| `zSize`   | Avatar box       | Text size     | Status dot |
| --------- | ---------------- | ------------- | ---------- |
| `xs`      | `size-6` (24px)  | `text-[10px]` | `size-2`   |
| `sm`      | `size-8` (32px)  | `text-xs`     | `size-2.5` |
| `default` | `size-10` (40px) | `text-sm`     | `size-3`   |
| `lg`      | `size-12` (48px) | `text-base`   | `size-3.5` |
| `xl`      | `size-16` (64px) | `text-xl`     | `size-4`   |

## `zShape`

| `zShape`  | Class          |
| --------- | -------------- |
| `circle`  | `rounded-full` |
| `rounded` | `rounded-md`   |
| `square`  | `rounded-sm`   |

## `zRing`

| `zRing`   | Class                                                                  |
| --------- | ---------------------------------------------------------------------- |
| `none`    | (no ring)                                                              |
| `default` | `ring-2 ring-border ring-offset-2 ring-offset-background`              |
| `active`  | `ring-2 ring-primary ring-offset-2 ring-offset-background`             |
| `muted`   | `ring-2 ring-muted-foreground/25 ring-offset-2 ring-offset-background` |

## `z-avatar-group` Inputs

Renders up to `zMax` `z-avatar`s (each forced `zInteractive`), overlapping via negative spacing, plus a `+N`
overflow badge for the remainder.

| Input              | Type                       | Default     | Notes                                                                                    |
| ------------------ | -------------------------- | ----------- | ---------------------------------------------------------------------------------------- |
| `class`            | `ClassValue`               | `''`        | Extra classes on the group root                                                          |
| `zItems`           | `ZAvatarItem[]`            | `[]`        | `{ id, name, email?, src?, alt?, icon?, status? }[]` — the avatars to render             |
| `zMax`             | `number`                   | `5`         | Max avatars shown before collapsing into the overflow badge                              |
| `zSize`            | `ZAvatarSize`              | `'default'` | Passed to every child avatar and the overflow badge                                      |
| `zShape`           | `ZAvatarShape`             | `'circle'`  | Passed to every child avatar and the overflow badge                                      |
| `zRing`            | `ZAvatarRing`              | `'muted'`   | Passed to every child avatar (overflow badge always uses `'none'`)                       |
| `zRandomColor`     | `boolean`                  | `false`     | Applies stable theme colors to image-less child avatars                                  |
| `zTooltip`         | `boolean`                  | `false`     | Shows each item's name and optional email in a hover tooltip                             |
| `zTooltipTemplate` | `TemplateRef<ZAvatarItem>` | —           | Optional custom tooltip template; a projected `<ng-template let-item>` is also supported |

No outputs.

## Examples

```html
<!-- Image with graceful fallback to initials -->
<z-avatar zSrc="/broken.jpg" zName="Alex Kim" />

<!-- Icon fallback, no name -->
<z-avatar zFallback="icon" [zIcon]="'lucideUser'" zShape="rounded" />

<!-- Status dot, animated -->
<z-avatar zName="Sam Lee" zStatus="online" [zStatusAnimated]="true" />

<!-- Stable theme color when no real image is available -->
<z-avatar zName="Alex Kim" [zRandomColor]="true" />

<!-- With label + description -->
<z-avatar zName="Priya Shah" zDescription="Product Designer" [zShowLabel]="true" zRing="default" />

<!-- Group with overflow -->
<z-avatar-group
  [zItems]="[
    { id: 1, name: 'A' },
    { id: 2, name: 'B' },
    { id: 3, name: 'C', status: 'online' },
  ]"
  [zMax]="2"
/>

<!-- Borderless group with colored initials -->
<z-avatar-group [zItems]="members" zRing="none" [zRandomColor]="true" />
```

## Anti-patterns

```html
<!-- WRONG: raw <img> with manual onerror fallback -->
<img src="{{ url }}" onerror="this.src='fallback.png'" class="h-10 w-10 rounded-full" />

<!-- WRONG: invented prop, zColor is not real -->
<z-avatar zColor="blue" zName="Jane" />

<!-- WRONG: guessing a size that doesn't exist -->
<z-avatar zSize="medium" />
```
