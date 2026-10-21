# Skeleton — `z-skeleton`

Selector: `z-skeleton`. A static loading-placeholder shape — bar, circle, or multi-row text block — with an
optional shimmer animation. Use this for simple, fixed-shape loading placeholders. For placeholders that
auto-generate their shape from real DOM content, use [z-skeleton-auto](./skeleton-auto.md) instead. Never
hand-roll `<div class="animate-pulse bg-gray-200">`.

```html
<z-skeleton zType="text" [zRows]="3" />
<z-skeleton zType="circle" zWidth="48px" />
```

## Inputs

| Input       | Type                          | Default              | Notes                                                                                                |
| ----------- | ----------------------------- | -------------------- | ---------------------------------------------------------------------------------------------------- |
| `class`     | `ClassValue`                  | `''`                 | Extra classes on the inner container                                                                 |
| `zType`     | `'bar' \| 'circle' \| 'text'` | `'bar'`              | Shape — `bar` is a single block, `text` supports multiple rows                                       |
| `zSize`     | `'sm' \| 'default' \| 'lg'`   | `'default'`          | Only affects `circle` sizing (`z-skeleton-circle-{sm,default,lg}`); no visual effect on `bar`/`text` |
| `zWidth`    | `string`                      | `'100%'`             | CSS width                                                                                            |
| `zHeight`   | `string \| undefined`         | `undefined`          | CSS height; if unset, `bar`/`text` fill `100%` of host height                                        |
| `zRows`     | `number`                      | `1`                  | Number of stacked rows for `bar`/`text` (ignored for `circle`)                                       |
| `zGap`      | `number`                      | `8`                  | Gap in px between rows when `zRows > 1`                                                              |
| `zAnimated` | `boolean`                     | `true`               | Enable the shimmer animation                                                                         |
| `zRadius`   | `string`                      | `'var(--radius-sm)'` | Border radius of each row/shape; follows the active theme by default                                 |

Host bindings: the host element's own `width`/`height` styles are driven by `zWidth`/`zHeight` (with a
computed multi-row height when `zType="text"` and `zRows > 1`), so you generally don't need to wrap
`z-skeleton` in a sized container.

## `zType` — shape

| `zType`  | Behavior                                                                                               |
| -------- | ------------------------------------------------------------------------------------------------------ |
| `bar`    | Single rectangular block (or multiple stacked rows if `zRows > 1`)                                     |
| `text`   | Same rendering as `bar`, semantically for text-line placeholders; supports `zRows`                     |
| `circle` | Circular shape; sized by `zWidth`/`zHeight` (falls back to a square using `zWidth` if `zHeight` unset) |

There is no dedicated size/height class table beyond the `circle` compound variants (`z-skeleton-circle-sm`,
`-default`, `-lg`) — actual pixel sizing for `circle` comes from `zWidth`/`zHeight`, and `bar`/`text` sizing
is fully driven by `zWidth`/`zHeight`/`zRows`/`zGap`, not by `zSize`.

## Examples

```html
<!-- Single-line text placeholder -->
<z-skeleton zType="text" zWidth="60%" />

<!-- Multi-row paragraph placeholder -->
<z-skeleton zType="text" [zRows]="3" zHeight="1rem" [zGap]="8" />

<!-- Avatar placeholder -->
<z-skeleton zType="circle" zSize="lg" zWidth="56px" />

<!-- Static (non-animated) card placeholder -->
<z-skeleton zType="bar" zHeight="120px" zRadius="8px" [zAnimated]="false" />
```

## Anti-patterns

```html
<!-- WRONG: hand-rolled pulse animation, no theme tokens -->
<div class="h-4 w-3/5 animate-pulse rounded bg-gray-200"></div>

<!-- WRONG: invented prop, there is no zLines (it's zRows) -->
<z-skeleton zType="text" [zLines]="3"></z-skeleton>

<!-- WRONG: zSize does not control bar/text height, zHeight does -->
<z-skeleton zType="bar" zSize="lg"></z-skeleton>
```
