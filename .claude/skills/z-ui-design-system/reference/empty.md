# Empty — `z-empty`

Selector: `z-empty`. A centered empty/error-state placeholder — icon, message, optional description, and a
content-projection slot for an action (e.g. a "Retry" or "Create new" button). Use it inside tables, lists, and
grids whenever there's no data to show, instead of a bare "No data" text node.

```html
<z-empty zMessage="No results found" zDescription="Try adjusting your filters." />
```

## Inputs

| Input          | Type                        | Default               | Notes                                                                                                                                                            |
| -------------- | --------------------------- | --------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `class`        | `ClassValue`                | `''`                  | Extra classes merged with variant classes on the container                                                                                                       |
| `zType`        | `'empty' \| 'error'`        | `'empty'`             | Switches icon/copy/coloring to an error presentation (see below)                                                                                                 |
| `zIcon`        | `ZIcon`                     | `'lucidePackageOpen'` | Icon shown for `zType="empty"`; ignored for `zType="error"` (always `lucideServerCrash`)                                                                         |
| `zIconSize`    | `ZIconVariants['zSize']`    | `'40'`                | Icon pixel size                                                                                                                                                  |
| `zSize`        | `'sm' \| 'default' \| 'lg'` | `'default'`           | Vertical padding + text scale (see table below)                                                                                                                  |
| `zMessage`     | `string`                    | `''`                  | Primary message; falls back to i18n "No data" when empty and `zType="empty"` (ignored/overridden for `zType="error"`, which always shows the i18n error message) |
| `zDescription` | `string`                    | `''`                  | Secondary line under the message (ignored/overridden for `zType="error"`, which always shows the i18n error description)                                         |

No outputs — `z-empty` is purely presentational; wire any retry action through content projection.

## Content projection

| Slot    | Selector             | Purpose                                                             |
| ------- | -------------------- | ------------------------------------------------------------------- |
| Default | (unselected content) | Rendered below the message/description — typically an action button |

## `zSize` — padding + text scale

From `z-empty.variants.ts` (four separate `cva` calls share these variant keys — container, icon, message,
description):

| `zSize`   | Container padding | Message text | Description text |
| --------- | ----------------- | ------------ | ---------------- |
| `sm`      | `py-8`            | `text-xs`    | `text-xs`        |
| `default` | `py-12`           | `text-sm`    | `text-xs`        |
| `lg`      | `py-16`           | `text-base`  | `text-sm`        |

## `zType` — empty vs. error presentation

| `zType`           | Icon                                  | Container                                                  | Message color           | Description color          |
| ----------------- | ------------------------------------- | ---------------------------------------------------------- | ----------------------- | -------------------------- |
| `empty` (default) | `zIcon` (default `lucidePackageOpen`) | no extra background                                        | `text-muted-foreground` | `text-muted-foreground/70` |
| `error`           | forced `lucideServerCrash`            | `bg-destructive/5 rounded-lg border border-destructive/20` | `text-destructive`      | `text-destructive/60`      |

`zType="error"` always shows the i18n error message/description regardless of `zMessage`/`zDescription` inputs
— those inputs only take effect for `zType="empty"`.

## Examples

```html
<!-- Default empty state -->
<z-empty />

<!-- Custom icon/message inside a table -->
<z-empty zIcon="lucideSearchX" zMessage="No matching users" zDescription="Try a different search term." />

<!-- Error state with retry action -->
<z-empty zType="error">
  <button z-button zType="outline" zSize="sm" (click)="retry()">Retry</button>
</z-empty>

<!-- Dense empty state inside a small card -->
<z-empty zSize="sm" zMessage="Nothing here yet" />
```

## Anti-patterns

```html
<!-- WRONG: hand-rolled empty state, no consistent icon/spacing scale -->
<div class="flex flex-col items-center py-12 text-gray-400">
  <svg>...</svg>
  <p>No data</p>
</div>

<!-- WRONG: invented prop; there is no zAction input, use content projection -->
<z-empty zAction="Retry" (zActionClick)="retry()" />

<!-- WRONG: zMessage/zDescription are ignored for zType="error" -->
<z-empty zType="error" zMessage="Custom error text" />
```

```html
<!-- RIGHT: project the action button instead -->
<z-empty zType="error">
  <button z-button zType="outline" zSize="sm" (click)="retry()">Retry</button>
</z-empty>
```
