# Metric Card — `z-metric-card`

Selector: `z-metric-card`. A dashboard stat tile — icon, title, big value, description, trend badge, overflow
action, and optional progress bar, with an optional decorative background graphic. Never hand-roll a stat tile
with a bare `<div class="border rounded-lg p-4">` and manual flex layout.

```html
<z-metric-card
  zTitle="Total revenue"
  [zValue]="'$45,231'"
  zDescription="vs last month"
  [zIcon]="'lucideDollarSign'"
  [zTrend]="{ value: '+12.5%', direction: 'up' }"
/>
```

## Inputs

| Input          | Type                                                                   | Default     | Notes                                                                                                                           |
| -------------- | ---------------------------------------------------------------------- | ----------- | ------------------------------------------------------------------------------------------------------------------------------- |
| `class`        | `ClassValue`                                                           | `''`        | Extra Tailwind classes, merged with variant classes                                                                             |
| `zTitle`       | `string`                                                               | `''`        | Label above the value                                                                                                           |
| `zValue`       | `string \| number`                                                     | `''`        | The headline metric                                                                                                             |
| `zDescription` | `string`                                                               | `''`        | Secondary text below the value                                                                                                  |
| `zIcon`        | `ZIcon`                                                                | `undefined` | Leading icon, rendered in a tinted badge above the title                                                                        |
| `zTrend`       | `ZMetricCardTrend`                                                     | `undefined` | `{ value: string \| number; direction?: 'up' \| 'down' \| 'neutral'; label?: string }` — renders a colored pill with trend icon |
| `zProgress`    | `number`                                                               | `undefined` | 0–100 progress value; renders a progress bar below the content when set (clamped to 0–100)                                      |
| `zAction`      | `ZMetricCardAction`                                                    | `undefined` | `{ label: string; icon?: ZIcon }` — renders a top-right icon button; `label` is used as `aria-label`                            |
| `zSize`        | `'sm' \| 'default' \| 'lg'`                                            | `'default'` | Card padding + value text size                                                                                                  |
| `zVariant`     | `'default' \| 'bordered' \| 'soft'`                                    | `'default'` | Background/border style                                                                                                         |
| `zDecor`       | `'none' \| 'wave' \| 'grid' \| 'rings' \| 'bars' \| 'dots' \| 'spark'` | `'wave'`    | Decorative background SVG in the top-right corner                                                                               |
| `zLoading`     | `boolean`                                                              | `false`     | Shows an animated skeleton in place of content                                                                                  |

## Outputs

| Output         | Payload             | Fires when                                                                       |
| -------------- | ------------------- | -------------------------------------------------------------------------------- |
| `zActionClick` | `ZMetricCardAction` | The `zAction` icon button is clicked (only rendered/wired when `zAction` is set) |

There is no click output on the card itself — only on the optional `zAction` button.

## Content projection

Default (unselected) content is projected after the value/trend block and the progress bar, so you can append
extra markup (e.g. a mini chart) below the standard layout. There are no named/attribute slots like `zCardExtra`
on this component — everything else is driven by inputs.

## `zSize` — padding + value scale

| `zSize`   | Card padding | Icon badge | Value text |
| --------- | ------------ | ---------- | ---------- |
| `sm`      | `p-4`        | `size-8`   | `text-xl`  |
| `default` | `p-5`        | `size-9`   | `text-2xl` |
| `lg`      | `p-6`        | `size-10`  | `text-3xl` |

## `zVariant` — background style

| `zVariant` | Classes                                                       | Icon badge                             | Use for                                              |
| ---------- | ------------------------------------------------------------- | -------------------------------------- | ---------------------------------------------------- |
| `default`  | `bg-card text-card-foreground border border-border shadow-sm` | `bg-primary/10 text-primary`           | Standard elevated tile, most dashboards              |
| `bordered` | `bg-background text-foreground border border-border`          | `bg-muted text-muted-foreground`       | Flat, low-emphasis tile on a colored page background |
| `soft`     | `bg-muted/45 text-foreground border border-transparent`       | `bg-background text-primary shadow-xs` | Subtle, borderless grouping inside another surface   |

## `zTrend.direction` — pill color + icon

| `direction`                    | Pill classes                                               | Icon                 |
| ------------------------------ | ---------------------------------------------------------- | -------------------- |
| `up`                           | `bg-emerald-500/10 text-emerald-600 dark:text-emerald-400` | `lucideTrendingUp`   |
| `down`                         | `bg-destructive/10 text-destructive`                       | `lucideTrendingDown` |
| `neutral` (default if omitted) | `bg-muted text-muted-foreground`                           | `lucideMinus`        |

`zTrend.value` is rendered as-is (e.g. pass `'+12.5%'` already formatted — the component does not format numbers
or add a `+`/`-` sign for you). `zTrend.label` is optional trailing text (e.g. `"vs last month"`) shown after the
value inside the same pill.

## Examples

```html
<!-- Standard revenue tile with trend -->
<z-metric-card
  zTitle="Total revenue"
  [zValue]="'$45,231.89'"
  zDescription="Compared to last month"
  [zIcon]="'lucideDollarSign'"
  [zTrend]="{ value: '+20.1%', direction: 'up' }"
/>

<!-- With progress bar and overflow action -->
<z-metric-card
  zTitle="Storage used"
  [zValue]="'42.3 GB / 100 GB'"
  [zIcon]="'lucideDatabase'"
  [zProgress]="42.3"
  [zAction]="{ label: 'More options' }"
  (zActionClick)="onMetricAction($event)"
/>

<!-- Compact soft-variant tile, no decor -->
<z-metric-card
  zTitle="Open tickets"
  [zValue]="12"
  zSize="sm"
  zVariant="soft"
  zDecor="none"
  [zTrend]="{ value: '-3', direction: 'down', label: 'this week' }"
/>

<!-- Loading skeleton state -->
<z-metric-card zTitle="Active users" [zValue]="activeUsers()" [zLoading]="isLoadingMetrics()" />
```

## Anti-patterns

```html
<!-- WRONG: hand-rolled stat tile, no theme tokens, no loading/decor support -->
<div class="rounded-lg border p-5 shadow-sm">
  <p class="text-sm text-gray-500">Total revenue</p>
  <p class="text-2xl font-bold">$45,231.89</p>
</div>

<!-- WRONG: trend as a plain string prop / manual color classes — zTrend is an object -->
<z-metric-card zTitle="Revenue" [zValue]="'$45,231'" zTrend="+20.1%" trendColor="green" />

<!-- WRONG: invented zOnClick — the card has no click output, only zActionClick tied to zAction -->
<z-metric-card zTitle="Revenue" [zValue]="'$45,231'" (zOnClick)="onClick()" />

<!-- WRONG: guessing a variant that doesn't exist -->
<z-metric-card zTitle="Revenue" [zValue]="'$45,231'" zVariant="outline" />
```
