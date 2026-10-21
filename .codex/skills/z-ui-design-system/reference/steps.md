# Steps — `z-steps`

Selector: `z-steps`. A step/progress indicator (wizard-style) with vertical or horizontal orientation and
four visual styles. Never hand-roll a numbered-circle-plus-line progress tracker — `z-steps` handles status
derivation, responsive collapse, and an imperative control for step navigation.

```html
<z-steps
  [zSteps]="[
    { title: 'Account', description: 'Create your account' },
    { title: 'Profile', description: 'Add your details' },
    { title: 'Done' }
  ]"
  [zCurrent]="1"
/>
```

## Inputs

| Input          | Type                                        | Default      | Notes                                                                                                         |
| -------------- | ------------------------------------------- | ------------ | ------------------------------------------------------------------------------------------------------------- |
| `class`        | `ClassValue`                                | `''`         | Extra classes on the host `<ol>`                                                                              |
| `zSteps`       | `ZStepItem[]`                               | **required** | `{ title?, description?, icon?, status?, disabled?, iconSize?, iconClass? }[]`                                |
| `zCurrent`     | `number`                                    | `0`          | Index of the active step (0-based); overridden internally once `zControl`'s `setActive`/`next`/`prev` is used |
| `zOrientation` | `'vertical' \| 'horizontal'`                | `'vertical'` | Layout direction                                                                                              |
| `zType`        | `'default' \| 'inline' \| 'arrow' \| 'dot'` | `'default'`  | Visual style (horizontal orientation only — vertical only distinguishes `dot` vs. everything else)            |
| `zSize`        | `'sm' \| 'default' \| 'lg'`                 | `'default'`  | Icon/dot size and spacing                                                                                     |
| `zClickable`   | `boolean`                                   | `false`      | Enables clicking a step to emit `zOnStepClick`                                                                |
| `zShowNumber`  | `boolean`                                   | `true`       | Show the step index number when no `icon` is set and status is `pending`/`current`                            |
| `zResponsive`  | `boolean`                                   | `true`       | Auto-switch a horizontal layout to vertical below a 640px container width (via `ResizeObserver`)              |
| `zDisabled`    | `boolean`                                   | `false`      | Disables clicking and all `zControl` navigation methods                                                       |

A step's own `status` (on `ZStepItem`) always wins over the derived status; otherwise status is derived from
its index vs. `zCurrent` (`completed` / `current` / `pending`), and `error` only ever comes from an explicit
`status: 'error'`.

## Outputs

| Output         | Payload               | Fires when                                                                                                                                                            |
| -------------- | --------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `zOnStepClick` | `number` (step index) | A step is clicked, only if `zClickable` and the step isn't `disabled`/the whole component isn't `zDisabled`                                                           |
| `zControl`     | `ZStepsControl`       | Once after view init — imperative handle: `setActive(i)`, `next()`, `prev()`, `reset()`, `setDisabled(i, bool)`, `setStatus(i, status)`, `getCurrent()`, `getTotal()` |

`ZStepsControl` methods are all no-ops while `zDisabled` is `true`. Use `zControl` to drive a wizard from a
"Next"/"Back" button pair instead of manually managing a `currentIndex` signal and passing it back into
`zCurrent` — once `setActive`/`next`/`prev` has been called, internal state takes over from `zCurrent`.

## `zSize` — icon/spacing scale

| `zSize`   | Icon circle        | Icon size | Dot size      |
| --------- | ------------------ | --------- | ------------- |
| `sm`      | `w-8 h-8` (32px)   | `16`      | `w-2 h-2`     |
| `default` | `w-9 h-9` (36px)   | `20`      | `w-2.5 h-2.5` |
| `lg`      | `w-11 h-11` (44px) | `24`      | `w-3 h-3`     |

## `zType` — visual style (applies to horizontal orientation)

| `zType`   | Look                                                                           |
| --------- | ------------------------------------------------------------------------------ |
| `default` | Icon circles connected by a line, title/description centered below each icon   |
| `inline`  | Icon + title/description on one row per step, connected by a line between rows |
| `arrow`   | Solid chevron/arrow-shaped segments, filled by status color                    |
| `dot`     | Small dots with a truncated label next to each, connected by a thin line       |

Vertical orientation only distinguishes `dot` (dot + line + label) from all other types (icon circle + line +
title/description) — `inline`/`arrow` don't have distinct vertical renderings.

## Examples

```html
<!-- Horizontal wizard, arrow style -->
<z-steps [zSteps]="wizardSteps" [zCurrent]="currentIndex" zOrientation="horizontal" zType="arrow" />

<!-- Vertical timeline with clickable steps -->
<z-steps [zSteps]="timelineSteps" [zCurrent]="currentIndex" [zClickable]="true" (zOnStepClick)="goTo($event)" />

<!-- Driven by an imperative control -->
<z-steps [zSteps]="steps" (zControl)="stepsControl = $event" />
<button z-button (click)="stepsControl.prev()">Back</button>
<button z-button (click)="stepsControl.next()">Next</button>

<!-- A step with an explicit error status -->
<z-steps [zSteps]="[{ title: 'Payment', status: 'error' }, { title: 'Confirm' }]" />
```

## Anti-patterns

```html
<!-- WRONG: hand-rolled numbered circles + connecting divs -->
<div class="flex items-center">
  <div class="bg-primary flex size-8 items-center justify-center rounded-full text-white">1</div>
  <div class="bg-muted h-0.5 flex-1"></div>
</div>

<!-- WRONG: invented prop, there is no zActiveIndex (it's zCurrent) -->
<z-steps [zSteps]="steps" [zActiveIndex]="1"></z-steps>

<!-- WRONG: manually tracking a currentIndex variable when zControl already exists -->
currentIndex++;
<z-steps [zSteps]="steps" [zCurrent]="currentIndex"></z-steps>
<!-- RIGHT: use the imperative control instead -->
stepsControl.next();
```
