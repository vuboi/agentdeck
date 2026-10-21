# Button Group — `z-button-group`

Selectors: `z-button-group` (container), `[z-button-group-item]` (attribute directive on each `z-button`/button
to make it selectable within the group), `z-button-group-separator` (visual divider between items). Handles
joined borders/radii (first/last child rounding, no double borders) and single-select active-item tracking —
never hand-roll a segmented control with `flex` + manual border-radius overrides.

```html
<z-button-group [(zActive)]="view">
  <button z-button z-button-group-item zValue="list" zType="outline">List</button>
  <button z-button z-button-group-item zValue="grid" zType="outline">Grid</button>
</z-button-group>
```

## `z-button-group` Inputs

| Input          | Type                               | Default        | Notes                                                                                        |
| -------------- | ---------------------------------- | -------------- | -------------------------------------------------------------------------------------------- |
| `zClass`       | `ClassValue`                       | `''`           | Extra classes merged with variant classes (note: `zClass`, not `class`)                      |
| `zOrientation` | `'horizontal' \| 'vertical'`       | `'horizontal'` | Layout direction; also flips which side borders/radii are trimmed                            |
| `zActive`      | `model<ZButtonGroupValue \| null>` | `null`         | Two-way bindable active value — use `[(zActive)]`. `ZButtonGroupValue` is `string \| number` |

`exportAs="zButtonGroup"`.

## `z-button-group` Outputs

| Output               | Payload                     | Fires when                                                                  |
| -------------------- | --------------------------- | --------------------------------------------------------------------------- |
| `zActiveIndexChange` | `number \| null`            | The active item changes, emits its index (or `null` if deselected)          |
| `zActiveValueChange` | `ZButtonGroupValue \| null` | The active item changes, emits its resolved value (or `null` if deselected) |

Clicking the currently-active item toggles it back off (both `zActive` and the change outputs go back to `null`).

## `[z-button-group-item]` Inputs

Attribute directive — apply alongside `z-button` (or any element) inside a `z-button-group`.

| Input    | Type                                     | Default     | Notes                                                                                          |
| -------- | ---------------------------------------- | ----------- | ---------------------------------------------------------------------------------------------- |
| `zValue` | `ZButtonGroupValue` (`string \| number`) | `undefined` | Stable key used to determine active state; if omitted, falls back to the item's position index |

`exportAs="zButtonGroupItem"`. Exposes a public `index` signal (readonly) for template-ref use. Adds the host
class `z-button-group-item-active` when selected — the group's variant CSS keys off that class for
primary-colored active styling.

## `z-button-group-separator` Inputs

| Input          | Type                         | Default        | Notes                                       |
| -------------- | ---------------------------- | -------------- | ------------------------------------------- |
| `zClass`       | `ClassValue`                 | `''`           | Extra classes (note: `zClass`, not `class`) |
| `zOrientation` | `'horizontal' \| 'vertical'` | `'horizontal'` | Should match the parent group's orientation |

`exportAs="zButtonGroupSeparator"`. Renders `role="separator"` with no content — a thin `bg-border` line.

## `zOrientation`

| `zOrientation` | Group layout                                                                           | Separator     |
| -------------- | -------------------------------------------------------------------------------------- | ------------- |
| `horizontal`   | `flex-row`, left/right radii trimmed between items, left borders removed between items | `h-full w-px` |
| `vertical`     | `flex-col`, top/bottom radii trimmed between items, top borders removed between items  | `w-full h-px` |

## Theme radius

The group does not own a separate radius setting. Its buttons use the active Z-UI radius scale (`rounded-sm` for
joined edges and `rounded-lg` for a single child), so changing `--radius` or the configured theme applies to the
group automatically. Pass layout classes through `zClass`; do not replace the group with a wrapper just to restyle
its corners.

## Examples

```html
<!-- Segmented toggle, controlled -->
<z-button-group [(zActive)]="alignment">
  <button z-button z-button-group-item zValue="left" zType="outline" [zTypeIcon]="'lucideAlignLeft'"></button>
  <button z-button z-button-group-item zValue="center" zType="outline" [zTypeIcon]="'lucideAlignCenter'"></button>
  <button z-button z-button-group-item zValue="right" zType="outline" [zTypeIcon]="'lucideAlignRight'"></button>
</z-button-group>

<!-- Vertical, with separator, react to changes -->
<z-button-group zOrientation="vertical" (zActiveValueChange)="onToolChange($event)">
  <button z-button z-button-group-item zValue="pen" zType="outline">Pen</button>
  <z-button-group-separator zOrientation="vertical" />
  <button z-button z-button-group-item zValue="eraser" zType="outline">Eraser</button>
</z-button-group>

<!-- Falls back to index-based value when zValue is omitted -->
<z-button-group (zActiveIndexChange)="onIndexChange($event)">
  <button z-button z-button-group-item zType="outline">One</button>
  <button z-button z-button-group-item zType="outline">Two</button>
</z-button-group>
```

## Anti-patterns

```html
<!-- WRONG: hand-rolled segmented control, manual border-radius trimming -->
<div class="flex">
  <button class="rounded-l-md border">List</button>
  <button class="rounded-r-md border border-l-0">Grid</button>
</div>

<!-- WRONG: plain class on the group container instead of zClass -->
<z-button-group class="mt-2">...</z-button-group>

<!-- WRONG: invented input, zSelected is not real -->
<z-button-group [zSelected]="view">...</z-button-group>
```
